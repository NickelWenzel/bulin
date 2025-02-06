use iced::widget::{pane_grid, text};
use iced::widget::pane_grid::{Configuration, Content, Pane, State, TitleBar};
use iced::widget::PaneGrid;
use iced::Task;

pub struct Layout {
    panes: State<PaneContent>,
    focus: Option<pane_grid::Pane>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Clicked(pane_grid::Pane),
    Dragged(pane_grid::DragEvent),
    Resized(pane_grid::ResizeEvent),
    Maximize(pane_grid::Pane),
    Restore,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            panes: pane_grid::State::with_configuration(Configuration::Split {
                axis: pane_grid::Axis::Vertical,
                ratio: 0.5,
                a: Box::new(Configuration::Pane(PaneContent::Editor)),
                b: Box::new(Configuration::Pane(PaneContent::Viewer)),
            }),
            focus: None,
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Clicked(pane) => {
                self.focus = Some(pane);
                Task::none()
            }
            Message::Resized(pane_grid::ResizeEvent { split, ratio }) => {
                self.panes.resize(split, ratio);
                Task::none()
            }
            Message::Dragged(pane_grid::DragEvent::Dropped { pane, target }) => {
                self.panes.drop(pane, target);
                Task::none()
            }
            Message::Dragged(_) => Task::none(),
            Message::Maximize(pane) => {
                self.panes.maximize(pane);
                Task::none()
            }
            Message::Restore => {
                self.panes.restore();
                Task::none()
            }
        }
    }

    pub fn view<'a, T>(
        &'a self,
        view: impl Fn(Pane, &PaneContent, bool) -> (Content<'a, T>, Option<String>),
    ) -> PaneGrid<'a, T> {
        PaneGrid::new(&self.panes, move |id, pane, maximized| {
            match view(id, pane, maximized) {
                (view, Some(title)) => view.title_bar(TitleBar::new(text(title))),
                (view, _) => view
            }
        })
    }
}

pub enum PaneContent {
    Editor,
    Viewer,
}
