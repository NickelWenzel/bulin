//! A folder the user opens in bulin, whose shader files form one WESL package: its paths, the
//! files bulin generates in it, its settings, and the changes storage reports about it.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wesl::toml_cfg::{DependenciesConfig, DependencySpec, WeslToml};

use crate::program::{BULIN_PACKAGE_NAME, BULIN_PACKAGE_SOURCE, UNIFORMS_PACKAGE_NAME};
use crate::{Clock, Uniforms};

/// The manifest of a package whose modules sit next to it, instead of in wesl's default
/// `./shaders/`.
pub const PACKAGE_MANIFEST: &str = "edition = \"2026_pre\"\nroot = \".\"\n";

/// The `[dependencies]` lines a folder's `wesl.toml` needs so its shaders can import the packages
/// bulin generates.
pub const DEPENDENCY_LINES: &str = "bulin = { path = \".bulin/bulin\" }\n\
                                    uniforms = { path = \".bulin/uniforms\" }\n";

/// Where a folder keeps its settings.
pub const SETTINGS: &str = ".bulin/settings.json";

/// The name of a package's manifest, and of the folder's own at its root.
pub const MANIFEST: &str = "wesl.toml";

/// The directory storage writes into before moving a file into place.
pub const TMP: &str = ".bulin/tmp";

/// Directories bulin never reads or watches, at any depth.
pub const SKIP: [&str; 4] = [".git", ".bulin", "target", "node_modules"];

/// The largest file, in bytes, whose text storage reads.
pub const MAX_TEXT: usize = 1 << 20;

/// The format version of the settings this build writes and reads.
const SETTINGS_VERSION: u32 = 1;

/// A file or directory inside a folder, relative to the folder: `/`-separated, never absolute,
/// with no empty, `.` or `..` segments. Every file has exactly one `Path`, so paths compare equal
/// exactly when they name the same file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
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

/// Why a `wesl.toml` gives no package root.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    #[error("`wesl.toml` has no `edition`, so its `root` is ignored")]
    MissingEdition,
    #[error("the package root `{0}` in `wesl.toml` isn't a directory inside the folder")]
    Escapes(String),
    #[error("`wesl.toml` is invalid: {0}")]
    Invalid(String),
}

/// A folder's bulin state: its main file, the uniforms and the clock.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Settings {
    version: u32,
    /// The file the preview runs, once the user chose one.
    pub main: Option<Path>,
    pub uniforms: Uniforms,
    /// The clock behind `time`, when it is on.
    pub clock: Option<Clock>,
}

/// Why a file couldn't be read as a folder's settings.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    #[error("not bulin settings ({0})")]
    Invalid(String),
    #[error("bulin settings version {0} are not supported, only version {SETTINGS_VERSION}")]
    Unsupported(u32),
}

/// What a write expects to find on disk before it replaces a file.
#[derive(Debug, Clone, PartialEq)]
pub enum Expect {
    /// No file at the path.
    Missing,
    /// A file with this text, line endings aside.
    Text(String),
    /// Whatever is there, if anything.
    Anything,
}

/// What storage reports about a path in the folder.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A file was created or changed. `text` is its text where [`reads_text`] holds and the file
    /// is UTF-8 and at most [`MAX_TEXT`] bytes, and `None` otherwise.
    Changed { path: Path, text: Option<String> },
    /// A file or a directory, with everything under it, is gone.
    Removed(Path),
}

/// The texts of the folder's files that bulin reads, outside [`SKIP`]: what a [`Program`] compiles
/// and the `wesl.toml` files that shape its packages.
///
/// [`Program`]: crate::Program
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mirror(Arc<BTreeMap<Path, String>>);

/// Whether a folder entry is a directory or a file. Directories order first, so a sorted listing
/// shows them above the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Dir,
    File,
}

/// One entry of a directory listing. Entries order by kind, then by path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Entry {
    pub kind: Kind,
    pub path: Path,
}

