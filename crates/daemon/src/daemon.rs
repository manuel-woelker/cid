use serde::{Deserialize, Serialize};

use crate::repository::Repository;
use crate::run::Run;

/// Broad runtime state for the local cid daemon.
///
/// This holds the first durable domain types the daemon will manage and gives
/// later watcher, scheduler, and persistence work a clear home.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CidDaemon {
    repositories: Vec<Repository>,
    runs: Vec<Run>,
}

impl CidDaemon {
    /// Creates an empty daemon state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a repository with the daemon state.
    pub fn add_repository(&mut self, repository: Repository) {
        self.repositories.push(repository);
    }

    /// Records a run in the daemon state.
    pub fn add_run(&mut self, run: Run) {
        self.runs.push(run);
    }

    /// Returns the repositories known to the daemon.
    pub fn repositories(&self) -> &[Repository] {
        &self.repositories
    }

    /// Returns the runs known to the daemon.
    pub fn runs(&self) -> &[Run] {
        &self.runs
    }
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;

    use crate::repository::Repository;
    use crate::run::Run;
    use crate::run_status::RunStatus;

    use super::CidDaemon;

    #[test]
    fn daemon_starts_with_no_repositories_or_runs() {
        let daemon = CidDaemon::new();

        assert!(daemon.repositories().is_empty());
        assert!(daemon.runs().is_empty());
    }

    #[test]
    fn daemon_tracks_registered_repositories() {
        let mut daemon = CidDaemon::new();
        let repository = Repository::new("cid", FilePath::new("/repos/cid"));

        daemon.add_repository(repository.clone());

        assert_eq!(daemon.repositories(), &[repository]);
    }

    #[test]
    fn daemon_tracks_recorded_runs() {
        let mut daemon = CidDaemon::new();
        let run = Run::new("cid", "main", "abc123", RunStatus::Queued);

        daemon.add_run(run.clone());

        assert_eq!(daemon.runs(), &[run]);
    }
}
