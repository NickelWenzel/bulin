//! The user's settings: the modal that edits them, where they are stored and how the editor
//! takes them.

pub mod keymap;
pub mod storage;

use iced::keyboard::{self, key};
use iced::widget::{button, checkbox, column, container, row, scrollable, slider, text};
use iced::{Center, Element, Event, Fill, Subscription, event};

use crate::action::Action;
use crate::core;

/// The settings modal. The settings themselves belong to its parent.
pub struct State {
    /// The command whose new chord the next key press sets.
    recording: Option<core::settings::Command>,
    /// Why the last chord pressed while recording can't be bound.
    refusal: Option<core::settings::chord::Refusal>,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleInlayHints(bool),
    ToggleWhitespace(bool),
    ToggleFind(bool),
    FontSize(f32),
    FontSizeReleased,
    Record(core::settings::Command),
    Recorded {
        key: keyboard::Key,
        physical: key::Physical,
        modifiers: keyboard::Modifiers,
    },
    CancelRecording,
    Reset(core::settings::Command),
    Unbind(core::settings::Command),
    Close,
}

pub enum Instruction {
    /// The settings changed, so the editor takes them and they are saved.
    Changed,
    /// The settings changed while the user is still choosing, so the editor takes them but
    /// they aren't saved yet.
    Previewed,
    Close,
}

impl State {
    pub fn new() -> Self {
        Self {
            recording: None,
            refusal: None,
        }
    }

    /// Whether a chord is being recorded, so every key press belongs to [`State::subscription`].
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    pub fn update(
        &mut self,
        message: Message,
        settings: &mut core::Settings,
    ) -> Action<Instruction, Message> {
        match message {
            Message::ToggleInlayHints(on) => {
                settings.editor.inlay_hints = on;
                Action::instruction(Instruction::Changed)
            }
            Message::ToggleWhitespace(on) => {
                settings.editor.render_whitespace = on;
                Action::instruction(Instruction::Changed)
            }
            Message::ToggleFind(on) => {
                settings.editor.find = on;
                Action::instruction(Instruction::Changed)
            }
            // Dragging the slider sends a size per step; only where it stops is saved.
            Message::FontSize(size) => {
                settings.editor.font_size = size;
                Action::instruction(Instruction::Previewed)
            }
            Message::FontSizeReleased => Action::instruction(Instruction::Changed),
            Message::Record(command) => {
                self.recording = Some(command);
                self.refusal = None;
                Action::none()
            }
            Message::Recorded {
                key,
                physical,
                modifiers,
            } => {
                let Some(command) = self.recording else {
                    return Action::none();
                };
                // A lone modifier starts a chord and an unsupported key makes none, so both wait.
                let Some(chord) = keymap::chord(&key, physical, modifiers) else {
                    return Action::none();
                };
                if let Err(refusal) = chord.check() {
                    self.refusal = Some(refusal);
                    return Action::none();
                }

                settings.keybindings.set(command, chord);
                self.recording = None;
                self.refusal = None;
                Action::instruction(Instruction::Changed)
            }
            Message::CancelRecording => {
                self.recording = None;
                self.refusal = None;
                Action::none()
            }
            Message::Reset(command) => {
                settings.keybindings.reset(command);
                Action::instruction(Instruction::Changed)
            }
            Message::Unbind(command) => {
                settings.keybindings.unbind(command);
                Action::instruction(Instruction::Changed)
            }
            Message::Close => Action::instruction(Instruction::Close),
        }
    }

