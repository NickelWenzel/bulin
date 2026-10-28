//! The keybindings in the editor's terms.

use iced::keyboard::{Key, Modifiers, key};
use matcha_iced::Action;
use matcha_iced::keymap;

use crate::core::settings;
use crate::core::settings::chord;

/// The chord a press of `key` makes, or `None` for a lone modifier or a key no shortcut uses.
///
/// Native reports a character without Shift and the web with it, so a letter is read through
/// its Latin form and a shifted symbol through its physical key on a US layout.
pub fn chord(key: &Key, physical: key::Physical, modifiers: Modifiers) -> Option<settings::Chord> {
    let key = match key {
        Key::Character(_) => {
            let c = match key.to_latin(physical) {
                Some(c) if c.is_ascii_alphabetic() => c,
                latin => unshifted(physical).or(latin)?,
            };
            chord::Key::Character(c.to_ascii_lowercase().to_string())
        }
        Key::Named(named) => chord::Key::Named(match named {
            key::Named::F1 => chord::Named::F1,
            key::Named::F2 => chord::Named::F2,
            key::Named::F3 => chord::Named::F3,
            key::Named::F4 => chord::Named::F4,
            key::Named::F5 => chord::Named::F5,
            key::Named::F6 => chord::Named::F6,
            key::Named::F7 => chord::Named::F7,
            key::Named::F8 => chord::Named::F8,
            key::Named::F9 => chord::Named::F9,
            key::Named::F10 => chord::Named::F10,
            key::Named::F11 => chord::Named::F11,
            key::Named::F12 => chord::Named::F12,
            key::Named::Enter => chord::Named::Enter,
            key::Named::Escape => chord::Named::Escape,
            key::Named::Space => chord::Named::Space,
            key::Named::Tab => chord::Named::Tab,
            key::Named::Backspace => chord::Named::Backspace,
            key::Named::Delete => chord::Named::Delete,
            key::Named::Insert => chord::Named::Insert,
            key::Named::Home => chord::Named::Home,
            key::Named::End => chord::Named::End,
            key::Named::PageUp => chord::Named::PageUp,
            key::Named::PageDown => chord::Named::PageDown,
            key::Named::ArrowUp => chord::Named::ArrowUp,
            key::Named::ArrowDown => chord::Named::ArrowDown,
            key::Named::ArrowLeft => chord::Named::ArrowLeft,
            key::Named::ArrowRight => chord::Named::ArrowRight,
            _ => return None,
        }),
        Key::Unidentified => return None,
    };

    Some(settings::Chord {
        key,
        ctrl: modifiers.control(),
        shift: modifiers.shift(),
        alt: modifiers.alt(),
        logo: modifiers.logo(),
    })
}

/// The editor chords a press of `chord` can arrive as. With Shift held the browser reports the
/// shifted character, so `\` also binds `|`, as the editor's own Ctrl+Shift+\ does.
pub fn matcha(chord: &settings::Chord) -> Vec<keymap::Chord> {
    let mut modifiers = Modifiers::empty();
    for (held, modifier) in [
        (chord.ctrl, Modifiers::CTRL),
        (chord.shift, Modifiers::SHIFT),
        (chord.alt, Modifiers::ALT),
        (chord.logo, Modifiers::LOGO),
    ] {
        modifiers.set(modifier, held);
    }

    let keys = match &chord.key {
        chord::Key::Character(c) if c == "\\" => {
            vec![Key::Character("\\".into()), Key::Character("|".into())]
        }
        chord::Key::Character(c) => vec![Key::Character(c.as_str().into())],
        chord::Key::Named(named) => vec![Key::Named(match named {
            chord::Named::F1 => key::Named::F1,
            chord::Named::F2 => key::Named::F2,
            chord::Named::F3 => key::Named::F3,
            chord::Named::F4 => key::Named::F4,
            chord::Named::F5 => key::Named::F5,
            chord::Named::F6 => key::Named::F6,
            chord::Named::F7 => key::Named::F7,
            chord::Named::F8 => key::Named::F8,
            chord::Named::F9 => key::Named::F9,
            chord::Named::F10 => key::Named::F10,
            chord::Named::F11 => key::Named::F11,
            chord::Named::F12 => key::Named::F12,
            chord::Named::Enter => key::Named::Enter,
            chord::Named::Escape => key::Named::Escape,
            chord::Named::Space => key::Named::Space,
            chord::Named::Tab => key::Named::Tab,
            chord::Named::Backspace => key::Named::Backspace,
            chord::Named::Delete => key::Named::Delete,
            chord::Named::Insert => key::Named::Insert,
            chord::Named::Home => key::Named::Home,
            chord::Named::End => key::Named::End,
            chord::Named::PageUp => key::Named::PageUp,
            chord::Named::PageDown => key::Named::PageDown,
            chord::Named::ArrowUp => key::Named::ArrowUp,
            chord::Named::ArrowDown => key::Named::ArrowDown,
            chord::Named::ArrowLeft => key::Named::ArrowLeft,
            chord::Named::ArrowRight => key::Named::ArrowRight,
        })],
    };

    keys.into_iter()
        .map(|key| keymap::Chord::new(key, modifiers))
        .collect()
}

