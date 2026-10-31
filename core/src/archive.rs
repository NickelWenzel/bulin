//! What the web imports into its library and exports from it: an archive's entries as names and
//! bytes, the limits that keep an oversized or hostile archive out, and the paths that don't travel
//! because bulin regenerates them.

use crate::folder::{Path, SETTINGS, Settings, TMP};

/// The largest import, in bytes, before it is read: a zip's size, or the total of a directory's
/// files.
pub const MAX_COMPRESSED: u64 = 64 << 20;

/// The most bytes an import's entries may hold together, once unpacked.
pub const MAX_UNCOMPRESSED: u64 = 64 << 20;

/// The most entries an import may hold, directories included.
pub const MAX_ENTRIES: usize = 10_000;

/// The directories, at the folder's root, that neither import nor export carries: git's, and the
/// ones bulin regenerates.
const SKIPPED: [&str; 4] = [".git", ".bulin/bulin", ".bulin/uniforms", TMP];

/// The bytes an import may still unpack, shared by all its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    remaining: u64,
}

/// What an import keeps: the files to store with their texts, the directories to mark, and what
/// it left out, for the user.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Imported {
    pub files: Vec<(Path, String)>,
    pub dirs: Vec<Path>,
    pub notices: Vec<String>,
}

/// Why an import was refused as a whole.
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    #[error("it is larger than 64 MiB")]
    TooLarge,
    #[error("it holds more than 10,000 entries")]
    TooManyEntries,
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

impl Budget {
    /// The whole of [`MAX_UNCOMPRESSED`].
    pub fn new() -> Self {
        Self {
            remaining: MAX_UNCOMPRESSED,
        }
    }

    /// How many bytes to read from the next entry: one more than remain, so an entry that
    /// overruns the budget is caught by [`Budget::spend`] without reading the rest of it.
    pub fn limit(&self) -> u64 {
        self.remaining + 1
    }

    /// Takes `bytes` from the budget, or fails with [`Error::TooLarge`] when fewer remain.
    pub fn spend(&mut self, bytes: u64) -> Result<(), Error> {
        self.remaining = self.remaining.checked_sub(bytes).ok_or(Error::TooLarge)?;
        Ok(())
    }
}