    pub fn view<'a>(&self, settings: &'a core::Settings) -> Element<'a, Message> {
        let editor = &settings.editor;
        let font_size = row![
            text("Font size"),
            slider(
                core::settings::Editor::FONT_SIZES,
                editor.font_size,
                Message::FontSize
            )
            .on_release(Message::FontSizeReleased),
            text(editor.font_size.to_string()),
        ]
        .spacing(10)
        .align_y(Center);

        let conflicts = settings.keybindings.conflicts();
        let shortcuts = column(core::settings::Command::ALL.into_iter().map(|command| {
            shortcut(
                command,
                self.recording == Some(command),
                &settings.keybindings,
                &conflicts,
            )
        }))
        .spacing(5);

        container(
            column![
                checkbox(editor.inlay_hints)
                    .label("Inlay hints")
                    .on_toggle(Message::ToggleInlayHints),
                checkbox(editor.render_whitespace)
                    .label("Render whitespace")
                    .on_toggle(Message::ToggleWhitespace),
                checkbox(editor.find)
                    .label("Find and replace")
                    .on_toggle(Message::ToggleFind),
                font_size,
                text("Keyboard shortcuts"),
                scrollable(shortcuts).height(300.0),
                self.refusal
                    .as_ref()
                    .map(|refusal| text(refusal.to_string()).style(text::danger)),
                container(button(text("Close")).on_press(Message::Close)).align_right(Fill),
            ]
            .spacing(10),
        )
        .width(520.0)
        .padding(10)
        .style(container::rounded_box)
        .into()
    }

    /// The key presses while recording, seen even when a widget captures them so no other
    /// listener needs to run meanwhile. Escape stops recording instead of being recorded.
    pub fn subscription(&self) -> Subscription<Message> {
        event::listen_with(|event, _status, _window| match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key::Named::Escape),
                ..
            }) => Some(Message::CancelRecording),
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                repeat: false,
                ..
            }) => Some(Message::Recorded {
                key,
                physical: physical_key,
                modifiers,
            }),
            _ => None,
        })
    }
}

