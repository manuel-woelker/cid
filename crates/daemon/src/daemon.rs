use std::time::{SystemTime, UNIX_EPOCH};

use cid_base::result::CidResult;
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
    pub fn from_config(config: &CidConfig) -> CidResult<Self> {
        let store = CidStateStore::new(config.state_dir().clone());
        let mut state = store.load()?;
        let repositories = config.repositories()?;
        sync_repositories(&mut state, repositories);
        store.save(&state)?;

        Ok(Self {
            runner: DockerRunner::new(store.clone()),
            scheduler: Scheduler::new(),
            watcher: RepositoryWatcher::new(),
            store,
            state,
        })
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
        let now_ms = now_ms();
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
}

impl DaemonState {
    pub fn repositories(&self) -> &[Repository] {
        &self.repositories
    }

    pub fn discovered_commits(&self) -> &[DiscoveredCommit] {
        &self.discovered_commits
    }

    pub fn runs(&self) -> &[Run] {
        &self.runs
    }
}

fn sync_repositories(state: &mut DaemonState, repositories: Vec<Repository>) {
    state.repositories = repositories;
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;

    use crate::repository::{BranchRule, Pipeline, PipelineStep, Repository};

    use super::{DaemonState, sync_repositories};

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
}
