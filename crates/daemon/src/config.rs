use std::time::Duration;

use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use cid_pal::pal::Pal;
use serde::{Deserialize, Serialize};

use crate::repository::{BranchRule, Pipeline, PipelineStep, Repository};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CidConfig {
    #[serde(default = "default_state_dir")]
    state_dir: FilePath,
    #[serde(default)]
    web: WebConfig,
    #[serde(default = "default_poll_interval_seconds")]
    poll_interval_seconds: u64,
    repositories: Vec<RepositoryConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebConfig {
    #[serde(default = "default_web_enabled")]
    enabled: bool,
    #[serde(default = "default_web_address")]
    address: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RepositoryConfig {
    name: Option<String>,
    path: FilePath,
    #[serde(default = "default_branches")]
    branches: Vec<String>,
    #[serde(default)]
    pipeline: PipelineConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PipelineConfig {
    #[serde(default = "default_image")]
    image: String,
    #[serde(default = "default_steps")]
    steps: Vec<PipelineStepConfig>,
    #[serde(default)]
    artifact_paths: Vec<FilePath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PipelineStepConfig {
    name: String,
    command: String,
}

impl CidConfig {
    pub fn load_from_path(path: &FilePath, pal: &dyn Pal) -> CidResult<Self> {
        let contents = pal
            .read_file_to_string(path)
            .with_context(|| format!("failed to read config file `{path}`"))?;
        let config: Self = serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse config file `{path}`"))?;

        config.validate(pal)?;
        Ok(config)
    }

    pub fn state_dir(&self) -> &FilePath {
        &self.state_dir
    }

    pub fn state_file_path(&self) -> FilePath {
        self.state_dir.join("state.yaml")
    }

    pub fn web(&self) -> &WebConfig {
        &self.web
    }

    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.poll_interval_seconds)
    }

    pub fn repositories(&self) -> CidResult<Vec<Repository>> {
        self.repositories
            .iter()
            .enumerate()
            .map(|(index, repository)| repository.to_repository(index as u64 + 1))
            .collect()
    }

    fn validate(&self, pal: &dyn Pal) -> CidResult<()> {
        for repository in &self.repositories {
            repository.validate(pal)?;
        }

        Ok(())
    }
}

impl WebConfig {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn address(&self) -> &str {
        &self.address
    }
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: default_web_enabled(),
            address: default_web_address(),
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            image: default_image(),
            steps: default_steps(),
            artifact_paths: Vec::new(),
        }
    }
}

impl RepositoryConfig {
    fn validate(&self, pal: &dyn Pal) -> CidResult<()> {
        if !pal.file_exists(&self.path)? {
            return Err(cid_base::err!(
                "configured repository path does not exist: {}",
                self.path
            ));
        }

        if !pal.directory_exists(&self.path)? {
            return Err(cid_base::err!(
                "configured repository path is not a directory: {}",
                self.path
            ));
        }

        if !pal.file_exists(&self.path.join(".git"))? {
            return Err(cid_base::err!(
                "configured repository path is not a git repository: {}",
                self.path
            ));
        }

        if self.branches.is_empty() {
            return Err(cid_base::err!(
                "repository configuration must include at least one branch: {}",
                self.path
            ));
        }

        if self.pipeline.steps.is_empty() {
            return Err(cid_base::err!(
                "repository configuration must include at least one pipeline step: {}",
                self.path
            ));
        }

        Ok(())
    }

    fn to_repository(&self, id: u64) -> CidResult<Repository> {
        let name = self
            .name
            .clone()
            .unwrap_or_else(|| self.path.file_name().unwrap_or("repository").to_string());

        let branch_rules = self.branches.iter().cloned().map(BranchRule::new).collect();
        let pipeline = Pipeline::new(
            self.pipeline.image.clone(),
            self.pipeline
                .steps
                .iter()
                .map(|step| PipelineStep::new(step.name.clone(), step.command.clone()))
                .collect(),
            self.pipeline.artifact_paths.clone(),
        );

        Ok(Repository::new(
            id,
            name,
            self.path.clone(),
            branch_rules,
            pipeline,
        ))
    }
}

fn default_state_dir() -> FilePath {
    FilePath::new(".cid")
}

fn default_web_enabled() -> bool {
    true
}

fn default_web_address() -> String {
    "127.0.0.1:4000".to_string()
}

fn default_poll_interval_seconds() -> u64 {
    30
}

fn default_branches() -> Vec<String> {
    vec!["main".to_string()]
}

fn default_image() -> String {
    "alpine:3.20".to_string()
}

fn default_steps() -> Vec<PipelineStepConfig> {
    vec![PipelineStepConfig {
        name: "noop".to_string(),
        command: "echo no pipeline configured".to_string(),
    }]
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;
    use cid_pal::pal_mock::PalMock;

    use super::CidConfig;

    #[test]
    fn load_from_path_parses_repository_entries() {
        let pal = PalMock::new();
        pal.set_directory("repos/foo");
        pal.set_directory("repos/foo/.git");
        pal.set_file(
            "cid-config.yaml",
            "state_dir: state\npoll_interval_seconds: 5\nrepositories:\n  - path: repos/foo\n    branches: [main]\n    pipeline:\n      image: rust:1.85\n      steps:\n        - name: test\n          command: cargo test\n",
        );

        let config = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"), &pal).unwrap();
        let repositories = config.repositories().unwrap();

        assert_eq!(config.poll_interval().as_secs(), 5);
        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].name(), "foo");
        assert_eq!(repositories[0].pipeline().image(), "rust:1.85");
    }

    #[test]
    fn load_from_path_rejects_non_git_directories() {
        let pal = PalMock::new();
        pal.set_directory("repos/foo");
        pal.set_file("cid-config.yaml", "repositories:\n  - path: repos/foo\n");

        let error = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"), &pal).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("configured repository path is not a git repository")
        );
    }
}
