use std::time::Duration;

use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use cid_pal::pal::Pal;
use serde::{Deserialize, Serialize};

use crate::repository::{BranchRule, Pipeline, Repository};

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct UpstreamRepositoryConfig {
    #[serde(default = "default_branches")]
    branches: Vec<String>,
    #[serde(default)]
    artifact_paths: Vec<FilePath>,
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

    pub fn repositories(&self, pal: &dyn Pal) -> CidResult<Vec<Repository>> {
        self.repositories
            .iter()
            .enumerate()
            .map(|(index, repository)| repository.to_repository(pal, index as u64 + 1))
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

        let upstream = self.load_upstream_config(pal)?;
        upstream.validate(&self.path)?;

        let devcontainer_path = self.devcontainer_config_path();
        if !pal.file_exists(&devcontainer_path)? {
            return Err(cid_base::err!(
                "repository is missing .devcontainer/devcontainer.json: {}",
                self.path
            ));
        }

        let ci_script_path = self.ci_script_path();
        if !pal.file_exists(&ci_script_path)? {
            return Err(cid_base::err!(
                "repository is missing scripts/ci.sh: {}",
                self.path
            ));
        }

        Ok(())
    }

    fn to_repository(&self, pal: &dyn Pal, id: u64) -> CidResult<Repository> {
        let name = self
            .name
            .clone()
            .unwrap_or_else(|| self.path.file_name().unwrap_or("repository").to_string());

        let upstream = self.load_upstream_config(pal)?;
        let branch_rules = upstream
            .branches
            .iter()
            .cloned()
            .map(BranchRule::new)
            .collect();
        let pipeline = Pipeline::for_devcontainer(upstream.artifact_paths);

        Ok(Repository::new(
            id,
            name,
            self.path.clone(),
            branch_rules,
            pipeline,
        ))
    }

    fn load_upstream_config(&self, pal: &dyn Pal) -> CidResult<UpstreamRepositoryConfig> {
        let path = self.upstream_config_path();
        let contents = pal
            .read_file_to_string(&path)
            .with_context(|| format!("failed to read repository config file `{path}`"))?;
        serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse repository config file `{path}`"))
    }

    fn upstream_config_path(&self) -> FilePath {
        self.path.join(".cid").join("cid.yaml")
    }

    fn devcontainer_config_path(&self) -> FilePath {
        self.path.join(".devcontainer").join("devcontainer.json")
    }

    fn ci_script_path(&self) -> FilePath {
        self.path.join("scripts").join("ci.sh")
    }
}

impl UpstreamRepositoryConfig {
    fn validate(&self, repository_path: &FilePath) -> CidResult<()> {
        if self.branches.is_empty() {
            return Err(cid_base::err!(
                "repository config must include at least one branch: {}",
                repository_path
            ));
        }

        Ok(())
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
        pal.set_directory("repos/foo/.devcontainer");
        pal.set_directory("repos/foo/scripts");
        pal.set_file(
            "cid-config.yaml",
            "state_dir: state\npoll_interval_seconds: 5\nrepositories:\n  - path: repos/foo\n",
        );
        pal.set_file("repos/foo/.cid/cid.yaml", "branches: [main]\n");
        pal.set_file(
            "repos/foo/.devcontainer/devcontainer.json",
            "{\"image\":\"rust:1.85\"}",
        );
        pal.set_file("repos/foo/scripts/ci.sh", "#!/usr/bin/env bash\nnao ci\n");

        let config = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"), &pal).unwrap();
        let repositories = config.repositories(&pal).unwrap();

        assert_eq!(config.poll_interval().as_secs(), 5);
        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].name(), "foo");
        assert_eq!(repositories[0].pipeline().image(), "devcontainer");
        assert_eq!(repositories[0].pipeline().steps()[0].name(), "ci");
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

    #[test]
    fn load_from_path_reads_repository_config_from_upstream_repo() {
        let pal = PalMock::new();
        pal.set_directory("repos/foo");
        pal.set_directory("repos/foo/.git");
        pal.set_directory("repos/foo/.devcontainer");
        pal.set_directory("repos/foo/scripts");
        pal.set_file("cid-config.yaml", "repositories:\n  - path: repos/foo\n");
        pal.set_file(
            "repos/foo/.cid/cid.yaml",
            "branches: [main]\nartifact_paths:\n  - target\n",
        );
        pal.set_file(
            "repos/foo/.devcontainer/devcontainer.json",
            "{\"image\":\"rust:1.85\"}",
        );
        pal.set_file("repos/foo/scripts/ci.sh", "#!/usr/bin/env bash\nnao ci\n");

        let config = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"), &pal).unwrap();
        let repositories = config.repositories(&pal).unwrap();

        assert_eq!(repositories[0].branch_rules()[0].branch(), "main");
        assert_eq!(
            repositories[0].pipeline().artifact_paths()[0].as_str(),
            "target"
        );
    }

    #[test]
    fn load_from_path_rejects_missing_devcontainer() {
        let pal = PalMock::new();
        pal.set_directory("repos/foo");
        pal.set_directory("repos/foo/.git");
        pal.set_directory("repos/foo/scripts");
        pal.set_file("cid-config.yaml", "repositories:\n  - path: repos/foo\n");
        pal.set_file("repos/foo/.cid/cid.yaml", "branches: [main]\n");
        pal.set_file("repos/foo/scripts/ci.sh", "#!/usr/bin/env bash\nnao ci\n");

        let error = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"), &pal).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("repository is missing .devcontainer/devcontainer.json")
        );
    }

    #[test]
    fn load_from_path_rejects_missing_ci_script() {
        let pal = PalMock::new();
        pal.set_directory("repos/foo");
        pal.set_directory("repos/foo/.git");
        pal.set_directory("repos/foo/.devcontainer");
        pal.set_file("cid-config.yaml", "repositories:\n  - path: repos/foo\n");
        pal.set_file("repos/foo/.cid/cid.yaml", "branches: [main]\n");
        pal.set_file(
            "repos/foo/.devcontainer/devcontainer.json",
            "{\"image\":\"rust:1.85\"}",
        );

        let error = CidConfig::load_from_path(&FilePath::new("cid-config.yaml"), &pal).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("repository is missing scripts/ci.sh")
        );
    }
}
