//! The settings file: the editor options and shortcuts bulin keeps for the user across projects.

pub mod chord;
pub mod command;

pub use chord::Chord;
pub use command::Command;

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

/// The format version this build writes and reads.
const VERSION: u32 = 1;

/// Everything bulin remembers for the user. A field missing from the file keeps its default.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Settings {
    version: u32,
    #[serde(default)]
    pub editor: Editor,
    #[serde(default)]
    pub keybindings: Keybindings,
}

/// How the shader editor looks and behaves, named like VS Code's options.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Editor {
    pub inlay_hints: bool,
    pub render_whitespace: bool,
    /// The text size in logical pixels, within [`Editor::FONT_SIZES`].
    pub font_size: f32,
    /// Whether the find-and-replace bar is available.
    pub find: bool,
}

/// The chords the user bound differently from the defaults.
///
/// A command without an override keeps its defaults, so a changed default reaches users who
/// never touched it. The effective bindings are the overrides plus every default no override
/// claims.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(from = "BTreeMap<String, serde_json::Value>")]
pub struct Keybindings(BTreeMap<Command, Option<Chord>>);

/// A binding that hides another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// Several commands are bound to the same chord.
    Shared {
        chord: Chord,
        commands: Vec<Command>,
    },
    /// A command is bound to a chord the editor already uses for something else.
    Shadowed { command: Command, chord: Chord },
}

/// Why a file couldn't be read as settings.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("not bulin settings ({0})")]
    Invalid(String),
    #[error("bulin settings version {0} is not supported, only version {VERSION}")]
    Unsupported(u32),
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            editor: Editor::default(),
            keybindings: Keybindings::default(),
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            inlay_hints: true,
            render_whitespace: false,
            font_size: 14.0,
            find: true,
        }
    }
}

/// Skips an entry whose command or chord doesn't parse, so one bad line keeps the rest.
impl From<BTreeMap<String, serde_json::Value>> for Keybindings {
    fn from(entries: BTreeMap<String, serde_json::Value>) -> Self {
        Self(
            entries
                .into_iter()
                .filter_map(|(command, chord)| {
                    Some((
                        serde_json::from_value(serde_json::Value::String(command)).ok()?,
                        serde_json::from_value(chord).ok()?,
                    ))
                })
                .collect(),
        )
    }
}

impl Settings {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("invariant: settings serialize to JSON")
    }

    pub fn from_json(json: &str) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Header {
            version: Option<u32>,
        }

        let header: Header =
            serde_json::from_str(json).map_err(|error| Error::Invalid(error.to_string()))?;
        match header.version {
            None => Err(Error::Invalid("version missing".to_owned())),
            Some(VERSION) => {
                let mut settings: Self = serde_json::from_str(json)
                    .map_err(|error| Error::Invalid(error.to_string()))?;
                settings.editor.font_size = settings
                    .editor
                    .font_size
                    .clamp(*Editor::FONT_SIZES.start(), *Editor::FONT_SIZES.end());
                Ok(settings)
            }
            Some(version) => Err(Error::Unsupported(version)),
        }
    }
}

impl Editor {
    /// The font sizes the editor accepts; a stored size outside them is clamped on load.
    pub const FONT_SIZES: RangeInclusive<f32> = 8.0..=32.0;
}

impl Keybindings {
    /// The chords `command` answers to.
    pub fn chords(&self, command: Command) -> Vec<Chord> {
        self.effective()
            .filter(|(bound, _)| *bound == command)
            .map(|(_, chord)| chord)
            .collect()
    }

    /// The command bound to `chord`. When overrides share it, the first in [`Command::ALL`]
    /// wins, and [`Keybindings::conflicts`] reports it.
    pub fn command(&self, chord: &Chord) -> Option<Command> {
        self.effective()
            .find(|(_, bound)| bound == chord)
            .map(|(command, _)| command)
    }

    /// Binds `command` to `chord` alone, replacing its defaults.
    pub fn set(&mut self, command: Command, chord: Chord) {
        self.0.insert(command, Some(chord));
    }

    /// Gives `command` its defaults back.
    pub fn reset(&mut self, command: Command) {
        self.0.remove(&command);
    }

    /// Leaves `command` without a chord.
    pub fn unbind(&mut self, command: Command) {
        self.0.insert(command, None);
    }

    /// Whether the user bound `command` differently from its defaults.
    pub fn is_overridden(&self, command: Command) -> bool {
        self.0.contains_key(&command)
    }

    /// Overrides that share a chord, and overrides onto a chord in [`command::SHADOWED`].
    pub fn conflicts(&self) -> Vec<Conflict> {
        let mut shared: Vec<(Chord, Vec<Command>)> = Vec::new();
        let mut conflicts = Vec::new();
        for (&command, chord) in &self.0 {
            let Some(chord) = chord else {
                continue;
            };
            if command::SHADOWED.contains(&chord.to_string().as_str()) {
                conflicts.push(Conflict::Shadowed {
                    command,
                    chord: chord.clone(),
                });
            }
            match shared.iter_mut().find(|(bound, _)| bound == chord) {
                Some((_, commands)) => commands.push(command),
                None => shared.push((chord.clone(), vec![command])),
            }
        }
        conflicts.extend(
            shared
                .into_iter()
                .filter(|(_, commands)| commands.len() > 1)
                .map(|(chord, commands)| Conflict::Shared { chord, commands }),
        );
        conflicts
    }

