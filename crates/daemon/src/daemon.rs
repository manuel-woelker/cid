use std::sync::{Arc, RwLock, mpsc};
use std::time::{Duration, Instant};

use cid_base::logging::info;
use cid_base::result::{CidResult, ResultExt};
use cid_pal::pal::PalHandle;
use cid_pal::process_command::ProcessCommand;
use cid_pal::process_event::ProcessEvent;
use cid_pal::process_event_sink::ProcessEventSink;
use serde::{Deserialize, Serialize};
use tracing::debug;
use crate::config::CidConfig;
use crate::persistence::CidStateStore;
use crate::repository::{DiscoveredCommit, Repository};
use crate::run::{Run, RunSummary};
use crate::runner::DockerRunner;
use crate::scheduler::Scheduler;
use crate::watcher::RepositoryWatcher;

#[derive(Debug)]
pub struct CidDaemon {
    config: CidConfig,
    pal: PalHandle,
    store: CidStateStore,
    watcher: RepositoryWatcher,
    scheduler: Scheduler,
    runner: DockerRunner,
    state: DaemonState,
    snapshot: Arc<RwLock<DaemonState>>,
    command_sender: mpsc::Sender<DaemonCommand>,
    command_receiver: mpsc::Receiver<DaemonCommand>,
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
}

impl CidDaemon {
    pub fn from_config(config: &CidConfig, pal: PalHandle) -> CidResult<Self> {
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
            runner: DockerRunner::new(store.clone(), pal.clone()),
            scheduler: Scheduler::new(),
            watcher: RepositoryWatcher::new(pal.clone()),
            pal,
            store,
            state,
            snapshot,
            command_sender,
            command_receiver,
        })
    }

    pub fn handle(&self) -> DaemonHandle {
        DaemonHandle {
            snapshot: Arc::clone(&self.snapshot),
            command_sender: self.command_sender.clone(),
        }
    }

    pub fn run_forever(&mut self, poll_interval: std::time::Duration) -> CidResult<()> {
        loop {
            self.process_pending_commands()?;
            let report = self.run_cycle()?;
            debug!(
                repository_count = self.repositories().len(),
                discovered_commits = report.discovered_commits,
                queued_runs = report.queued_runs,
                executed_runs = report.executed_runs,
                "daemon cycle completed"
            );
            self.wait_for_next_cycle(poll_interval)?;
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

    pub fn run_cycle(&mut self) -> CidResult<RunCycleReport> {
        self.reload_externally_added_runs()?;
        let repositories = self.config.repositories(&*self.pal)?;
        sync_repositories(&mut self.state, repositories);

        let now_ms = self.now_ms();
        let discoveries = self.watcher.poll(
            &mut self.state.repositories,
            &self.state.discovered_commits,
            now_ms,
        );
        let discovered_count = discoveries.len();
        self.state.discovered_commits.extend(discoveries);

        let queued_runs = self.scheduler.enqueue_runs(
            &self.state.repositories,
            &self.state.discovered_commits,
            &mut self.state.runs,
        );

        let executed_runs = self
            .runner
            .execute_queued_runs(&self.state.repositories, &mut self.state.runs)?;
        self.reload_externally_added_runs()?;
        self.store.save(&self.state)?;
        self.publish_snapshot()?;
        self.process_pending_commands()?;

        Ok(RunCycleReport {
            discovered_commits: discovered_count,
            queued_runs,
            executed_runs,
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

    fn process_pending_commands(&mut self) -> CidResult<()> {
        while let Ok(command) = self.command_receiver.try_recv() {
            self.handle_command(command)?;
        }

        Ok(())
    }

    fn wait_for_next_cycle(&mut self, duration: Duration) -> CidResult<()> {
        let deadline = Instant::now() + duration;

        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(());
            }

            let timeout = (deadline - now).min(Duration::from_millis(100));
            match self.command_receiver.recv_timeout(timeout) {
                Ok(command) => self.handle_command(command)?,
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
        self.store.save(&self.state)?;
        self.publish_snapshot()?;

        Ok(next_run)
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

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;
    use cid_base::timestamp::Timestamp;
    use cid_pal::pal_mock::PalMock;
    use cid_pal::process_result::ProcessResult;

    use crate::repository::{BranchRule, Pipeline, PipelineStep, Repository};
    use crate::run::{Run, RunStep};

    use super::{
        DaemonState, ensure_devcontainer_cli_available, merge_missing_runs, sync_repositories,
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
}