/// The editor's key overrides for `keybindings`, in the order a `Keymap` takes them.
///
/// The toggles' chords and the editor defaults no longer in force are unbound, so the editor
/// leaves those presses to bulin. The editor commands' chords are bound after them, so a chord
/// taken from one command by another ends up on the new one.
pub fn overrides(keybindings: &settings::Keybindings) -> Vec<(keymap::Chord, Option<Action>)> {
    let mut unbound = Vec::new();
    let mut bound = Vec::new();

    for command in settings::Command::ALL {
        let chords = keybindings.chords(command);
        let action = match command {
            settings::Command::ToggleInlayHints | settings::Command::ToggleWhitespace => {
                unbound.extend(chords);
                continue;
            }
            settings::Command::Undo => Action::Undo,
            settings::Command::Redo => Action::Redo,
            settings::Command::ToggleComment => Action::ToggleComment,
            settings::Command::DeleteLine => Action::DeleteLine,
            settings::Command::AddNextOccurrence => Action::AddNextOccurrence,
            settings::Command::SelectAllOccurrences => Action::SelectAllOccurrences,
            settings::Command::JumpToBracket => Action::JumpToBracket,
            settings::Command::TriggerCompletion => Action::TriggerCompletion,
            settings::Command::Format => Action::Format,
            settings::Command::GotoDefinition => Action::GotoDefinition,
            settings::Command::Rename => Action::Rename,
        };

        unbound.extend(
            command
                .defaults()
                .into_iter()
                .filter(|chord| !chords.contains(chord)),
        );
        bound.extend(
            chords
                .into_iter()
                .map(|chord| (chord, Some(action.clone()))),
        );
    }

    unbound
        .into_iter()
        .map(|chord| (chord, None))
        .chain(bound)
        .flat_map(|(chord, action)| {
            matcha(&chord)
                .into_iter()
                .map(move |chord| (chord, action.clone()))
        })
        .collect()
}

