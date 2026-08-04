use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackagePath(String);

impl PackagePath {
    pub fn new(value: impl Into<String>) -> Result<Self, PackagePathError> {
        let value = value.into();
        validate_package_path(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn join_to(&self, root: impl AsRef<Path>) -> PathBuf {
        root.as_ref().join(&self.0)
    }

    pub fn join_package_path(&self, child: &PackagePath) -> Result<Self, PackagePathError> {
        Self::new(format!("{}/{}", self.as_str(), child.as_str()))
    }
}

impl fmt::Display for PackagePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for PackagePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PackagePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackagePathError {
    Empty,
    Absolute(String),
    ContainsNul(String),
    ContainsBackslash(String),
    ContainsRepeatedSeparator(String),
    EndsWithSeparator(String),
    InvalidSegment { path: String, segment: String },
}

impl fmt::Display for PackagePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("package path must not be empty"),
            Self::Absolute(path) => write!(f, "package path must be relative: {path}"),
            Self::ContainsNul(path) => write!(f, "package path contains NUL byte: {path:?}"),
            Self::ContainsBackslash(path) => {
                write!(f, "package path must use '/' separators, not '\\': {path}")
            }
            Self::ContainsRepeatedSeparator(path) => {
                write!(f, "package path contains repeated '/': {path}")
            }
            Self::EndsWithSeparator(path) => write!(f, "package path ends with '/': {path}"),
            Self::InvalidSegment { path, segment } => {
                write!(
                    f,
                    "package path contains invalid segment {segment:?}: {path}"
                )
            }
        }
    }
}

impl std::error::Error for PackagePathError {}

fn validate_package_path(path: &str) -> Result<(), PackagePathError> {
    if path.is_empty() {
        return Err(PackagePathError::Empty);
    }
    if Path::new(path).is_absolute() || path.starts_with('/') {
        return Err(PackagePathError::Absolute(path.to_owned()));
    }
    if path.contains('\0') {
        return Err(PackagePathError::ContainsNul(path.to_owned()));
    }
    if path.contains('\\') {
        return Err(PackagePathError::ContainsBackslash(path.to_owned()));
    }
    if path.contains("//") {
        return Err(PackagePathError::ContainsRepeatedSeparator(path.to_owned()));
    }
    if path.ends_with('/') {
        return Err(PackagePathError::EndsWithSeparator(path.to_owned()));
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(PackagePathError::InvalidSegment {
                path: path.to_owned(),
                segment: segment.to_owned(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_relative_package_paths() {
        let path = PackagePath::new("assets/wallpaper.png").unwrap();
        assert_eq!(path.as_str(), "assets/wallpaper.png");
    }

    #[test]
    fn rejects_package_path_traversal() {
        assert!(PackagePath::new("../secret").is_err());
        assert!(PackagePath::new("assets/../secret").is_err());
        assert!(PackagePath::new("/tmp/wallpaper.png").is_err());
    }
}
