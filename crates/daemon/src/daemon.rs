use cid_base::logging::info;
use cid_base::result::{CidResult, ResultExt};
use cid_pal::pal::PalHandle;
use cid_pal::process_command::ProcessCommand;
use cid_pal::process_event::ProcessEvent;
use cid_pal::process_event_sink::ProcessEventSink;
use serde::{Deserialize, Serialize};

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

impl CidDaemon {
    pub fn from_config(config: &CidConfig, pal: PalHandle) -> CidResult<Self> {
        ensure_devcontainer_cli_available(&*pal)?;
        let store = CidStateStore::new(config.state_dir().clone());
        let mut state = store.load()?;
        let repositories = config.repositories(&*pal)?;
        sync_repositories(&mut state, repositories);
        store.save(&state)?;

        Ok(Self {
            config: config.clone(),
            runner: DockerRunner::new(store.clone(), pal.clone()),
            scheduler: Scheduler::new(),
            watcher: RepositoryWatcher::new(pal.clone()),
            pal,
            store,
            state,
        })
    }

    pub fn run_forever(&mut self, poll_interval: std::time::Duration) -> CidResult<()> {
        loop {
            let report = self.run_cycle()?;
            info!(
                repository_count = self.repositories().len(),
                discovered_commits = report.discovered_commits,
                queued_runs = report.queued_runs,
                executed_runs = report.executed_runs,
                "daemon cycle completed"
            );
            self.sleep(poll_interval);
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
        self.store.save(&self.state)?;

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

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;
    use cid_base::timestamp::Timestamp;
    use cid_pal::pal_mock::PalMock;
    use cid_pal::process_result::ProcessResult;

    use crate::repository::{BranchRule, Pipeline, PipelineStep, Repository};

    use super::{DaemonState, ensure_devcontainer_cli_available, sync_repositories};

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
}
