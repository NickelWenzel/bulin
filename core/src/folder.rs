//! A folder the user opens in bulin, whose shader files form one WESL package.

use std::fmt;

/// A file or directory inside a folder, relative to the folder: `/`-separated, never absolute,
/// with no empty, `.` or `..` segments. Every file has exactly one `Path`, so paths compare equal
/// exactly when they name the same file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path(String);

/// Why a text isn't a folder path.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    #[error("a path needs a name")]
    Empty,
    #[error("`{0}` is absolute, but paths are relative to the folder")]
    Absolute(String),
    #[error("`{0}` contains `\\`, but paths are separated by `/`")]
    Backslash(String),
    #[error("`{0}` has an empty segment")]
    EmptySegment(String),
    #[error("`{0}` has a `.` segment")]
    CurrentDir(String),
    #[error("`{0}` has a `..` segment, which would leave the folder")]
    ParentDir(String),
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Path {
    /// Validates `text` as a path inside the folder.
    pub fn new(text: &str) -> Result<Self, PathError> {
        if text.is_empty() {
            return Err(PathError::Empty);
        }
        if text.starts_with('/') {
            return Err(PathError::Absolute(text.to_owned()));
        }
        if text.contains('\\') {
            return Err(PathError::Backslash(text.to_owned()));
        }
        for segment in text.split('/') {
            match segment {
                "" => return Err(PathError::EmptySegment(text.to_owned())),
                "." => return Err(PathError::CurrentDir(text.to_owned())),
                ".." => return Err(PathError::ParentDir(text.to_owned())),
                _ => {}
            }
        }
        Ok(Self(text.to_owned()))
    }

    /// The path as written, with `/` between its segments.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The file name's extension, without the dot.
    pub fn extension(&self) -> Option<&str> {
        let name = self.0.rsplit('/').next().unwrap_or(&self.0);
        name.rsplit_once('.')
            .and_then(|(stem, extension)| (!stem.is_empty()).then_some(extension))
    }

    /// The rest of this path below the directory `prefix`, or `None` when it isn't inside it.
    pub fn strip_prefix(&self, prefix: &Path) -> Option<&str> {
        self.0
            .strip_prefix(prefix.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_validation() {
        assert_eq!(Path::new(""), Err(PathError::Empty));
        assert_eq!(Path::new("/a"), Err(PathError::Absolute("/a".into())));
        assert_eq!(
            Path::new("a/../b"),
            Err(PathError::ParentDir("a/../b".into()))
        );
        assert_eq!(
            Path::new("a//b"),
            Err(PathError::EmptySegment("a//b".into()))
        );
        assert_eq!(Path::new("a/"), Err(PathError::EmptySegment("a/".into())));
        assert_eq!(Path::new("./a"), Err(PathError::CurrentDir("./a".into())));
        assert_eq!(Path::new("a\\b"), Err(PathError::Backslash("a\\b".into())));

        assert_eq!(Path::new("a/b.wesl").unwrap().as_str(), "a/b.wesl");
    }

    #[test]
    fn extensions_and_prefixes() {
        let path = Path::new("shaders/noise.wesl").unwrap();
        let shaders = Path::new("shaders").unwrap();

        assert_eq!(path.extension(), Some("wesl"));
        assert_eq!(Path::new(".hidden").unwrap().extension(), None);
        assert_eq!(Path::new("a.b/c").unwrap().extension(), None);
        assert_eq!(path.strip_prefix(&shaders), Some("noise.wesl"));
        assert_eq!(
            path.strip_prefix(&Path::new("shade").unwrap()),
            None,
            "a prefix must end at a separator"
        );
    }
}