/// The character `physical` types without Shift on a US layout, for the keys whose shifted
/// character differs.
fn unshifted(physical: key::Physical) -> Option<char> {
    let key::Physical::Code(code) = physical else {
        return None;
    };

    Some(match code {
        key::Code::Backquote => '`',
        key::Code::Digit0 => '0',
        key::Code::Digit1 => '1',
        key::Code::Digit2 => '2',
        key::Code::Digit3 => '3',
        key::Code::Digit4 => '4',
        key::Code::Digit5 => '5',
        key::Code::Digit6 => '6',
        key::Code::Digit7 => '7',
        key::Code::Digit8 => '8',
        key::Code::Digit9 => '9',
        key::Code::Minus => '-',
        key::Code::Equal => '=',
        key::Code::BracketLeft => '[',
        key::Code::BracketRight => ']',
        key::Code::Backslash => '\\',
        key::Code::Semicolon => ';',
        key::Code::Quote => '\'',
        key::Code::Comma => ',',
        key::Code::Period => '.',
        key::Code::Slash => '/',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers, key};
    use matcha_iced::Action;
    use matcha_iced::keymap::Chord;

    use super::{chord, overrides};
    use crate::core::settings::{Command, Keybindings};

    fn ctrl(c: &str) -> Chord {
        Chord::new(Key::Character(c.into()), Modifiers::CTRL)
    }

    /// Where `chord` sits in `overrides` and what it does there, for each time it appears.
    fn find(overrides: &[(Chord, Option<Action>)], chord: &Chord) -> Vec<(usize, Option<Action>)> {
        overrides
            .iter()
            .enumerate()
            .filter(|(_, (bound, _))| bound == chord)
            .map(|(index, (_, action))| (index, action.clone()))
            .collect()
    }

    #[test]
    fn a_rebound_default_is_unbound_and_its_override_bound() {
        let mut keybindings = Keybindings::default();
        keybindings.set(Command::Undo, "ctrl+e".parse().unwrap());

        let overrides = overrides(&keybindings);

        assert!(matches!(find(&overrides, &ctrl("z"))[..], [(_, None)]));
        assert!(matches!(
            find(&overrides, &ctrl("e"))[..],
            [(_, Some(Action::Undo))]
        ));
    }

    #[test]
    fn a_chord_taken_by_another_command_is_unbound_before_it_is_bound() {
        let mut keybindings = Keybindings::default();
        keybindings.set(Command::Redo, "ctrl+z".parse().unwrap());

        let overrides = overrides(&keybindings);

        match find(&overrides, &ctrl("z"))[..] {
            [(unbound, None), (bound, Some(Action::Redo))] => assert!(unbound < bound),
            ref found => panic!("ctrl+z appears as {found:?}"),
        }
    }

    #[test]
    fn toggle_chords_are_left_to_bulin() {
        let mut keybindings = Keybindings::default();
        keybindings.set(Command::ToggleWhitespace, "ctrl+d".parse().unwrap());

        let overrides = overrides(&keybindings);

        let ctrl_alt_i = Chord::new(Key::Character("i".into()), Modifiers::CTRL | Modifiers::ALT);
        for chord in [ctrl_alt_i, ctrl("d")] {
            let found = find(&overrides, &chord);
            assert!(!found.is_empty(), "{chord:?}");
            assert!(
                found.iter().all(|(_, action)| action.is_none()),
                "{chord:?}"
            );
        }
    }

    #[test]
    fn jump_to_bracket_binds_both_backslash_and_bar() {
        let overrides = overrides(&Keybindings::default());

        for c in ["\\", "|"] {
            let chord = Chord::new(Key::Character(c.into()), Modifiers::CTRL | Modifiers::SHIFT);
            assert!(
                matches!(
                    find(&overrides, &chord)[..],
                    [(_, Some(Action::JumpToBracket))]
                ),
                "{c}"
            );
        }
    }

    #[test]
    fn a_shifted_web_letter_reads_as_its_lowercase_chord() {
        let pressed = chord(
            &Key::Character("W".into()),
            key::Physical::Code(key::Code::KeyW),
            Modifiers::CTRL | Modifiers::ALT,
        );

        assert_eq!(pressed, Some("ctrl+alt+w".parse().unwrap()));
    }

    #[test]
    fn a_shifted_web_symbol_reads_as_its_unshifted_key() {
        let pressed = chord(
            &Key::Character("|".into()),
            key::Physical::Code(key::Code::Backslash),
            Modifiers::CTRL | Modifiers::SHIFT,
        );

        assert_eq!(pressed, Some("ctrl+shift+\\".parse().unwrap()));
    }

    #[test]
    fn a_lone_modifier_is_no_chord() {
        let pressed = chord(
            &Key::Named(key::Named::Shift),
            key::Physical::Code(key::Code::ShiftLeft),
            Modifiers::SHIFT,
        );

        assert_eq!(pressed, None);
    }
}
