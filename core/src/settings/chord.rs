//! Chords: a key with the modifiers held, written VS Code style as `ctrl+alt+w`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A key pressed with modifiers, as a shortcut is bound to it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// The Super, Windows or Command key, written `meta`.
    pub logo: bool,
}

/// The key of a [`Chord`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// A character key as it reads without Shift (`\`, not `|`), ASCII-lowercased.
    Character(String),
    Named(Named),
}

/// The keys without a character that a shortcut may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Named {
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

/// Why a string isn't a chord.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("`{0}` has no key")]
    MissingKey(String),
    #[error("`{0}` has more than one key")]
    ExtraKey(String),
    #[error("`{0}` names a modifier twice")]
    RepeatedModifier(String),
    #[error("`{0}` is not a key")]
    UnknownKey(String),
}

/// Why a chord can't be bound to a command.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    #[error("`{0}` is needed for typing; add Ctrl, Alt or Meta")]
    Typing(Chord),
    #[error("`{0}` is taken by bulin and can't be rebound")]
    Reserved(Chord),
}

/// Chords bulin and the editor handle before any keybinding: save, the find bar, and Tab.
pub const RESERVED: [&str; 6] = ["ctrl+s", "ctrl+f", "ctrl+h", "escape", "alt+enter", "tab"];

impl Named {
    const ALL: [Named; 27] = [
        Named::F1,
        Named::F2,
        Named::F3,
        Named::F4,
        Named::F5,
        Named::F6,
        Named::F7,
        Named::F8,
        Named::F9,
        Named::F10,
        Named::F11,
        Named::F12,
        Named::Enter,
        Named::Escape,
        Named::Space,
        Named::Tab,
        Named::Backspace,
        Named::Delete,
        Named::Insert,
        Named::Home,
        Named::End,
        Named::PageUp,
        Named::PageDown,
        Named::ArrowUp,
        Named::ArrowDown,
        Named::ArrowLeft,
        Named::ArrowRight,
    ];

    /// The name VS Code writes in a chord.
    fn name(self) -> &'static str {
        match self {
            Self::F1 => "f1",
            Self::F2 => "f2",
            Self::F3 => "f3",
            Self::F4 => "f4",
            Self::F5 => "f5",
            Self::F6 => "f6",
            Self::F7 => "f7",
            Self::F8 => "f8",
            Self::F9 => "f9",
            Self::F10 => "f10",
            Self::F11 => "f11",
            Self::F12 => "f12",
            Self::Enter => "enter",
            Self::Escape => "escape",
            Self::Space => "space",
            Self::Tab => "tab",
            Self::Backspace => "backspace",
            Self::Delete => "delete",
            Self::Insert => "insert",
            Self::Home => "home",
            Self::End => "end",
            Self::PageUp => "pageup",
            Self::PageDown => "pagedown",
            Self::ArrowUp => "up",
            Self::ArrowDown => "down",
            Self::ArrowLeft => "left",
            Self::ArrowRight => "right",
        }
    }

    fn is_function(self) -> bool {
        matches!(
            self,
            Self::F1
                | Self::F2
                | Self::F3
                | Self::F4
                | Self::F5
                | Self::F6
                | Self::F7
                | Self::F8
                | Self::F9
                | Self::F10
                | Self::F11
                | Self::F12
        )
    }
}

impl Chord {
    /// Whether the chord can be bound, and why not. Without Ctrl, Alt or Meta only a function
    /// key leaves typing and editing alone.
    pub fn check(&self) -> Result<(), Refusal> {
        if RESERVED.contains(&self.to_string().as_str()) {
            return Err(Refusal::Reserved(self.clone()));
        }
        let function = matches!(self.key, Key::Named(named) if named.is_function());
        if !(self.ctrl || self.alt || self.logo || function) {
            return Err(Refusal::Typing(self.clone()));
        }
        Ok(())
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (held, name) in [
            (self.ctrl, "ctrl+"),
            (self.shift, "shift+"),
            (self.alt, "alt+"),
            (self.logo, "meta+"),
        ] {
            if held {
                f.write_str(name)?;
            }
        }
        match &self.key {
            Key::Character(c) if c == "+" => f.write_str("plus"),
            Key::Character(c) => f.write_str(c),
            Key::Named(named) => f.write_str(named.name()),
        }
    }
}

