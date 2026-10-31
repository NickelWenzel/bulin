//! How the web keeps a library's folders in IndexedDB: each file is a record keyed by its entry's
//! id and its path, so a directory is a range of keys.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::folder::{Entry, Kind, Path};

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

/// The path part of the key that marks `dir` as a directory, so the directory persists while it
/// holds no file. [`collapse`] lists it as a directory.
pub fn marker_key(dir: &Path) -> String {
    format!("{dir}/")
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

/// The listing of the directory `dir`, `None` being the entry's root, from the paths of the keys
/// in its [`range`]: the next segment of each, a directory when more follow or when the key is a
/// directory marker ending in `/`. Sorted, directories first.
pub fn collapse<'a>(dir: Option<&Path>, keys: impl IntoIterator<Item = &'a str>) -> Vec<Entry> {
    let prefix = dir.map_or_else(String::new, |dir| format!("{dir}/"));
    keys.into_iter()
        .filter_map(|key| {
            let rest = key.strip_prefix(prefix.as_str())?;
            let (name, kind) = match rest.split_once('/') {
                Some((name, _)) => (name, Kind::Dir),
                None => (rest, Kind::File),
            };
            let path = Path::in_dir(dir, name).ok()?;
            Some(Entry { kind, path })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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

    #[test]
    fn markers_list_as_directories() {
        let empty = Path::new("empty").unwrap();
        assert_eq!(marker_key(&empty), "empty/");
        assert_eq!(
            collapse(None, [marker_key(&empty).as_str()]),
            [Entry {
                kind: Kind::Dir,
                path: empty
            }]
        );

        let dir = Path::new("dir").unwrap();
        let inner = marker_key(&Path::new("dir/sub").unwrap());
        assert!(inside(&range(Some(&dir)), &inner));
        assert!(inside(&range(Some(&dir)), &marker_key(&dir)));
    }

    #[test]
    fn collapse_lists_one_level() {
        let keys = ["a.wesl", "dir/b.wesl", "dir/sub/c.wesl", "empty/"];
        let entry = |kind, path| Entry {
            kind,
            path: Path::new(path).unwrap(),
        };

        assert_eq!(
            collapse(None, keys),
            [
                entry(Kind::Dir, "dir"),
                entry(Kind::Dir, "empty"),
                entry(Kind::File, "a.wesl"),
            ]
        );
        assert_eq!(
            collapse(Some(&Path::new("dir").unwrap()), keys),
            [entry(Kind::Dir, "dir/sub"), entry(Kind::File, "dir/b.wesl")]
        );
    }
}