/// A row for `command`: its chords, which record a new one when pressed, reset and unbind, and
/// what its binding hides.
fn shortcut<'a>(
    command: core::settings::Command,
    recording: bool,
    keybindings: &core::settings::Keybindings,
    conflicts: &[core::settings::Conflict],
) -> Element<'a, Message> {
    let chords = keybindings.chords(command);
    let shown = if recording {
        "Press a shortcut…".to_owned()
    } else if chords.is_empty() {
        "unbound".to_owned()
    } else {
        chords
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };

    let markers = conflicts.iter().filter_map(|conflict| match conflict {
        core::settings::Conflict::Shared { commands, .. } if commands.contains(&command) => {
            let others = commands
                .iter()
                .filter(|&&other| other != command)
                .map(|other| other.label())
                .collect::<Vec<_>>()
                .join(", ");
            Some(
                text!("conflicts with {others}")
                    .size(12)
                    .style(text::danger)
                    .into(),
            )
        }
        core::settings::Conflict::Shadowed {
            command: shadowing, ..
        } if *shadowing == command => Some(
            text("hides a built-in shortcut")
                .size(12)
                .style(text::danger)
                .into(),
        ),
        _ => None,
    });

    row![
        column![text(command.label())].extend(markers).width(Fill),
        button(text(shown))
            .style(button::secondary)
            .on_press(Message::Record(command)),
        button(text("Reset")).on_press_maybe(
            keybindings
                .is_overridden(command)
                .then_some(Message::Reset(command))
        ),
        button(text("Unbind"))
            .on_press_maybe((!chords.is_empty()).then_some(Message::Unbind(command))),
    ]
    .spacing(5)
    .align_y(Center)
    .into()
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers, key};

    use super::{Instruction, Message, State};
    use crate::core;
    use crate::core::settings::Command;

    /// A press of the letter `c` on its US key with `modifiers` held.
    fn press(c: &str, code: key::Code, modifiers: Modifiers) -> Message {
        Message::Recorded {
            key: Key::Character(c.into()),
            physical: key::Physical::Code(code),
            modifiers,
        }
    }

    #[test]
    fn a_recorded_chord_rebinds_the_command_and_changes_the_settings() {
        let mut settings = core::Settings::default();
        let mut state = State::new();

        let _ = state.update(Message::Record(Command::ToggleWhitespace), &mut settings);
        let action = state.update(press("e", key::Code::KeyE, Modifiers::CTRL), &mut settings);

        assert!(!state.is_recording());
        assert_eq!(
            settings.keybindings.chords(Command::ToggleWhitespace),
            ["ctrl+e".parse().unwrap()]
        );
        assert!(matches!(action.instruction, Some(Instruction::Changed)));
    }

    #[test]
    fn cancelling_a_recording_leaves_the_binding() {
        let mut settings = core::Settings::default();
        let mut state = State::new();

        let _ = state.update(Message::Record(Command::Undo), &mut settings);
        let action = state.update(Message::CancelRecording, &mut settings);

        assert!(!state.is_recording());
        assert_eq!(settings, core::Settings::default());
        assert!(action.instruction.is_none());
    }

    #[test]
    fn a_lone_modifier_keeps_recording() {
        let mut settings = core::Settings::default();
        let mut state = State::new();

        let _ = state.update(Message::Record(Command::Undo), &mut settings);
        let action = state.update(
            Message::Recorded {
                key: Key::Named(key::Named::Control),
                physical: key::Physical::Code(key::Code::ControlLeft),
                modifiers: Modifiers::CTRL,
            },
            &mut settings,
        );

        assert!(state.is_recording());
        assert!(state.refusal.is_none());
        assert!(action.instruction.is_none());
    }

    #[test]
    fn a_bare_letter_is_refused_and_recording_goes_on() {
        let mut settings = core::Settings::default();
        let mut state = State::new();

        let _ = state.update(Message::Record(Command::Undo), &mut settings);
        let action = state.update(
            press("a", key::Code::KeyA, Modifiers::empty()),
            &mut settings,
        );

        assert!(state.is_recording());
        assert!(matches!(
            state.refusal,
            Some(core::settings::chord::Refusal::Typing(_))
        ));
        assert_eq!(settings, core::Settings::default());
        assert!(action.instruction.is_none());

        let _ = state.update(Message::Record(Command::Redo), &mut settings);
        assert!(state.refusal.is_none());
    }

    #[test]
    fn recording_a_chord_another_override_has_is_a_conflict() {
        let mut settings = core::Settings::default();
        let mut state = State::new();

        for command in [Command::ToggleInlayHints, Command::ToggleWhitespace] {
            let _ = state.update(Message::Record(command), &mut settings);
            let _ = state.update(press("e", key::Code::KeyE, Modifiers::CTRL), &mut settings);
        }

        assert_eq!(
            settings.keybindings.conflicts(),
            [core::settings::Conflict::Shared {
                chord: "ctrl+e".parse().unwrap(),
                commands: vec![Command::ToggleInlayHints, Command::ToggleWhitespace],
            }]
        );
    }

    #[test]
    fn reset_and_unbind_edit_the_binding_and_change_the_settings() {
        let mut settings = core::Settings::default();
        let mut state = State::new();

        let unbound = state.update(Message::Unbind(Command::Undo), &mut settings);
        assert_eq!(settings.keybindings.chords(Command::Undo), []);
        assert!(matches!(unbound.instruction, Some(Instruction::Changed)));

        let reset = state.update(Message::Reset(Command::Undo), &mut settings);
        assert_eq!(settings, core::Settings::default());
        assert!(matches!(reset.instruction, Some(Instruction::Changed)));
    }

    #[test]
    fn a_checkbox_sets_its_option_and_changes_the_settings() {
        let mut settings = core::Settings::default();
        let shown = settings.editor.render_whitespace;

        let action = State::new().update(Message::ToggleWhitespace(!shown), &mut settings);

        assert_eq!(settings.editor.render_whitespace, !shown);
        assert!(matches!(action.instruction, Some(Instruction::Changed)));
    }

    #[test]
    fn the_font_size_is_previewed_while_dragging_and_changed_on_release() {
        let mut settings = core::Settings::default();
        let mut state = State::new();

        let dragged = state.update(Message::FontSize(20.0), &mut settings);
        assert_eq!(settings.editor.font_size, 20.0);
        assert!(matches!(dragged.instruction, Some(Instruction::Previewed)));

        let released = state.update(Message::FontSizeReleased, &mut settings);
        assert_eq!(settings.editor.font_size, 20.0);
        assert!(matches!(released.instruction, Some(Instruction::Changed)));
    }
}
