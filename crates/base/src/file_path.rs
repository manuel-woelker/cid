//! File path wrapper type for efficient path handling.

use crate::shared_string::SharedString;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Path, PathBuf};

/// A wrapper around SharedString for file path handling.
///
/// This type provides efficient string-based path storage with cheap cloning,
/// making it ideal for compiler internal path representation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FilePath(pub SharedString);

impl FilePath {
    /// Creates a new FilePath from the given path string.
    pub fn new<P: AsRef<std::path::Path>>(path: P) -> Self {
        Self(SharedString::new(path.as_ref().to_string_lossy()))
    }

    /// Creates a new FilePath from a string.
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(SharedString::new(s.into()))
    }

    /// Returns the path as a string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the path as a Path.
    pub fn as_path(&self) -> &Path {
        Path::new(self.0.as_str())
    }

    /// Returns the underlying SharedString.
    pub fn into_shared_string(self) -> SharedString {
        self.0
    }

    /// Joins this path with another path component.
    pub fn join<P: AsRef<std::path::Path>>(&self, path: P) -> Self {
        let mut path_buf = PathBuf::from(self.0.as_str());
        path_buf.push(path);
        Self(SharedString::new(path_buf.to_string_lossy()))
    }

    /// Returns the parent directory of this path, if any.
    pub fn parent(&self) -> Option<Self> {
        Path::new(self.0.as_str())
            .parent()
            .map(|p| Self(SharedString::new(p.to_string_lossy())))
    }

    /// Returns the file name of this path, if any.
    pub fn file_name(&self) -> Option<&str> {
        Path::new(self.0.as_str())
            .file_name()
            .and_then(|s| s.to_str())
    }

    /// Returns the file stem (name without extension) of this path, if any.
    pub fn file_stem(&self) -> Option<&str> {
        Path::new(self.0.as_str())
            .file_stem()
            .and_then(|s| s.to_str())
    }

    /// Returns the extension of this path, if any.
    pub fn extension(&self) -> Option<&str> {
        Path::new(self.0.as_str())
            .extension()
            .and_then(|s| s.to_str())
    }

    /// Returns true if this path is absolute.
    pub fn is_absolute(&self) -> bool {
        Path::new(self.0.as_str()).is_absolute()
    }

    /// Returns true if this path is relative.
    pub fn is_relative(&self) -> bool {
        Path::new(self.0.as_str()).is_relative()
    }

    /// Normalizes the path by removing redundant components.
    pub fn normalize(&self) -> Self {
        let path = Path::new(self.0.as_str());
        let mut components = Vec::new();

        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    // Remove the last normal component if there is one
                    if let Some(last) = components.last()
                        && matches!(last, std::path::Component::Normal(_))
                    {
                        components.pop();
                    }
                }
                std::path::Component::CurDir => {
                    // Skip current directory components
                }
                _ => {
                    components.push(component);
                }
            }
        }

        let normalized: PathBuf = components.iter().collect();
        Self(SharedString::new(normalized.to_string_lossy()))
    }
}

impl From<String> for FilePath {
    fn from(s: String) -> Self {
        Self(SharedString::new(s))
    }
}

impl From<&str> for FilePath {
    fn from(s: &str) -> Self {
        Self(SharedString::new(s))
    }
}

impl From<SharedString> for FilePath {
    fn from(s: SharedString) -> Self {
        Self(s)
    }
}

impl From<&FilePath> for FilePath {
    fn from(path: &FilePath) -> Self {
        path.clone()
    }
}

impl AsRef<Path> for FilePath {
    fn as_ref(&self) -> &Path {
        Path::new(self.0.as_str())
    }
}

impl std::fmt::Display for FilePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Deref for FilePath {
    type Target = SharedString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for FilePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FilePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(value.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_path_creation() {
        let p1 = FilePath::new("src/main.rs");
        let p2 = FilePath::from_string("test.holo");
        let p3: FilePath = "lib/core.rs".into();

        assert_eq!(p1.as_str(), "src/main.rs");
        assert_eq!(p2.as_str(), "test.holo");
        assert_eq!(p3.as_str(), "lib/core.rs");
    }

    #[test]
    fn test_file_path_equality() {
        let p1 = FilePath::new("src/main.rs");
        let p2 = FilePath::new("src/main.rs");
        let p3 = FilePath::new("src/lib.rs");

        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
    }

    #[test]
    fn test_file_path_clone() {
        let p1 = FilePath::new("src/main.rs");
        let p2 = p1.clone();

        assert_eq!(p1, p2);
        assert_eq!(p1.as_str(), p2.as_str());
    }

    #[test]
    fn test_file_path_join() {
        let base = FilePath::new("src");
        let joined = base.join("main.rs");
        assert_eq!(joined.as_str(), "src/main.rs");
    }

    #[test]
    fn test_file_path_components() {
        let path = FilePath::new("src/main.rs");

        assert_eq!(path.file_name(), Some("main.rs"));
        assert_eq!(path.file_stem(), Some("main"));
        assert_eq!(path.extension(), Some("rs"));

        let parent = path.parent().unwrap();
        assert_eq!(parent.as_str(), "src");
    }

    #[test]
    fn test_file_path_normalize() {
        let path = FilePath::new("src/../src/main.rs");
        let normalized = path.normalize();
        assert_eq!(normalized.as_str(), "src/main.rs");
    }

    #[test]
    fn test_file_path_display() {
        let path = FilePath::new("src/main.rs");
        assert_eq!(format!("{}", path), "src/main.rs");
    }

    #[test]
    fn test_file_path_default() {
        let path = FilePath::default();
        assert!(path.as_str().is_empty());
    }
}