    /// Every binding in force, in [`Command::ALL`] order: the overrides, and each default no
    /// override claims.
    fn effective(&self) -> impl Iterator<Item = (Command, Chord)> + '_ {
        let claimed: Vec<&Chord> = self.0.values().flatten().collect();
        Command::ALL.into_iter().flat_map(move |command| {
            let chords: Vec<Chord> = match self.0.get(&command) {
                Some(chord) => chord.iter().cloned().collect(),
                None => command
                    .defaults()
                    .into_iter()
                    .filter(|chord| !claimed.contains(&chord))
                    .collect(),
            };
            chords.into_iter().map(move |chord| (command, chord))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        text.parse().unwrap()
    }

    #[test]
    fn chords_find_their_default_command() {
        let keybindings = Keybindings::default();

        assert_eq!(
            keybindings.command(&chord("ctrl+alt+w")),
            Some(Command::ToggleWhitespace)
        );
        assert_eq!(
            keybindings.command(&chord("ctrl+shift+z")),
            Some(Command::Redo)
        );
        assert_eq!(
            keybindings.chords(Command::Redo),
            [chord("ctrl+y"), chord("ctrl+shift+z")]
        );
        assert_eq!(keybindings.command(&chord("ctrl+e")), None);
    }

    #[test]
    fn an_override_replaces_its_commands_defaults() {
        let mut keybindings = Keybindings::default();
        keybindings.set(Command::Redo, chord("ctrl+e"));

        assert_eq!(keybindings.chords(Command::Redo), [chord("ctrl+e")]);
        assert_eq!(keybindings.command(&chord("ctrl+e")), Some(Command::Redo));
        assert_eq!(keybindings.command(&chord("ctrl+y")), None);
    }

    #[test]
    fn an_override_beats_another_commands_default() {
        let mut keybindings = Keybindings::default();
        keybindings.set(Command::ToggleWhitespace, chord("ctrl+z"));

        assert_eq!(
            keybindings.command(&chord("ctrl+z")),
            Some(Command::ToggleWhitespace)
        );
        assert_eq!(keybindings.chords(Command::Undo), []);
        assert_eq!(keybindings.conflicts(), []);
    }

    #[test]
    fn unbinding_and_resetting_edit_the_overrides() {
        let mut keybindings = Keybindings::default();
        keybindings.unbind(Command::Undo);

        assert!(keybindings.is_overridden(Command::Undo));
        assert_eq!(keybindings.chords(Command::Undo), []);
        assert_eq!(keybindings.command(&chord("ctrl+z")), None);

        keybindings.reset(Command::Undo);

        assert!(!keybindings.is_overridden(Command::Undo));
        assert_eq!(keybindings, Keybindings::default());
        assert_eq!(keybindings.command(&chord("ctrl+z")), Some(Command::Undo));
    }

    #[test]
    fn conflicts_report_shared_and_shadowing_overrides() {
        let mut keybindings = Keybindings::default();
        keybindings.set(Command::ToggleInlayHints, chord("ctrl+e"));
        keybindings.set(Command::ToggleWhitespace, chord("ctrl+e"));
        keybindings.set(Command::Format, chord("ctrl+a"));

        assert_eq!(
            keybindings.conflicts(),
            [
                Conflict::Shadowed {
                    command: Command::Format,
                    chord: chord("ctrl+a"),
                },
                Conflict::Shared {
                    chord: chord("ctrl+e"),
                    commands: vec![Command::ToggleInlayHints, Command::ToggleWhitespace],
                },
            ]
        );
        assert_eq!(
            keybindings.command(&chord("ctrl+e")),
            Some(Command::ToggleInlayHints)
        );
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let mut settings = Settings::default();
        settings.editor.render_whitespace = true;
        settings
            .keybindings
            .set(Command::Rename, chord("ctrl+plus"));
        settings.keybindings.unbind(Command::Undo);

        assert_eq!(
            Settings::from_json(&Settings::default().to_json()),
            Ok(Settings::default())
        );
        assert_eq!(Settings::from_json(&settings.to_json()), Ok(settings));
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let settings = Settings::from_json(r#"{"version": 1, "editor": {"fontSize": 18}}"#);

        let mut expected = Settings::default();
        expected.editor.font_size = 18.0;
        assert_eq!(settings, Ok(expected));
    }

    #[test]
    fn font_sizes_are_clamped_on_load() {
        let settings = Settings::from_json(r#"{"version": 1, "editor": {"fontSize": 100}}"#);

        assert_eq!(settings.map(|settings| settings.editor.font_size), Ok(32.0));
    }

    #[test]
    fn bad_keybinding_entries_are_skipped() {
        let settings = Settings::from_json(
            r#"{"version": 1, "keybindings": {
                "undo": "ctrl+e",
                "redo": null,
                "explode": "ctrl+x",
                "format": "ctrl+wat",
                "rename": 5
            }}"#,
        )
        .unwrap();

        let mut expected = Keybindings::default();
        expected.set(Command::Undo, chord("ctrl+e"));
        expected.unbind(Command::Redo);
        assert_eq!(settings.keybindings, expected);
    }

    #[test]
    fn files_without_a_version_are_not_settings() {
        assert_eq!(
            Settings::from_json(r#"{"editor": {}}"#),
            Err(Error::Invalid("version missing".to_owned()))
        );
        assert_eq!(
            Settings::from_json(r#"{"version": 2}"#),
            Err(Error::Unsupported(2))
        );
    }
}
