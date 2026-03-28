use cid_base::file_path::FilePath;
use cid_base::shared_string::SharedString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    id: u64,
    name: SharedString,
    path: FilePath,
    branch_rules: Vec<BranchRule>,
    pipeline: Pipeline,
    status: RepositoryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchRule {
    branch: SharedString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pipeline {
    image: SharedString,
    steps: Vec<PipelineStep>,
    artifact_paths: Vec<FilePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineStep {
    name: SharedString,
    command: SharedString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RepositoryStatus {
    last_seen_at_ms: Option<u64>,
    last_error: Option<SharedString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredCommit {
    repository_id: u64,
    repository_name: SharedString,
    branch: SharedString,
    commit_sha: SharedString,
    discovered_at_ms: u64,
}

impl Repository {
    pub fn new(
        id: u64,
        name: impl Into<SharedString>,
        path: FilePath,
        branch_rules: Vec<BranchRule>,
        pipeline: Pipeline,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            path,
            branch_rules,
            pipeline,
            status: RepositoryStatus::default(),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn path(&self) -> &FilePath {
        &self.path
    }

    pub fn branch_rules(&self) -> &[BranchRule] {
        &self.branch_rules
    }

    pub fn pipeline(&self) -> &Pipeline {
        &self.pipeline
    }

    pub fn status(&self) -> &RepositoryStatus {
        &self.status
    }

    pub(crate) fn mark_seen(&mut self, timestamp_ms: u64) {
        self.status.last_seen_at_ms = Some(timestamp_ms);
        self.status.last_error = None;
    }

    pub(crate) fn mark_error(&mut self, message: impl Into<SharedString>) {
        self.status.last_error = Some(message.into());
    }
}

impl BranchRule {
    pub fn new(branch: impl Into<SharedString>) -> Self {
        Self {
            branch: branch.into(),
        }
    }

    pub fn branch(&self) -> &str {
        self.branch.as_str()
    }
}

impl Pipeline {
    pub fn new(
        image: impl Into<SharedString>,
        steps: Vec<PipelineStep>,
        artifact_paths: Vec<FilePath>,
    ) -> Self {
        Self {
            image: image.into(),
            steps,
            artifact_paths,
        }
    }

    pub fn image(&self) -> &str {
        self.image.as_str()
    }

    pub fn steps(&self) -> &[PipelineStep] {
        &self.steps
    }

    pub fn artifact_paths(&self) -> &[FilePath] {
        &self.artifact_paths
    }
}

impl PipelineStep {
    pub fn new(name: impl Into<SharedString>, command: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn command(&self) -> &str {
        self.command.as_str()
    }
}

impl RepositoryStatus {
    pub fn last_seen_at_ms(&self) -> Option<u64> {
        self.last_seen_at_ms
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

impl DiscoveredCommit {
    pub fn new(
        repository_id: u64,
        repository_name: impl Into<SharedString>,
        branch: impl Into<SharedString>,
        commit_sha: impl Into<SharedString>,
        discovered_at_ms: u64,
    ) -> Self {
        Self {
            repository_id,
            repository_name: repository_name.into(),
            branch: branch.into(),
            commit_sha: commit_sha.into(),
            discovered_at_ms,
        }
    }

    pub fn repository_id(&self) -> u64 {
        self.repository_id
    }

    pub fn repository_name(&self) -> &str {
        self.repository_name.as_str()
    }

    pub fn branch(&self) -> &str {
        self.branch.as_str()
    }

    pub fn commit_sha(&self) -> &str {
        self.commit_sha.as_str()
    }

    pub fn discovered_at_ms(&self) -> u64 {
        self.discovered_at_ms
    }
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;

    use super::{BranchRule, Pipeline, PipelineStep, Repository};

    #[test]
    fn repository_exposes_core_configuration() {
        let repository = Repository::new(
            7,
            "cid",
            FilePath::new("/repos/cid"),
            vec![BranchRule::new("main")],
            Pipeline::new(
                "alpine:3.20",
                vec![PipelineStep::new("test", "cargo test")],
                vec![FilePath::new("target")],
            ),
        );

        assert_eq!(repository.id(), 7);
        assert_eq!(repository.name(), "cid");
        assert_eq!(repository.path().as_str(), "/repos/cid");
        assert_eq!(repository.branch_rules()[0].branch(), "main");
        assert_eq!(repository.pipeline().image(), "alpine:3.20");
        assert_eq!(repository.pipeline().steps()[0].command(), "cargo test");
    }
}
