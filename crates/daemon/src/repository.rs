use cid_base::file_path::FilePath;
use cid_base::shared_string::SharedString;
use serde::{Deserialize, Serialize};

/// Repository configuration known to the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    name: SharedString,
    path: FilePath,
}

impl Repository {
    /// Creates a repository value from a name and local path.
    pub fn new(name: impl Into<SharedString>, path: FilePath) -> Self {
        Self {
            name: name.into(),
            path,
        }
    }

    /// Returns the configured repository name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the configured local repository path.
    pub fn path(&self) -> &FilePath {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use cid_base::file_path::FilePath;

    use super::Repository;

    #[test]
    fn repository_exposes_name_and_path() {
        let repository = Repository::new("cid", FilePath::new("/repos/cid"));

        assert_eq!(repository.name(), "cid");
        assert_eq!(repository.path().as_str(), "/repos/cid");
    }
}
