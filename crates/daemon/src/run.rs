use cid_base::shared_string::SharedString;
use serde::{Deserialize, Serialize};

use crate::run_status::RunStatus;

/// Summary record for a single build run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    repository_name: SharedString,
    branch: SharedString,
    commit_sha: SharedString,
    status: RunStatus,
}

impl Run {
    /// Creates a run value for a repository, branch, commit, and status.
    pub fn new(
        repository_name: impl Into<SharedString>,
        branch: impl Into<SharedString>,
        commit_sha: impl Into<SharedString>,
        status: RunStatus,
    ) -> Self {
        Self {
            repository_name: repository_name.into(),
            branch: branch.into(),
            commit_sha: commit_sha.into(),
            status,
        }
    }

    /// Returns the repository name for this run.
    pub fn repository_name(&self) -> &str {
        self.repository_name.as_str()
    }

    /// Returns the branch name for this run.
    pub fn branch(&self) -> &str {
        self.branch.as_str()
    }

    /// Returns the commit SHA for this run.
    pub fn commit_sha(&self) -> &str {
        self.commit_sha.as_str()
    }

    /// Returns the current run status.
    pub fn status(&self) -> RunStatus {
        self.status
    }
}

#[cfg(test)]
mod tests {
    use crate::run_status::RunStatus;

    use super::Run;

    #[test]
    fn run_exposes_its_core_fields() {
        let run = Run::new("cid", "main", "abc123", RunStatus::Queued);

        assert_eq!(run.repository_name(), "cid");
        assert_eq!(run.branch(), "main");
        assert_eq!(run.commit_sha(), "abc123");
        assert_eq!(run.status(), RunStatus::Queued);
    }
}