/// What a change did to one file of a [`Mirror`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Created,
    Changed,
    Removed,
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Path {
    type Error = PathError;

    fn try_from(text: String) -> Result<Self, PathError> {
        Self::new(&text)
    }
}

/// Paths are stored as their text, as in `settings.json`.
impl From<Path> for String {
    fn from(path: Path) -> Self {
        path.0
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new(None, Uniforms::default(), None)
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

    /// The entry `name` of the directory `dir`, `None` being the folder itself.
    pub fn in_dir(dir: Option<&Path>, name: &str) -> Result<Self, PathError> {
        match dir {
            Some(dir) => Self::new(&format!("{dir}/{name}")),
            None => Self::new(name),
        }
    }

    /// The path as written, with `/` between its segments.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The last segment: the name of the file or directory.
    pub fn name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// The directory holding this path, `None` when that is the folder itself.
    pub fn parent(&self) -> Option<Path> {
        self.0
            .rsplit_once('/')
            .map(|(parent, _)| Self(parent.to_owned()))
    }

    /// The file name's extension, without the dot.
    pub fn extension(&self) -> Option<&str> {
        self.name()
            .rsplit_once('.')
            .and_then(|(stem, extension)| (!stem.is_empty()).then_some(extension))
    }

    /// The rest of this path below the directory `prefix`, or `None` when it isn't inside it.
    pub fn strip_prefix(&self, prefix: &Path) -> Option<&str> {
        self.0
            .strip_prefix(prefix.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
    }
}

impl Settings {
    /// Settings in the version this build writes.
    pub fn new(main: Option<Path>, uniforms: Uniforms, clock: Option<Clock>) -> Self {
        Self {
            version: SETTINGS_VERSION,
            main,
            uniforms,
            clock,
        }
    }

    /// The text of `settings.json`.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("invariant: settings serialize to JSON")
    }

    /// Reads `settings.json`, refusing any version but this build's.
    pub fn from_json(json: &str) -> Result<Self, SettingsError> {
        #[derive(Deserialize)]
        struct Header {
            version: Option<u32>,
        }

        let header: Header = serde_json::from_str(json)
            .map_err(|error| SettingsError::Invalid(error.to_string()))?;
        match header.version {
            None => Err(SettingsError::Invalid("version missing".to_owned())),
            Some(SETTINGS_VERSION) => serde_json::from_str(json)
                .map_err(|error| SettingsError::Invalid(error.to_string())),
            Some(version) => Err(SettingsError::Unsupported(version)),
        }
    }
}

impl Expect {
    /// Whether the file on disk, `None` when there is none, is what this expects.
    pub fn matches(&self, disk: Option<&str>) -> bool {
        match self {
            Self::Missing => disk.is_none(),
            Self::Text(text) => disk.is_some_and(|disk| same_text(text, disk)),
            Self::Anything => true,
        }
    }
}

impl Mirror {
    /// The texts by path, shared so a [`Program`](crate::Program) can hold them without a copy.
    pub fn files(&self) -> &Arc<BTreeMap<Path, String>> {
        &self.0
    }

    /// Applies `change`, reporting each file it created, changed or removed. A change that leaves
    /// the texts as they were, line endings aside, reports nothing.
    pub fn apply(&mut self, change: &Change) -> Vec<(Path, Effect)> {
        match change {
            Change::Changed { path, .. } if is_skipped(path) || !reads_text(path) => Vec::new(),
            Change::Changed {
                path,
                text: Some(text),
            } => {
                let effect = match self.0.get(path) {
                    Some(old) if same_text(old, text) => return Vec::new(),
                    Some(_) => Effect::Changed,
                    None => Effect::Created,
                };
                Arc::make_mut(&mut self.0).insert(path.clone(), text.clone());
                vec![(path.clone(), effect)]
            }
            // The file is too large or not UTF-8 now, so a program can't read it any more.
            Change::Changed { path, text: None } => {
                if !self.0.contains_key(path) {
                    return Vec::new();
                }
                Arc::make_mut(&mut self.0).remove(path);
                vec![(path.clone(), Effect::Removed)]
            }
            Change::Removed(path) => {
                let removed = self
                    .0
                    .keys()
                    .filter(|file| *file == path || file.strip_prefix(path).is_some())
                    .cloned()
                    .collect::<Vec<_>>();
                if removed.is_empty() {
                    return Vec::new();
                }
                let files = Arc::make_mut(&mut self.0);
                removed
                    .into_iter()
                    .map(|file| {
                        files.remove(&file);
                        (file, Effect::Removed)
                    })
                    .collect()
            }
        }
    }