/// Whether the archive entry `name` lies in a directory that neither import nor export carries.
pub fn skipped(name: &str) -> bool {
    let name = name.strip_suffix('/').unwrap_or(name);
    SKIPPED.iter().any(|dir| {
        name.strip_prefix(dir)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// What to store from an archive's `entries`, each a name and its unpacked bytes. A name ending
/// in `/` is a directory.
///
/// Entries that are [`skipped`] are left out silently. Entries whose name isn't a [`Path`], files
/// that aren't UTF-8, and a `settings.json` bulin can't read are left out with a notice.
pub fn import(entries: impl IntoIterator<Item = (String, Vec<u8>)>) -> Result<Imported, Error> {
    let mut imported = Imported::default();
    for (index, (name, bytes)) in entries.into_iter().enumerate() {
        if index >= MAX_ENTRIES {
            return Err(Error::TooManyEntries);
        }
        if skipped(&name) {
            continue;
        }
        let (text, is_dir) = match name.strip_suffix('/') {
            Some(dir) => (dir, true),
            None => (name.as_str(), false),
        };
        let path = match Path::new(text) {
            Ok(path) => path,
            Err(error) => {
                imported
                    .notices
                    .push(format!("`{name}` was left out: {error}."));
                continue;
            }
        };
        if is_dir {
            imported.dirs.push(path);
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            imported
                .notices
                .push(format!("{path} was left out: it isn't UTF-8 text."));
            continue;
        };
        if path.as_str() == SETTINGS
            && let Err(error) = Settings::from_json(&text)
        {
            imported
                .notices
                .push(format!("{path} was left out: {error}."));
            continue;
        }
        imported.files.push((path, text));
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::uniform::Type;
    use crate::{Clock, Uniforms};

    fn entry(name: &str, bytes: &[u8]) -> (String, Vec<u8>) {
        (name.to_owned(), bytes.to_vec())
    }

    fn names(imported: &Imported) -> Vec<&str> {
        imported
            .files
            .iter()
            .map(|(path, _)| path.as_str())
            .collect()
    }

    #[test]
    fn paths_are_validated() {
        let imported = import([
            entry("../x.wesl", b""),
            entry("/abs.wesl", b""),
            entry("a//b.wesl", b""),
            entry("dir/x.wesl", b"fn f() {}"),
        ])
        .unwrap();

        assert_eq!(names(&imported), ["dir/x.wesl"]);
        assert_eq!(imported.notices.len(), 3);
        assert!(imported.notices[0].contains("../x.wesl"));
        assert!(imported.notices[1].contains("/abs.wesl"));
        assert!(imported.notices[2].contains("a//b.wesl"));
    }

    #[test]
    fn regenerated_and_git_paths_are_skipped() {
        let settings = Settings::default().to_json();
        let imported = import([
            entry(".git/config", b""),
            entry(".bulin/bulin/package.wesl", b""),
            entry(".bulin/uniforms/wesl.toml", b""),
            entry(".bulin/tmp/1.tmp", b""),
            entry(".bulin/uniforms/", b""),
            entry(".bulin/settings.json", settings.as_bytes()),
            entry(".github/x.wesl", b""),
        ])
        .unwrap();

        assert_eq!(names(&imported), [".bulin/settings.json", ".github/x.wesl"]);
        assert!(imported.dirs.is_empty());
        assert!(imported.notices.is_empty());
    }

    #[test]
    fn settings_are_kept_only_when_valid() {
        let valid = Settings::default().to_json();
        let imported = import([entry(SETTINGS, valid.as_bytes())]).unwrap();
        assert_eq!(names(&imported), [SETTINGS]);
        assert!(imported.notices.is_empty());

        let imported = import([entry(SETTINGS, br#"{"version": 9}"#)]).unwrap();
        assert!(imported.files.is_empty());
        assert_eq!(imported.notices.len(), 1);
        assert!(imported.notices[0].contains(SETTINGS));
    }

    #[test]
    fn non_utf8_entries_are_reported_not_stored() {
        let imported = import([
            entry("image.png", &[0xff, 0xfe, 0x00]),
            entry("main.wesl", b"fn f() {}"),
        ])
        .unwrap();

        assert_eq!(names(&imported), ["main.wesl"]);
        assert_eq!(imported.notices.len(), 1);
        assert!(imported.notices[0].contains("image.png"));
    }

    #[test]
    fn directories_become_dirs() {
        let imported = import([entry("empty/", b"")]).unwrap();

        assert_eq!(imported.dirs, [Path::new("empty").unwrap()]);
        assert!(imported.files.is_empty());
    }

    #[test]
    fn too_many_entries_are_rejected() {
        let entries = (0..=MAX_ENTRIES).map(|index| (format!("{index}.wesl"), Vec::new()));

        assert_eq!(import(entries), Err(Error::TooManyEntries));
        let entries = (0..MAX_ENTRIES).map(|index| (format!("{index}.wesl"), Vec::new()));
        assert_eq!(import(entries).unwrap().files.len(), MAX_ENTRIES);
    }

    #[test]
    fn the_budget_is_shared() {
        let mut budget = Budget::new();
        assert_eq!(budget.limit(), MAX_UNCOMPRESSED + 1);

        budget.spend(40 << 20).unwrap();
        assert_eq!(budget.limit(), MAX_UNCOMPRESSED - (40 << 20) + 1);
        assert_eq!(budget.spend(40 << 20), Err(Error::TooLarge));
    }

    #[test]
    fn a_round_trip_keeps_main_uniforms_and_clock() {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(0.5)).unwrap();
        let mut clock = Clock::started();
        clock.advance(Duration::from_secs(3));
        let settings = Settings::new(Some(Path::new("main.wesl").unwrap()), uniforms, Some(clock));
        let json = settings.to_json();

        let imported = import([
            entry("main.wesl", b"import package::lib;"),
            entry("lib.wesl", b"fn f() {}"),
            entry("empty/", b""),
            entry(".bulin/uniforms/package.wesl", b""),
            entry(SETTINGS, json.as_bytes()),
        ])
        .unwrap();

        let (_, kept) = imported
            .files
            .iter()
            .find(|(path, _)| path.as_str() == SETTINGS)
            .unwrap();
        assert_eq!(Settings::from_json(kept), Ok(settings));
        assert_eq!(names(&imported), ["main.wesl", "lib.wesl", SETTINGS]);
        assert_eq!(imported.dirs, [Path::new("empty").unwrap()]);
        assert!(imported.notices.is_empty());
    }
}
