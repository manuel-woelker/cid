//! Shared string wrapper type for efficient string handling.

use crate::result::CidResult;
use ecow::EcoString;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A wrapper around ecow::EcoString for efficient shared string storage.
///
/// This type provides copy-on-write semantics with cheap cloning,
/// making it ideal for storing strings that are shared across multiple
/// parts of the compiler without unnecessary allocations.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SharedString(pub EcoString);

impl SharedString {
    /// Creates a new SharedString from the given string.
    pub fn new(s: impl Into<String>) -> Self {
        Self(EcoString::from(s.into()))
    }

    /// Creates a new empty SharedString.
    pub fn empty() -> Self {
        Self(EcoString::from(""))
    }

    /// Clears the contents of the string.
    pub fn clear(&mut self) {
        self.0 = EcoString::from("");
    }

    /// Appends a string slice to this string.
    pub fn push_str(&mut self, string: &str) {
        let mut new_string = self.0.to_string();
        new_string.push_str(string);
        self.0 = EcoString::from(new_string);
    }

    /// Returns the underlying string as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the length of the string.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the string is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn from_utf8(ut8_bytes: &[u8]) -> CidResult<Self> {
        Ok(Self(EcoString::from(std::str::from_utf8(ut8_bytes)?)))
    }
}

/// Creates a new SharedString from a format string.
///
/// This macro delegates to `eco_format` from the ecow crate.
#[macro_export]
macro_rules! shared_format {
    ($($arg:tt)*) => {
        $crate::shared_string::SharedString(ecow::eco_format!($($arg)*))
    };
}

impl Default for SharedString {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<String> for SharedString {
    fn from(s: String) -> Self {
        Self(EcoString::from(s))
    }
}

impl From<&str> for SharedString {
    fn from(s: &str) -> Self {
        Self(EcoString::from(s))
    }
}

impl From<Box<str>> for SharedString {
    fn from(s: Box<str>) -> Self {
        Self(EcoString::from(&*s))
    }
}

impl From<&SharedString> for SharedString {
    fn from(s: &SharedString) -> Self {
        s.clone()
    }
}

impl AsRef<str> for SharedString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for SharedString {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SharedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for SharedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}`", self.0)
    }
}

impl std::ops::Deref for SharedString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl PartialEq<str> for SharedString {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for SharedString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<SharedString> for str {
    fn eq(&self, other: &SharedString) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<SharedString> for &str {
    fn eq(&self, other: &SharedString) -> bool {
        *self == other.as_str()
    }
}

impl Serialize for SharedString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SharedString {
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
    fn test_shared_string_creation() {
        let s1 = SharedString::new("hello");
        let s2 = SharedString::from("world");
        let s3: SharedString = "test".into();

        assert_eq!(s1.as_str(), "hello");
        assert_eq!(s2.as_str(), "world");
        assert_eq!(s3.as_str(), "test");
    }

    #[test]
    fn test_shared_string_equality() {
        let s1 = SharedString::new("hello");
        let s2 = SharedString::new("hello");
        let s3 = SharedString::new("world");

        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn test_shared_string_clone() {
        let s1 = SharedString::new("hello");
        let s2 = s1.clone();

        assert_eq!(s1, s2);
        assert_eq!(s1.as_str(), s2.as_str());
    }

    #[test]
    fn test_shared_string_default() {
        let s = SharedString::default();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn test_shared_string_deref() {
        let s = SharedString::new("hello");
        assert_eq!(s.len(), 5);
        assert_eq!(&s[0..2], "he");
    }

    #[test]
    fn test_shared_string_display() {
        let s = SharedString::new("hello");
        assert_eq!(format!("{}", s), "hello");
    }

    #[test]
    fn test_shared_string_format_macro() {
        let name = "world";
        let s = shared_format!("Hello, {}!", name);
        assert_eq!(s.as_str(), "Hello, world!");
    }

    #[test]
    fn test_shared_string_format_macro_multiple_args() {
        let s = shared_format!("{} + {} = {}", 1, 2, 3);
        assert_eq!(s.as_str(), "1 + 2 = 3");
    }
}