impl FromStr for Chord {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Error> {
        let lowered = text.to_ascii_lowercase();
        let mut chord_key = None;
        let mut modifiers = [false; 4];
        for part in lowered.split('+') {
            let modifier = ["ctrl", "shift", "alt", "meta"]
                .iter()
                .position(|name| *name == part);
            if let Some(index) = modifier {
                if modifiers[index] {
                    return Err(Error::RepeatedModifier(text.to_owned()));
                }
                modifiers[index] = true;
                continue;
            }
            if chord_key.is_some() {
                return Err(Error::ExtraKey(text.to_owned()));
            }
            chord_key = Some(key(part).ok_or_else(|| Error::UnknownKey(part.to_owned()))?);
        }
        let [ctrl, shift, alt, logo] = modifiers;
        Ok(Self {
            key: chord_key.ok_or_else(|| Error::MissingKey(text.to_owned()))?,
            ctrl,
            shift,
            alt,
            logo,
        })
    }
}

impl TryFrom<String> for Chord {
    type Error = Error;

    fn try_from(text: String) -> Result<Self, Error> {
        text.parse()
    }
}

impl From<Chord> for String {
    fn from(chord: Chord) -> Self {
        chord.to_string()
    }
}

/// The key a lowercased chord part names: a key name, `plus`, or a single visible character.
fn key(part: &str) -> Option<Key> {
    if let Some(named) = Named::ALL.into_iter().find(|named| named.name() == part) {
        return Some(Key::Named(named));
    }
    if part == "plus" {
        return Some(Key::Character("+".to_owned()));
    }
    let mut chars = part.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_whitespace() && !c.is_control() => {
            Some(Key::Character(c.to_string()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        text.parse().unwrap()
    }

    #[test]
    fn chords_print_as_they_parse() {
        for text in [
            "ctrl+alt+w",
            "shift+alt+f",
            "f12",
            "ctrl+shift+\\",
            "ctrl+plus",
            "meta+up",
        ] {
            assert_eq!(chord(text).to_string(), text);
        }
    }

    #[test]
    fn parsing_ignores_case_and_modifier_order() {
        assert_eq!(chord("Alt+W+Ctrl"), chord("ctrl+alt+w"));
        assert_eq!(chord("SHIFT+META+PageUp").to_string(), "shift+meta+pageup");
        assert_eq!(
            chord("ctrl+plus").key,
            Key::Character("+".to_owned()),
            "`plus` is the + key"
        );
    }

    #[test]
    fn malformed_chords_do_not_parse() {
        assert_eq!(
            "ctrl+shift".parse::<Chord>(),
            Err(Error::MissingKey("ctrl+shift".to_owned()))
        );
        assert_eq!(
            "ctrl+a+b".parse::<Chord>(),
            Err(Error::ExtraKey("ctrl+a+b".to_owned()))
        );
        assert_eq!(
            "ctrl+ctrl+a".parse::<Chord>(),
            Err(Error::RepeatedModifier("ctrl+ctrl+a".to_owned()))
        );
        assert_eq!(
            "ctrl+wat".parse::<Chord>(),
            Err(Error::UnknownKey("wat".to_owned()))
        );
        assert_eq!(
            "ctrl++".parse::<Chord>(),
            Err(Error::UnknownKey(String::new()))
        );
        assert_eq!("".parse::<Chord>(), Err(Error::UnknownKey(String::new())));
    }

    #[test]
    fn reserved_chords_are_canonical() {
        for text in RESERVED {
            assert_eq!(chord(text).to_string(), text);
        }
    }

    #[test]
    fn only_chords_that_leave_typing_alone_can_be_bound() {
        assert_eq!(chord("a").check(), Err(Refusal::Typing(chord("a"))));
        assert_eq!(
            chord("shift+a").check(),
            Err(Refusal::Typing(chord("shift+a")))
        );
        assert_eq!(chord("tab").check(), Err(Refusal::Reserved(chord("tab"))));
        assert_eq!(
            chord("ctrl+s").check(),
            Err(Refusal::Reserved(chord("ctrl+s")))
        );
        assert_eq!(chord("f5").check(), Ok(()));
        assert_eq!(chord("ctrl+alt+w").check(), Ok(()));
    }
}