    /// The changes that turn this mirror into `scan`, a fresh read of every file it would hold
    /// with the same `text` rules as [`Change::Changed`]. Apply them with [`Mirror::apply`].
    pub fn replace(&self, scan: &BTreeMap<Path, Option<String>>) -> Vec<Change> {
        let changed = scan
            .iter()
            .filter_map(|(path, text)| match (text, self.0.get(path)) {
                (Some(text), Some(old)) if same_text(old, text) => None,
                _ => Some(Change::Changed {
                    path: path.clone(),
                    text: text.clone(),
                }),
            });
        let removed = self
            .0
            .keys()
            .filter(|path| !scan.contains_key(*path))
            .map(|path| Change::Removed(path.clone()));
        changed.chain(removed).collect()
    }
}

/// Whether `path` is or lies in a directory bulin never reads or watches.
pub fn is_skipped(path: &Path) -> bool {
    path.as_str()
        .split('/')
        .any(|segment| SKIP.contains(&segment))
}

/// Whether `path` is a `.wesl` or `.wgsl` file.
pub fn is_shader(path: &Path) -> bool {
    matches!(path.extension(), Some("wesl" | "wgsl"))
}

/// Whether storage reads `path`'s text: a shader or a `wesl.toml`.
pub fn reads_text(path: &Path) -> bool {
    is_shader(path) || path.as_str().rsplit('/').next() == Some(MANIFEST)
}

/// The packages bulin generates in a folder, as files under `.bulin/`, with `uniforms_package` as
/// the uniforms package's module.
pub fn generated(uniforms_package: &str) -> [(Path, String); 4] {
    let file = |package: &str, name: &str| {
        Path::new(&format!(".bulin/{package}/{name}"))
            .expect("invariant: a generated file has a valid path")
    };
    [
        (
            file(BULIN_PACKAGE_NAME, MANIFEST),
            PACKAGE_MANIFEST.to_owned(),
        ),
        (
            file(BULIN_PACKAGE_NAME, "package.wesl"),
            BULIN_PACKAGE_SOURCE.to_owned(),
        ),
        (
            file(UNIFORMS_PACKAGE_NAME, MANIFEST),
            PACKAGE_MANIFEST.to_owned(),
        ),
        (
            file(UNIFORMS_PACKAGE_NAME, "package.wesl"),
            uniforms_package.to_owned(),
        ),
    ]
}

/// The `wesl.toml` bulin creates in a folder that has none: the folder is the package root, and it
/// depends on the generated packages.
pub fn root_manifest() -> String {
    format!("{PACKAGE_MANIFEST}\n[dependencies]\n{DEPENDENCY_LINES}")
}

/// The package root a folder's `wesl.toml` declares: `None` for the folder itself.
pub fn package_root(text: &str) -> Result<Option<Path>, ManifestError> {
    // wesl folds a missing edition into its generic parse error, so look for it first.
    let table = toml::from_str::<toml::Table>(text)
        .map_err(|error| ManifestError::Invalid(error.to_string()))?;
    if !table.contains_key("edition") {
        return Err(ManifestError::MissingEdition);
    }
    let manifest =
        WeslToml::parse_str(text).map_err(|error| ManifestError::Invalid(error.to_string()))?;

    let root = &manifest.package.root;
    let written = root.to_string_lossy();
    if root.is_absolute() {
        return Err(ManifestError::Escapes(written.into_owned()));
    }
    let mut relative = written.as_ref();
    while let Some(rest) = relative.strip_prefix("./") {
        relative = rest;
    }
    let relative = relative.trim_end_matches('/');
    if relative.is_empty() || relative == "." {
        return Ok(None);
    }
    Path::new(relative)
        .map(Some)
        .map_err(|_| ManifestError::Escapes(written.into_owned()))
}

/// Whether a folder's `wesl.toml` declares the generated packages as the path dependencies
/// [`DEPENDENCY_LINES`] gives. A file that doesn't parse declares nothing.
pub fn declares_dependencies(text: &str) -> bool {
    let Ok(manifest) = WeslToml::parse_str(text) else {
        return false;
    };
    let DependenciesConfig::Manual(dependencies) = manifest.dependencies else {
        return false;
    };
    [BULIN_PACKAGE_NAME, UNIFORMS_PACKAGE_NAME]
        .into_iter()
        .all(|package| match dependencies.get(package) {
            Some(DependencySpec::Path(path)) => {
                let path = path.strip_prefix("./").unwrap_or(path);
                path.trim_end_matches('/') == format!(".bulin/{package}")
            }
            Some(DependencySpec::Auto | DependencySpec::Package(_)) | None => false,
        })
}

/// Whether two texts are equal once their line endings are: storage may hold `\r\n` or `\r`
/// where the editor, which normalizes them on load, holds `\n`.
pub fn same_text(a: &str, b: &str) -> bool {
    a == b || normalized(a).eq(normalized(b))
}

/// `text`'s bytes with every `\r\n` and lone `\r` read as `\n`.
fn normalized(text: &str) -> impl Iterator<Item = u8> + '_ {
    let mut bytes = text.bytes().peekable();
    std::iter::from_fn(move || {
        let byte = bytes.next()?;
        if byte == b'\r' {
            bytes.next_if_eq(&b'\n');
            return Some(b'\n');
        }
        Some(byte)
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::uniform::Type;

    fn path(text: &str) -> Path {
        Path::new(text).unwrap()
    }

    fn mirror(files: &[(&str, &str)]) -> Mirror {
        Mirror(Arc::new(
            files
                .iter()
                .map(|(file, text)| (path(file), (*text).to_owned()))
                .collect(),
        ))
    }

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
        assert_eq!(path.name(), "noise.wesl");
        assert_eq!(path.parent(), Some(shaders.clone()));
        assert_eq!(shaders.parent(), None);
        assert_eq!(Path::in_dir(Some(&shaders), "noise.wesl"), Ok(path.clone()));
        assert_eq!(Path::in_dir(None, "shaders"), Ok(shaders.clone()));
        assert!(Path::in_dir(None, "..").is_err());
        assert_eq!(
            path.strip_prefix(&Path::new("shade").unwrap()),
            None,
            "a prefix must end at a separator"
        );
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(0.5)).unwrap();
        let mut clock = Clock::started();
        clock.advance(Duration::from_secs(2));
        let settings = Settings::new(Some(path("shaders/main.wesl")), uniforms, Some(clock));

        assert_eq!(Settings::from_json(&settings.to_json()), Ok(settings));
        assert_eq!(
            Settings::from_json(&Settings::default().to_json()),
            Ok(Settings::default())
        );
    }

