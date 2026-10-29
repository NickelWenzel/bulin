//! How the web keeps a library's folders in IndexedDB: each file is a record keyed by its entry's
//! id and its path, so a directory is a range of keys.

use serde::{Deserialize, Serialize};

use crate::folder::Path;

/// A library entry, as IndexedDB numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Id(u64);

/// Where a range of file keys ends, exclusively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upper {
    /// Before the key with this path.
    Before(String),
    /// After every key with a path, which a key whose second part is an array is in IndexedDB's
    /// order.
    AfterStrings,
}

impl Id {
    /// The entry IndexedDB numbered `value`.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number IndexedDB knows the entry by.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// The key of the file `path` in the entry `id`, as the two parts of an IndexedDB array key.
pub fn file_key(id: Id, path: &Path) -> (Id, String) {
    (id, path.as_str().to_owned())
}

/// The paths of the keys under `dir`, `None` being the entry's root: from the first path,
/// inclusive, up to the [`Upper`] bound.
pub fn range(dir: Option<&Path>) -> (String, Upper) {
    match dir {
        None => (String::new(), Upper::AfterStrings),
        // `0` follows `/`, so every path that continues `dir/` sorts before `dir0`.
        Some(dir) => (format!("{dir}/"), Upper::Before(format!("{dir}0"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inside(range: &(String, Upper), key: &str) -> bool {
        let (lower, upper) = range;
        key >= lower.as_str()
            && match upper {
                Upper::Before(upper) => key < upper.as_str(),
                Upper::AfterStrings => true,
            }
    }

    #[test]
    fn keys_and_ranges() {
        let path = Path::new("dir/x.wesl").unwrap();
        assert_eq!(
            file_key(Id::new(7), &path),
            (Id::new(7), "dir/x.wesl".to_owned())
        );

        let root = range(None);
        assert_eq!(root, (String::new(), Upper::AfterStrings));
        assert!(inside(&root, ""));
        assert!(inside(&root, "dir/x"));

        let dir = range(Some(&Path::new("dir").unwrap()));
        assert_eq!(dir, ("dir/".to_owned(), Upper::Before("dir0".to_owned())));
        assert!(inside(&dir, "dir/x"));
        assert!(inside(&dir, "dir/sub/y"));
        assert!(!inside(&dir, "dir0"));
        assert!(!inside(&dir, "dir.x"));
        assert!(!inside(&dir, "dir"));
    }
}
