mod editor;
mod layout;
mod viewer;

use iced::widget::container;
use iced::{Element, Length, Task, Theme};

pub type FragmentShader = String;

pub struct Application {
    editor: editor::Editor,
    viewer: viewer::Viewer,
    layout: layout::Layout,
}

#[derive(Debug, Clone)]
pub enum Message {
    Editor(editor::Message),
    Viewer(viewer::Message),
    Layout(layout::Message),
}

impl Application {
    pub fn new() -> (Self, Task<Message>) {
        let (editor, editor_task) = editor::Editor::new();
        (
            Self {
                editor,
                viewer: viewer::Viewer::new(),
                layout: layout::Layout::new(),
            },
            editor_task.map(Message::Editor),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Editor(message) => match message {
                editor::Message::UpdatePipeline(shader) => self
                    .viewer
                    .update(viewer::Message::UpdatePipeline(shader))
                    .map(Message::Viewer),
                _ => self.editor.update(message).map(Message::Editor),
            },
            Message::Viewer(message) => self.viewer.update(message).map(Message::Viewer),
            Message::Layout(message) => self.layout.update(message).map(Message::Layout),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let panes = self
            .layout
            .view(|_, pane, _| {
                let element = match pane {
                    layout::PaneContent::Editor => self.editor.view().map(Message::Editor),
                    layout::PaneContent::Viewer => self.viewer.view().map(Message::Viewer),
                };

                element.into()
            })
            .on_click(|e| Message::Layout(layout::Message::Clicked(e)))
            .on_drag(|e| Message::Layout(layout::Message::Dragged(e)))
            .on_resize(10, |e| Message::Layout(layout::Message::Resized(e)));

        container(panes)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn theme(&self) -> Theme {
        self.editor.theme()
    }
}