    #[test]
    fn settings_reject_unknown_versions_and_garbage() {
        assert_eq!(
            Settings::from_json(r#"{"version": 2}"#),
            Err(SettingsError::Unsupported(2))
        );
        assert_eq!(
            Settings::from_json("{}"),
            Err(SettingsError::Invalid("version missing".to_owned()))
        );
        assert!(matches!(
            Settings::from_json("not json"),
            Err(SettingsError::Invalid(_))
        ));

        let mut json = serde_json::to_value(Settings::default()).unwrap();
        json["main"] = "../x.wesl".into();
        assert!(matches!(
            Settings::from_json(&json.to_string()),
            Err(SettingsError::Invalid(_))
        ));
    }

    #[test]
    fn generated_files_are_the_packages() {
        let uniforms = "struct Customs { radius: f32 }";
        let files = generated(uniforms)
            .map(|(path, text)| (path.as_str().to_owned(), text))
            .to_vec();

        assert_eq!(
            files,
            [
                (".bulin/bulin/wesl.toml", PACKAGE_MANIFEST),
                (".bulin/bulin/package.wesl", BULIN_PACKAGE_SOURCE),
                (".bulin/uniforms/wesl.toml", PACKAGE_MANIFEST),
                (".bulin/uniforms/package.wesl", uniforms),
            ]
            .map(|(path, text)| (path.to_owned(), text.to_owned()))
        );
        assert!(files.iter().all(|(file, _)| is_skipped(&path(file))));

        let manifest = root_manifest();
        assert!(WeslToml::parse_str(&manifest).is_ok());
        assert!(declares_dependencies(&manifest));
        assert_eq!(package_root(&manifest), Ok(None));
    }

    #[test]
    fn package_root_cases() {
        let root = |line: &str| package_root(&format!("edition = \"2026_pre\"\n{line}\n"));

        assert_eq!(root("root = \".\""), Ok(None));
        assert_eq!(root("root = \"./\""), Ok(None));
        assert_eq!(root("root = \"./shaders/\""), Ok(Some(path("shaders"))));
        assert_eq!(root("root = \"a/b\""), Ok(Some(path("a/b"))));
        assert_eq!(root(""), Ok(Some(path("shaders"))));
        assert_eq!(
            root("root = \"../x\""),
            Err(ManifestError::Escapes("../x".to_owned()))
        );
        assert_eq!(
            root("root = \"/abs\""),
            Err(ManifestError::Escapes("/abs".to_owned()))
        );
        assert_eq!(
            package_root("root = \".\"\n"),
            Err(ManifestError::MissingEdition)
        );
        assert!(matches!(
            package_root("edition = "),
            Err(ManifestError::Invalid(_))
        ));
        assert!(matches!(
            root("[dependencies]\nbulin = 3"),
            Err(ManifestError::Invalid(_))
        ));
    }

    #[test]
    fn declares_dependencies_cases() {
        assert!(declares_dependencies(&root_manifest()));
        assert!(declares_dependencies(&format!(
            "{PACKAGE_MANIFEST}[dependencies]\n\
             bulin = {{ path = \"./.bulin/bulin/\" }}\n\
             uniforms = {{ path = \".bulin/uniforms\" }}\n"
        )));
        assert!(!declares_dependencies(PACKAGE_MANIFEST));
        assert!(!declares_dependencies(&format!(
            "{PACKAGE_MANIFEST}[dependencies]\nbulin = {{ path = \".bulin/bulin\" }}\n"
        )));
        assert!(!declares_dependencies(&format!(
            "{PACKAGE_MANIFEST}[dependencies]\n\
             bulin = {{ path = \"bulin\" }}\n\
             uniforms = {{ path = \".bulin/uniforms\" }}\n"
        )));
        assert!(!declares_dependencies("garbage ="));
    }

    #[test]
    fn same_text_ignores_crlf() {
        assert!(same_text("a\r\nb", "a\nb"));
        assert!(same_text("a\rb\r\n", "a\nb\n"));
        assert!(!same_text("a", "b"));
        assert!(!same_text("a\n\nb", "a\nb"));
    }

    #[test]
    fn expect_matches() {
        assert!(Expect::Missing.matches(None));
        assert!(!Expect::Missing.matches(Some("")));
        assert!(Expect::Text("a\nb".to_owned()).matches(Some("a\r\nb")));
        assert!(!Expect::Text("a".to_owned()).matches(Some("b")));
        assert!(!Expect::Text(String::new()).matches(None));
        assert!(Expect::Anything.matches(None));
        assert!(Expect::Anything.matches(Some("a")));
    }

    #[test]
    fn a_directory_removal_reports_every_file_under_it() {
        let mut mirror = mirror(&[
            ("a/x.wesl", "x"),
            ("a/b/y.wgsl", "y"),
            ("a/b/wesl.toml", "t"),
            ("ab.wesl", "ab"),
        ]);

        assert_eq!(
            mirror.apply(&Change::Removed(path("a"))),
            [
                (path("a/b/wesl.toml"), Effect::Removed),
                (path("a/b/y.wgsl"), Effect::Removed),
                (path("a/x.wesl"), Effect::Removed),
            ]
        );
        assert_eq!(
            mirror.files().keys().collect::<Vec<_>>(),
            [&path("ab.wesl")]
        );
    }

    #[test]
    fn apply_reports_only_real_changes() {
        let mut mirror = mirror(&[("x.wesl", "a\nb")]);
        let changed = |file: &str, text: Option<&str>| Change::Changed {
            path: path(file),
            text: text.map(str::to_owned),
        };

        assert_eq!(mirror.apply(&changed("x.wesl", Some("a\r\nb"))), []);
        assert_eq!(
            mirror.apply(&changed("x.wesl", Some("c"))),
            [(path("x.wesl"), Effect::Changed)]
        );
        assert_eq!(
            mirror.apply(&changed("y.wgsl", Some("y"))),
            [(path("y.wgsl"), Effect::Created)]
        );
        assert_eq!(
            mirror.apply(&changed("y.wgsl", None)),
            [(path("y.wgsl"), Effect::Removed)]
        );
        assert_eq!(mirror.apply(&changed("y.wgsl", None)), []);
        assert_eq!(mirror.apply(&changed("notes.md", None)), []);
        assert_eq!(
            mirror.apply(&changed(".bulin/uniforms/package.wesl", Some("u"))),
            []
        );
        assert_eq!(mirror.apply(&Change::Removed(path("missing"))), []);
        assert_eq!(mirror.files().keys().collect::<Vec<_>>(), [&path("x.wesl")]);
    }

    #[test]
    fn a_rescan_after_deletions_removes_them() {
        let mut mirror = mirror(&[("x.wesl", "x"), ("y.wesl", "y\n"), ("z.wesl", "z")]);
        let scan = BTreeMap::from([
            (path("y.wesl"), Some("y\r\n".to_owned())),
            (path("z.wesl"), Some("z2".to_owned())),
            (path("big.wesl"), None),
        ]);

        let changes = mirror.replace(&scan);
        assert_eq!(
            changes,
            [
                Change::Changed {
                    path: path("big.wesl"),
                    text: None
                },
                Change::Changed {
                    path: path("z.wesl"),
                    text: Some("z2".to_owned())
                },
                Change::Removed(path("x.wesl")),
            ]
        );

        for change in &changes {
            mirror.apply(change);
        }
        assert!(!mirror.files().contains_key(&path("x.wesl")));
        assert_eq!(mirror.files()[&path("z.wesl")], "z2");
        assert!(
            mirror
                .replace(&scan)
                .iter()
                .all(|change| matches!(change, Change::Changed { text: None, .. }))
        );
    }

    #[test]
    fn skipped_and_text_paths() {
        assert!(is_skipped(&path(".git")));
        assert!(is_skipped(&path("a/node_modules/b.wesl")));
        assert!(!is_skipped(&path("targets/a.wesl")));
        assert!(reads_text(&path("a/wesl.toml")));
        assert!(reads_text(&path("a.wgsl")));
        assert!(!reads_text(&path("a.toml")));
        assert!(!reads_text(&path("notes.md")));
    }
}
