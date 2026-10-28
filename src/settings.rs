//! The user's settings: the modal that edits them, where they are stored and how the editor
//! takes them.

pub mod keymap;
pub mod storage;

use iced::widget::{button, checkbox, column, container, row, slider, text};
use iced::{Center, Element, Fill};

use crate::action::Action;
use crate::core;

/// The settings modal. The settings themselves belong to its parent.
pub struct State {}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleInlayHints(bool),
    ToggleWhitespace(bool),
    ToggleFind(bool),
    FontSize(f32),
    FontSizeReleased,
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
        Self {}
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
                container(button(text("Close")).on_press(Message::Close)).align_right(Fill),
            ]
            .spacing(10),
        )
        .width(360.0)
        .padding(10)
        .style(container::rounded_box)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::{Instruction, Message, State};
    use crate::core;

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
