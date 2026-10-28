//! The commands a user can bind to a chord.

use serde::{Deserialize, Serialize};

use super::Chord;

/// A bindable command: one of bulin's toggles or one of the editor's own actions.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    ToggleInlayHints,
    ToggleWhitespace,
    Undo,
    Redo,
    ToggleComment,
    DeleteLine,
    AddNextOccurrence,
    SelectAllOccurrences,
    JumpToBracket,
    TriggerCompletion,
    Format,
    GotoDefinition,
    Rename,
}

/// The editor's built-in chords for actions outside [`Command`], in canonical form. Binding one
/// of them hides what the editor does on it.
pub const SHADOWED: [&str; 33] = [
    "ctrl+a",
    "ctrl+c",
    "ctrl+v",
    "ctrl+x",
    "ctrl+u",
    "ctrl+[",
    "ctrl+]",
    "ctrl+shift+[",
    "ctrl+shift+]",
    "ctrl+enter",
    "ctrl+shift+enter",
    "ctrl+left",
    "ctrl+right",
    "ctrl+shift+left",
    "ctrl+shift+right",
    "ctrl+home",
    "ctrl+end",
    "ctrl+shift+home",
    "ctrl+shift+end",
    "ctrl+backspace",
    "ctrl+delete",
    "alt+up",
    "alt+down",
    "shift+alt+up",
    "shift+alt+down",
    "ctrl+shift+up",
    "ctrl+shift+down",
    "ctrl+shift+alt+up",
    "ctrl+shift+alt+down",
    "shift+alt+left",
    "shift+alt+right",
    "f8",
    "shift+f8",
];

impl Command {
    pub const ALL: [Command; 13] = [
        Command::ToggleInlayHints,
        Command::ToggleWhitespace,
        Command::Undo,
        Command::Redo,
        Command::ToggleComment,
        Command::DeleteLine,
        Command::AddNextOccurrence,
        Command::SelectAllOccurrences,
        Command::JumpToBracket,
        Command::TriggerCompletion,
        Command::Format,
        Command::GotoDefinition,
        Command::Rename,
    ];

    /// The name shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            Self::ToggleInlayHints => "Toggle Inlay Hints",
            Self::ToggleWhitespace => "Toggle Whitespace",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::ToggleComment => "Toggle Line Comment",
            Self::DeleteLine => "Delete Line",
            Self::AddNextOccurrence => "Add Next Occurrence",
            Self::SelectAllOccurrences => "Select All Occurrences",
            Self::JumpToBracket => "Jump to Bracket",
            Self::TriggerCompletion => "Trigger Completion",
            Self::Format => "Format Document",
            Self::GotoDefinition => "Go to Definition",
            Self::Rename => "Rename Symbol",
        }
    }

    /// The chords the command has until the user rebinds it. The editor commands' defaults are
    /// the editor's built-in chords. Browsers keep Ctrl+W, Ctrl+Shift+W, Ctrl+Shift+I and
    /// Ctrl+I for themselves, so the toggles use Ctrl+Alt, which is no AltGr character on common
    /// layouts.
    pub fn defaults(self) -> Vec<Chord> {
        let chords: &[&str] = match self {
            Self::ToggleInlayHints => &["ctrl+alt+i"],
            Self::ToggleWhitespace => &["ctrl+alt+w"],
            Self::Undo => &["ctrl+z"],
            Self::Redo => &["ctrl+y", "ctrl+shift+z"],
            Self::ToggleComment => &["ctrl+/"],
            Self::DeleteLine => &["ctrl+shift+k"],
            Self::AddNextOccurrence => &["ctrl+d"],
            Self::SelectAllOccurrences => &["ctrl+shift+l"],
            Self::JumpToBracket => &["ctrl+shift+\\"],
            Self::TriggerCompletion => &["ctrl+space"],
            Self::Format => &["shift+alt+f"],
            Self::GotoDefinition => &["f12"],
            Self::Rename => &["f2"],
        };
        chords
            .iter()
            .map(|text| text.parse().expect("invariant: default chords parse"))
            .collect()
    }

    /// Whether the editor carries the command out, rather than bulin.
    pub fn is_editor(self) -> bool {
        !matches!(self, Self::ToggleInlayHints | Self::ToggleWhitespace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_shadowed_chords_are_bindable() {
        for command in Command::ALL {
            for chord in command.defaults() {
                assert_eq!(chord.check(), Ok(()), "{chord}");
            }
        }
        for text in SHADOWED {
            let chord: Chord = text.parse().unwrap();
            assert_eq!(chord.to_string(), text, "canonical");
            assert_eq!(chord.check(), Ok(()), "{chord}");
        }
    }

    #[test]
    fn no_default_shadows_a_built_in() {
        for command in Command::ALL {
            for chord in command.defaults() {
                assert!(!SHADOWED.contains(&chord.to_string().as_str()), "{chord}");
            }
        }
    }

    #[test]
    fn commands_serialize_in_camel_case() {
        assert_eq!(
            serde_json::to_string(&Command::ToggleInlayHints).unwrap(),
            r#""toggleInlayHints""#
        );
    }
}
