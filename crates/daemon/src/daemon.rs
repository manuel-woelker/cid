use std::sync::{Arc, RwLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::CidConfig;
use crate::persistence::CidStateStore;
use crate::repository::{DiscoveredCommit, Repository};
use crate::run::{Run, RunSummary};
use crate::runner::DockerRunner;
use crate::scheduler::Scheduler;
use crate::watcher::RepositoryWatcher;
use cid_base::result::{CidResult, ResultExt};
use cid_pal::pal::PalHandle;
use cid_pal::process_command::ProcessCommand;
use cid_pal::process_event::ProcessEvent;
use cid_pal::process_event_sink::ProcessEventSink;
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Debug)]
pub struct CidDaemon {
    config: CidConfig,
    pal: PalHandle,
    store: CidStateStore,
    watcher: RepositoryWatcher,
    scheduler: Scheduler,
    state: DaemonState,
    snapshot: Arc<RwLock<DaemonState>>,
    command_sender: mpsc::Sender<DaemonCommand>,
    command_receiver: mpsc::Receiver<DaemonCommand>,
    execution_request_sender: mpsc::Sender<ExecutionRequest>,
    dispatch_wakeup_pending: bool,
    execution_in_progress: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DaemonState {
    pub(crate) repositories: Vec<Repository>,
    pub(crate) discovered_commits: Vec<DiscoveredCommit>,
    pub(crate) runs: Vec<Run>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunCycleReport {
    pub discovered_commits: usize,
    pub queued_runs: usize,
    pub executed_runs: usize,
}

#[derive(Clone)]
pub struct DaemonHandle {
    snapshot: Arc<RwLock<DaemonState>>,
    command_sender: mpsc::Sender<DaemonCommand>,
}

pub trait DaemonApi {
    fn snapshot(&self) -> CidResult<DaemonState>;
    fn replay_run(&self, run_id: u64) -> CidResult<Run>;
}

enum DaemonCommand {
    ReplayRun {
        run_id: u64,
        response_sender: mpsc::Sender<CidResult<Run>>,
    },
    ExecutionFinished {
        run: Run,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExecutionRequest {
    repository: Repository,
    run: Run,
}

impl CidDaemon {
    pub fn from_config(config: &CidConfig, pal: PalHandle) -> CidResult<Self> {
        let (execution_request_sender, execution_request_receiver) = mpsc::channel();
        let daemon =
            Self::from_config_with_execution_sender(config, pal.clone(), execution_request_sender)?;
        let worker_runner = DockerRunner::new(daemon.store.clone(), pal);
        let worker_command_sender = daemon.command_sender.clone();

        thread::spawn(move || {
            run_execution_worker(
                worker_runner,
                execution_request_receiver,
                worker_command_sender,
            );
        });

        Ok(daemon)
    }

    pub fn handle(&self) -> DaemonHandle {
        DaemonHandle {
            snapshot: Arc::clone(&self.snapshot),
            command_sender: self.command_sender.clone(),
        }
    }

    pub fn run_forever(&mut self, poll_interval: std::time::Duration) -> CidResult<()> {
        let mut next_discovery_at = Instant::now();

        loop {
            let mut made_progress = self.process_pending_commands()?;

            if Instant::now() >= next_discovery_at {
                let report = self.run_discovery_cycle()?;
                debug!(
                    repository_count = self.repositories().len(),
                    discovered_commits = report.discovered_commits,
                    queued_runs = report.queued_runs,
                    executed_runs = report.executed_runs,
                    "daemon discovery cycle completed"
                );
                next_discovery_at = Instant::now() + poll_interval;
                made_progress = true;
            }

            if self.dispatch_wakeup_pending {
                let executed_run = self.dispatch_if_possible()?;
                made_progress = made_progress || executed_run;
            }

            if !made_progress {
                self.wait_for_next_event(next_discovery_at)?;
            }
        }
    }

    pub fn repositories(&self) -> &[Repository] {
        &self.state.repositories
    }

    pub fn runs(&self) -> &[Run] {
        &self.state.runs
    }

    pub fn summary(&self) -> RunSummary {
        RunSummary::from_runs(&self.state.runs)
    }

    pub fn state_file_path(&self) -> cid_base::file_path::FilePath {
        self.store.state_file_path()
    }

    pub fn run_discovery_cycle(&mut self) -> CidResult<RunCycleReport> {
        self.reload_externally_added_runs()?;
        self.sync_repositories_from_config()?;
        let discoveries = self.discover_commits();
        let discovered_count = discoveries.len();
        self.state.discovered_commits.extend(discoveries);
        let queued_runs = self.plan_runs();
        self.persist_and_publish_state()?;

        if queued_runs > 0 {
            self.wake_dispatch();
        }

        Ok(RunCycleReport {
            discovered_commits: discovered_count,
            queued_runs,
            executed_runs: 0,
        })
    }

    pub fn sleep(&self, duration: std::time::Duration) {
        self.pal.sleep(duration);
    }

    fn now_ms(&self) -> u64 {
        self.pal
            .system_time()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn reload_externally_added_runs(&mut self) -> CidResult<()> {
        let persisted_state = self.store.load()?;
        merge_missing_runs(&mut self.state, persisted_state.runs);
        Ok(())
    }

    fn publish_snapshot(&self) -> CidResult<()> {
        let mut snapshot = self
            .snapshot
            .write()
            .map_err(|_| cid_base::err!("daemon snapshot lock is poisoned"))?;
        *snapshot = self.state.clone();
        Ok(())
    }

    fn process_pending_commands(&mut self) -> CidResult<bool> {
        let mut handled_commands = false;

        while let Ok(command) = self.command_receiver.try_recv() {
            self.handle_command(command)?;
            handled_commands = true;
        }

        Ok(handled_commands)
    }

    fn wait_for_next_event(&mut self, next_discovery_at: Instant) -> CidResult<()> {
        loop {
            let now = Instant::now();
            if now >= next_discovery_at {
                return Ok(());
            }

            let timeout = (next_discovery_at - now).min(Duration::from_millis(100));
            match self.command_receiver.recv_timeout(timeout) {
                Ok(command) => {
                    self.handle_command(command)?;
                    return Ok(());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }

    fn handle_command(&mut self, command: DaemonCommand) -> CidResult<()> {
        match command {
            DaemonCommand::ReplayRun {
                run_id,
                response_sender,
            } => {
                let _ = response_sender.send(self.replay_run(run_id));
                Ok(())
            }
            DaemonCommand::ExecutionFinished { run } => self.complete_run(run),
        }
    }

    fn replay_run(&mut self, run_id: u64) -> CidResult<Run> {
        let Some(source_run) = find_run(&self.state, run_id).cloned() else {
            return Err(cid_base::err!("run not found"));
        };
        let Some(repository) = self
            .state
            .repositories()
            .iter()
            .find(|repository| repository.id() == source_run.repository_id())
            .cloned()
        else {
            return Err(cid_base::err!("repository not found"));
        };

        let next_run = replay_run_from_source(&self.state, &repository, &source_run, self.now_ms());
        self.state.push_run(next_run.clone());
        self.persist_and_publish_state()?;
        self.wake_dispatch();

        Ok(next_run)
    }

    fn sync_repositories_from_config(&mut self) -> CidResult<()> {
        let repositories = self.config.repositories(&*self.pal)?;
        sync_repositories(&mut self.state, repositories);
        Ok(())
    }

    fn discover_commits(&mut self) -> Vec<DiscoveredCommit> {
        let now_ms = self.now_ms();
        self.watcher.poll(
            &mut self.state.repositories,
            &self.state.discovered_commits,
            now_ms,
        )
    }

    fn plan_runs(&mut self) -> usize {
        self.scheduler.enqueue_runs(
            &self.state.repositories,
            &self.state.discovered_commits,
            &mut self.state.runs,
        )
    }

    fn dispatch_next_run(&mut self) -> CidResult<bool> {
        let Some((repository, run)) = self.claim_next_queued_run() else {
            self.dispatch_wakeup_pending = false;
            return Ok(false);
        };

        self.persist_and_publish_state()?;
        self.execution_request_sender
            .send(ExecutionRequest { repository, run })
            .map_err(|_| cid_base::err!("execution request channel is closed"))?;
        self.execution_in_progress = true;
        self.dispatch_wakeup_pending = false;

        Ok(true)
    }

    fn dispatch_if_possible(&mut self) -> CidResult<bool> {
        if self.execution_in_progress {
            return Ok(false);
        }

        self.dispatch_next_run()
    }

    fn persist_and_publish_state(&mut self) -> CidResult<()> {
        self.store.save(&self.state)?;
        self.publish_snapshot()
    }

    fn wake_dispatch(&mut self) {
        self.dispatch_wakeup_pending = true;
    }

    fn claim_next_queued_run(&mut self) -> Option<(Repository, Run)> {
        let next_run_index = self
            .state
            .runs
            .iter()
            .position(|run| run.status() == crate::run_status::RunStatus::Queued)?;
        let repository = self
            .state
            .repositories()
            .iter()
            .find(|repository| repository.id() == self.state.runs[next_run_index].repository_id())
            .cloned()?;

        let started_at_ms = self.now_ms();
        self.state.runs[next_run_index].start(started_at_ms);
        let run = self.state.runs[next_run_index].clone();

        Some((repository, run))
    }

    fn complete_run(&mut self, completed_run: Run) -> CidResult<()> {
        if let Some(run) = self
            .state
            .runs
            .iter_mut()
            .find(|run| run.id() == completed_run.id())
        {
            *run = completed_run;
        }

        self.execution_in_progress = false;
        if has_queued_runs(&self.state) {
            self.wake_dispatch();
        }
        self.persist_and_publish_state()
    }

    fn from_config_with_execution_sender(
        config: &CidConfig,
        pal: PalHandle,
        execution_request_sender: mpsc::Sender<ExecutionRequest>,
    ) -> CidResult<Self> {
        ensure_devcontainer_cli_available(&*pal)?;
        let store = CidStateStore::new(config.state_dir().clone());
        let mut state = store.load()?;
        let repositories = config.repositories(&*pal)?;
        sync_repositories(&mut state, repositories);
        store.save(&state)?;
        let snapshot = Arc::new(RwLock::new(state.clone()));
        let (command_sender, command_receiver) = mpsc::channel();

        Ok(Self {
            config: config.clone(),
            scheduler: Scheduler::new(),
            watcher: RepositoryWatcher::new(pal.clone()),
            pal,
            store,
            state,
            snapshot,
            command_sender,
            command_receiver,
            execution_request_sender,
            dispatch_wakeup_pending: false,
            execution_in_progress: false,
        })
    }
}

fn ensure_devcontainer_cli_available(pal: &dyn cid_pal::pal::Pal) -> CidResult<()> {
    let result = pal
        .run_process(
            &ProcessCommand {
                executable: "devcontainer".into(),
                arguments: vec!["--version".into()],
                working_directory: None,
                environment: Vec::new(),
            },
            &mut NullProcessSink,
        )
        .context("failed to execute `devcontainer --version`")?;

    if result.exit_code != Some(0) {
        return Err(cid_base::err!(
            "`cid` requires a working Dev Container CLI on the host; `devcontainer --version` failed"
        ));
    }

    Ok(())
}

struct NullProcessSink;

impl ProcessEventSink for NullProcessSink {
    fn handle_event(&mut self, _event: ProcessEvent) -> CidResult<()> {
        Ok(())
    }
}

impl DaemonState {
    pub fn new(
        repositories: Vec<Repository>,
        discovered_commits: Vec<DiscoveredCommit>,
        runs: Vec<Run>,
    ) -> Self {
        Self {
            repositories,
            discovered_commits,
            runs,
        }
    }

    pub fn repositories(&self) -> &[Repository] {
        &self.repositories
    }

    pub fn discovered_commits(&self) -> &[DiscoveredCommit] {
        &self.discovered_commits
    }

    pub fn runs(&self) -> &[Run] {
        &self.runs
    }

    pub fn push_run(&mut self, run: Run) {
        self.runs.push(run);
    }
}

fn has_queued_runs(state: &DaemonState) -> bool {
    state
        .runs()
        .iter()
        .any(|run| run.status() == crate::run_status::RunStatus::Queued)
}

impl DaemonApi for DaemonHandle {
    fn snapshot(&self) -> CidResult<DaemonState> {
        self.snapshot
            .read()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| cid_base::err!("daemon snapshot lock is poisoned"))
    }

    fn replay_run(&self, run_id: u64) -> CidResult<Run> {
        let (response_sender, response_receiver) = mpsc::channel();
        self.command_sender
            .send(DaemonCommand::ReplayRun {
                run_id,
                response_sender,
            })
            .map_err(|_| cid_base::err!("daemon command channel is closed"))?;
        response_receiver
            .recv()
            .map_err(|_| cid_base::err!("daemon command response channel is closed"))?
    }
}

fn sync_repositories(state: &mut DaemonState, repositories: Vec<Repository>) {
    let previous_repositories = state.repositories.clone();
    let mut next_repositories = repositories;

    for repository in &mut next_repositories {
        if let Some(previous) = previous_repositories
            .iter()
            .find(|candidate| candidate.id() == repository.id())
        {
            if let Some(last_seen_at_ms) = previous.status().last_seen_at_ms() {
                repository.mark_seen(last_seen_at_ms);
            }

            if let Some(last_error) = previous.status().last_error() {
                repository.mark_error(last_error.to_string());
            }
        }
    }

    state.repositories = next_repositories;
}

fn find_run(state: &DaemonState, run_id: u64) -> Option<&Run> {
    state.runs().iter().find(|run| run.id() == run_id)
}

fn merge_missing_runs(state: &mut DaemonState, runs: Vec<Run>) {
    for run in runs {
        if state.runs.iter().any(|existing| existing.id() == run.id()) {
            continue;
        }

        state.runs.push(run);
    }

    state.runs.sort_by_key(Run::id);
}

fn replay_run_from_source(
    state: &DaemonState,
    repository: &Repository,
    source_run: &Run,
    queued_at_ms: u64,
) -> Run {
    let next_run_id = state.runs().iter().map(Run::id).max().unwrap_or(0) + 1;

    Run::new(
        next_run_id,
        source_run.repository_id(),
        source_run.repository_name(),
        source_run.branch(),
        source_run.commit_sha(),
        queued_at_ms,
        repository
            .pipeline()
            .steps()
            .iter()
            .map(|step| {
                crate::run::RunStep::new(
                    step.name(),
                    step.command(),
                    repository.pipeline().image(),
                    repository.pipeline().artifact_paths().to_vec(),
                )
            })
            .collect(),
    )
}

fn run_execution_worker(
    runner: DockerRunner,
    execution_request_receiver: mpsc::Receiver<ExecutionRequest>,
    command_sender: mpsc::Sender<DaemonCommand>,
) {
    while let Ok(request) = execution_request_receiver.recv() {
        let completed_run = runner.execute_claimed_run(&request.repository, request.run);
        if command_sender
            .send(DaemonCommand::ExecutionFinished { run: completed_run })
            .is_err()
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use cid_base::file_path::FilePath;
    use cid_base::timestamp::Timestamp;
    use cid_pal::pal::PalHandle;
    use cid_pal::pal_mock::PalMock;
    use cid_pal::process_command::ProcessCommand;
    use cid_pal::process_event::ProcessEvent;
    use cid_pal::process_output_event::ProcessOutputEvent;
    use cid_pal::process_output_stream::ProcessOutputStream;
    use cid_pal::process_result::ProcessResult;

    use crate::config::CidConfig;
    use crate::repository::{BranchRule, Pipeline, PipelineStep, Repository};
    use crate::run::{Run, RunStep};
    use crate::run_status::RunStatus;

    use super::{
        CidDaemon, DaemonApi, DaemonCommand, DaemonState, ExecutionRequest,
        ensure_devcontainer_cli_available, merge_missing_runs, sync_repositories,
    };

    #[test]
    fn repository_sync_replaces_in_memory_registry() {
        let mut state = DaemonState::default();
        let repositories = vec![Repository::new(
            1,
            "cid",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::new(
                "rust:1.85",
                vec![PipelineStep::new("test", "cargo test")],
                Vec::new(),
            ),
        )];

        sync_repositories(&mut state, repositories.clone());

        assert_eq!(state.repositories(), repositories);
    }

    #[test]
    fn repository_sync_preserves_repository_status() {
        let mut existing = Repository::new(
            1,
            "cid",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::new(
                "alpine:3.20",
                vec![PipelineStep::new("test", "echo old")],
                Vec::new(),
            ),
        );
        existing.mark_seen(123);
        existing.mark_error("watch failed");

        let mut state = DaemonState::new(vec![existing], Vec::new(), Vec::new());
        let repositories = vec![Repository::new(
            1,
            "cid",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::new(
                "rust:1.85",
                vec![PipelineStep::new("test", "cargo test")],
                Vec::new(),
            ),
        )];

        sync_repositories(&mut state, repositories);

        assert_eq!(state.repositories()[0].pipeline().image(), "rust:1.85");
        assert_eq!(
            state.repositories()[0].status().last_seen_at_ms(),
            Some(123)
        );
        assert_eq!(
            state.repositories()[0].status().last_error(),
            Some("watch failed")
        );
    }

    #[test]
    fn startup_validation_requires_devcontainer_cli() {
        let pal = PalMock::new();
        pal.set_process_execution(
            cid_pal::process_command::ProcessCommand {
                executable: "devcontainer".into(),
                arguments: vec!["--version".into()],
                working_directory: None,
                environment: Vec::new(),
            },
            Vec::new(),
            ProcessResult {
                started_at: Timestamp::new(0),
                finished_at: Timestamp::new(1),
                exit_code: Some(0),
            },
        );

        ensure_devcontainer_cli_available(&pal).unwrap();
    }

    #[test]
    fn startup_validation_fails_when_devcontainer_cli_is_unavailable() {
        let pal = PalMock::new();

        let error = ensure_devcontainer_cli_available(&pal).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("failed to execute `devcontainer --version`")
        );
    }

    #[test]
    fn merge_missing_runs_keeps_existing_runs_and_adds_external_replays() {
        let mut state = DaemonState::new(
            Vec::new(),
            Vec::new(),
            vec![Run::new(
                2,
                1,
                "cid",
                "main",
                "existing",
                200,
                vec![RunStep::new("test", "cargo test", "rust:1.85", Vec::new())],
            )],
        );

        merge_missing_runs(
            &mut state,
            vec![
                Run::new(
                    1,
                    1,
                    "cid",
                    "main",
                    "older",
                    100,
                    vec![RunStep::new("test", "cargo test", "rust:1.85", Vec::new())],
                ),
                Run::new(
                    2,
                    1,
                    "cid",
                    "main",
                    "stale-duplicate",
                    200,
                    vec![RunStep::new("test", "cargo test", "rust:1.85", Vec::new())],
                ),
            ],
        );

        assert_eq!(state.runs().len(), 2);
        assert_eq!(state.runs()[0].id(), 1);
        assert_eq!(state.runs()[1].id(), 2);
        assert_eq!(state.runs()[1].commit_sha(), "existing");
    }

    #[test]
    fn replay_dispatches_without_waiting_for_poll_interval() {
        let (mut daemon, _pal, _state_dir, _repository_path, execution_request_receiver) =
            test_daemon();
        add_completed_source_run(&mut daemon, "existing");

        let replayed_run = daemon.replay_run(1).unwrap();

        assert_eq!(replayed_run.status(), RunStatus::Queued);
        assert!(daemon.dispatch_wakeup_pending);
        assert_eq!(
            daemon.handle().snapshot().unwrap().runs()[1].status(),
            RunStatus::Queued
        );

        assert!(daemon.dispatch_if_possible().unwrap());
        let execution_request = recv_execution_request(&execution_request_receiver);

        let running_run = daemon
            .state
            .runs()
            .iter()
            .find(|run| run.id() == replayed_run.id())
            .unwrap();
        assert_eq!(running_run.status(), RunStatus::Running);
        assert_eq!(
            daemon.handle().snapshot().unwrap().runs()[1].status(),
            RunStatus::Running
        );

        finish_execution_request(&mut daemon, execution_request, 500);
        let finished_run = daemon
            .state
            .runs()
            .iter()
            .find(|run| run.id() == replayed_run.id())
            .unwrap();
        assert!(finished_run.status().is_finished());
        assert!(
            daemon
                .handle()
                .snapshot()
                .unwrap()
                .runs()
                .iter()
                .any(|run| run.id() == replayed_run.id() && run.status().is_finished())
        );
    }

    #[test]
    fn commit_discovery_continues_while_execution_is_busy() {
        let (mut daemon, pal, _state_dir, repository_path, execution_request_receiver) =
            test_daemon();
        add_completed_source_run(&mut daemon, "existing");
        register_git_head(&pal, &repository_path, "newcommit1");

        let replayed_run = daemon.replay_run(1).unwrap();
        assert!(daemon.dispatch_if_possible().unwrap());
        let execution_request = recv_execution_request(&execution_request_receiver);

        assert!(daemon.execution_in_progress);
        let report = daemon.run_discovery_cycle().unwrap();

        assert_eq!(report.discovered_commits, 1);
        assert_eq!(report.queued_runs, 1);
        assert!(daemon.execution_in_progress);
        assert!(
            daemon
                .state
                .runs()
                .iter()
                .any(|run| run.id() == replayed_run.id() && run.status() == RunStatus::Running)
        );
        assert!(
            daemon
                .state
                .runs()
                .iter()
                .any(|run| run.commit_sha() == "newcommit1" && run.status() == RunStatus::Queued)
        );

        finish_execution_request(&mut daemon, execution_request, 500);
    }

    #[test]
    fn dispatcher_does_not_double_start_and_wakes_when_capacity_returns() {
        let (mut daemon, _pal, _state_dir, _repository_path, execution_request_receiver) =
            test_daemon();
        add_completed_source_run(&mut daemon, "existing");
        add_completed_source_run(&mut daemon, "other");

        let first_replay = daemon.replay_run(1).unwrap();
        let second_replay = daemon.replay_run(2).unwrap();

        assert!(daemon.dispatch_if_possible().unwrap());
        let first_execution_request = recv_execution_request(&execution_request_receiver);
        assert!(daemon.execution_in_progress);
        assert!(!daemon.dispatch_if_possible().unwrap());
        assert_eq!(
            daemon
                .state
                .runs()
                .iter()
                .filter(|run| run.status() == RunStatus::Running)
                .count(),
            1
        );
        assert!(
            daemon
                .state
                .runs()
                .iter()
                .any(|run| run.id() == second_replay.id() && run.status() == RunStatus::Queued)
        );

        finish_execution_request(&mut daemon, first_execution_request, 500);

        assert!(!daemon.execution_in_progress);
        assert!(daemon.dispatch_wakeup_pending);
        assert!(
            daemon
                .state
                .runs()
                .iter()
                .any(|run| run.id() == first_replay.id() && run.status().is_finished())
        );
        assert!(daemon.dispatch_if_possible().unwrap());
        let second_execution_request = recv_execution_request(&execution_request_receiver);
        assert!(second_execution_request.run.id() == second_replay.id());
    }

    fn test_daemon() -> (
        CidDaemon,
        PalMock,
        String,
        FilePath,
        mpsc::Receiver<ExecutionRequest>,
    ) {
        let pal = PalMock::new();
        let state_dir = temp_state_dir("daemon-runtime-state");
        let repository_path = FilePath::new(temp_state_dir("daemon-runtime-repo"));
        let config_path = FilePath::new("cid-config.yaml");
        let (execution_request_sender, execution_request_receiver) = mpsc::channel();

        seed_repository_config(&pal, &config_path, &state_dir, &repository_path);
        register_devcontainer_version_check(&pal);

        let config = CidConfig::load_from_path(&config_path, &pal).unwrap();
        let daemon = CidDaemon::from_config_with_execution_sender(
            &config,
            PalHandle::new(pal.clone()),
            execution_request_sender,
        )
        .unwrap();

        (
            daemon,
            pal,
            state_dir,
            repository_path,
            execution_request_receiver,
        )
    }

    fn seed_repository_config(
        pal: &PalMock,
        config_path: &FilePath,
        state_dir: &str,
        repository_path: &FilePath,
    ) {
        pal.set_directory(repository_path.as_str());
        pal.set_directory(repository_path.join(".git").as_str());
        pal.set_directory(repository_path.join(".cid").as_str());
        pal.set_directory(repository_path.join(".devcontainer").as_str());
        pal.set_directory(repository_path.join("scripts").as_str());
        pal.set_file(
            config_path.as_str(),
            format!(
                "state_dir: {state_dir}\npoll_interval_seconds: 30\nrepositories:\n  - name: cid\n    path: {}\n",
                repository_path.as_str(),
            ),
        );
        pal.set_file(
            repository_path.join(".cid").join("cid.yaml").as_str(),
            "branches: [main]\n",
        );
        pal.set_file(
            repository_path
                .join(".devcontainer")
                .join("devcontainer.json")
                .as_str(),
            "{\"image\":\"rust:1.85\"}",
        );
        pal.set_file(
            repository_path.join("scripts").join("ci.sh").as_str(),
            "#!/usr/bin/env bash\ncargo test\n",
        );
    }

    fn register_devcontainer_version_check(pal: &PalMock) {
        pal.set_process_execution(
            ProcessCommand {
                executable: "devcontainer".into(),
                arguments: vec!["--version".into()],
                working_directory: None,
                environment: Vec::new(),
            },
            Vec::new(),
            ProcessResult {
                started_at: Timestamp::new(0),
                finished_at: Timestamp::new(1),
                exit_code: Some(0),
            },
        );
    }

    fn register_git_head(pal: &PalMock, repository_path: &FilePath, commit_sha: &str) {
        pal.set_process_execution(
            ProcessCommand {
                executable: "git".into(),
                arguments: vec!["rev-parse".into(), "refs/heads/main".into()],
                working_directory: Some(repository_path.clone()),
                environment: Vec::new(),
            },
            vec![ProcessEvent::Output(ProcessOutputEvent {
                timestamp: Timestamp::new(10),
                stream: ProcessOutputStream::Stdout,
                bytes: format!("{commit_sha}\n").into_bytes(),
            })],
            ProcessResult {
                started_at: Timestamp::new(10),
                finished_at: Timestamp::new(11),
                exit_code: Some(0),
            },
        );
    }

    fn add_completed_source_run(daemon: &mut CidDaemon, commit_sha: &str) {
        let repository = daemon.repositories()[0].clone();
        let next_id = daemon.state.runs().iter().map(Run::id).max().unwrap_or(0) + 1;
        let mut run = Run::new(
            next_id,
            repository.id(),
            repository.name(),
            "main",
            commit_sha,
            next_id,
            vec![RunStep::new(
                "ci",
                "./scripts/ci.sh",
                "devcontainer",
                Vec::new(),
            )],
        );
        run.finish(next_id + 1, RunStatus::Passed);
        daemon.state.push_run(run);
        daemon.persist_and_publish_state().unwrap();
    }

    fn recv_execution_request(
        execution_request_receiver: &mpsc::Receiver<ExecutionRequest>,
    ) -> ExecutionRequest {
        execution_request_receiver
            .recv_timeout(Duration::from_millis(200))
            .unwrap()
    }

    fn finish_execution_request(
        daemon: &mut CidDaemon,
        execution_request: ExecutionRequest,
        finished_at_ms: u64,
    ) {
        let mut completed_run = execution_request.run;
        completed_run.finish(finished_at_ms, RunStatus::Passed);
        daemon
            .handle_command(DaemonCommand::ExecutionFinished { run: completed_run })
            .unwrap();
    }

    fn temp_state_dir(prefix: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("cid-{prefix}-{unique}"))
            .to_string_lossy()
            .to_string()
    }
}
