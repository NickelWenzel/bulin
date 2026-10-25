use iced::widget::pane_grid;
use iced::widget::pane_grid::{Configuration, Content, Pane, PaneGrid};
use iced::widget::text;

pub struct State {
    panes: pane_grid::State<PaneContent>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Dragged(pane_grid::DragEvent),
    Resized(pane_grid::ResizeEvent),
}

impl State {
    pub fn new() -> Self {
        Self {
            panes: pane_grid::State::with_configuration(Configuration::Split {
                axis: pane_grid::Axis::Vertical,
                ratio: 0.5,
                a: Box::new(Configuration::Pane(PaneContent::Editor)),
                b: Box::new(Configuration::Pane(PaneContent::Viewer)),
            }),
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Resized(pane_grid::ResizeEvent { split, ratio }) => {
                self.panes.resize(split, ratio);
            }
            Message::Dragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(pane, target);
            }
            Message::Dragged(_) => {}
        }
    }

    pub fn view<'a, T>(
        &'a self,
        view: impl Fn(Pane, &PaneContent, bool) -> (Content<'a, T>, Option<String>),
    ) -> PaneGrid<'a, T> {
        pane_grid(&self.panes, move |id, pane, maximized| {
            match view(id, pane, maximized) {
                (view, Some(title)) => view.title_bar(pane_grid::TitleBar::new(text(title))),
                (view, _) => view,
            }
        })
    }
}

pub enum PaneContent {
    Editor,
    Viewer,
}
