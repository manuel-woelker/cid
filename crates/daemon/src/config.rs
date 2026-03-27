use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::repository::Repository;

/// Daemon configuration loaded from `cid-config.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CidConfig {
    repositories: Vec<RepositoryConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RepositoryConfig {
    path: FilePath,
}

impl CidConfig {
    /// Loads and validates daemon configuration from the given YAML file.
    pub fn load_from_path(path: &FilePath) -> CidResult<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file `{path}`"))?;
        let config: Self = serde_yaml::from_str(&contents)
            .with_context(|| format!("failed to parse config file `{path}`"))?;

        config.validate()?;

        Ok(config)
    }

    /// Returns the configured repositories as daemon domain values.
    pub fn repositories(&self) -> CidResult<Vec<Repository>> {
        self.repositories
            .iter()
            .map(RepositoryConfig::to_repository)
            .collect()
    }

    fn validate(&self) -> CidResult<()> {
        for repository in &self.repositories {
            repository.validate()?;
        }

        Ok(())
    }
}

impl RepositoryConfig {
    fn validate(&self) -> CidResult<()> {
        let path = self.path.as_path();

        if !path.exists() {
            return Err(cid_base::err!(
                "configured repository path does not exist: {}",
                self.path
            ));
        }

        if !path.is_dir() {
            return Err(cid_base::err!(
                "configured repository path is not a directory: {}",
                self.path
            ));
        }

        let git_path = path.join(".git");
        if !git_path.exists() {
            return Err(cid_base::err!(
                "configured repository path is not a git repository: {}",
                self.path
            ));
        }

        Ok(())
    }

    fn to_repository(&self) -> CidResult<Repository> {
        self.validate()?;

        let name = self.path.file_name().ok_or_else(|| {
            cid_base::err!(
                "failed to derive repository name from configured path: {}",
                self.path
            )
        })?;

        Ok(Repository::new(name, self.path.clone()))
    }
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::CidConfig;

    fn temp_test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("cid-{name}-{unique}"))
    }

    #[test]
    fn load_from_path_parses_repository_entries() {
        let root = temp_test_dir("config-parse");
        let repo_path = root.join("foo");
        fs::create_dir_all(repo_path.join(".git")).unwrap();

        let config_path = root.join("cid-config.yaml");
        fs::write(
            &config_path,
            format!("repositories:\n  - path: {}\n", repo_path.display()),
        )
        .unwrap();

        let config = CidConfig::load_from_path(&FilePath::new(&config_path)).unwrap();
        let repositories = config.repositories().unwrap();

        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].name(), "foo");
        assert_eq!(repositories[0].path().as_path(), repo_path.as_path());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn load_from_path_rejects_non_git_directories() {
        let root = temp_test_dir("config-invalid");
        let repo_path = root.join("foo");
        fs::create_dir_all(&repo_path).unwrap();

        let config_path = root.join("cid-config.yaml");
        fs::write(
            &config_path,
            format!("repositories:\n  - path: {}\n", repo_path.display()),
        )
        .unwrap();

        let error = CidConfig::load_from_path(&FilePath::new(&config_path)).unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("configured repository path is not a git repository")
        );

        fs::remove_dir_all(root).unwrap();
    }
}
