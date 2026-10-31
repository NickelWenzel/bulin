//! The questions bulin asks before it would lose unsaved work or overwrite someone else's.

use iced::Element;
use iced::widget::{button, column, container, row, text};

use crate::Pending;
use crate::core::folder::Path;

/// A question the user answers before bulin goes on.
pub enum Prompt {
    /// Something has unsaved changes, and `then` would replace them.
    Unsaved { then: Pending },
    /// The folder file `path`, open with unsaved changes, changed or was deleted on disk.
    ChangedOnDisk { path: Path, deleted: bool },
    /// Saving the folder file `path` found it changed or deleted on disk since bulin read it.
    Conflict { path: Path, deleted: bool },
    /// The user asked to delete the folder file or directory `path`.
    Delete { path: Path },
}

/// How the user answered a [`Prompt`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Cancel,
    Reload,
    KeepMine,
    Overwrite,
    Delete,
}

impl Prompt {
    /// The question with a button for each answer it takes.
    pub fn view(&self) -> Element<'_, Answer> {
        let (question, answers): (String, &[(&str, Answer)]) = match self {
            Prompt::Unsaved { .. } => (
                "There are unsaved changes. Save them before going on?".to_owned(),
                &[
                    ("Save", Answer::Save),
                    ("Discard", Answer::Discard),
                    ("Cancel", Answer::Cancel),
                ],
            ),
            Prompt::ChangedOnDisk {
                path,
                deleted: false,
            } => (
                format!("{path} changed on disk, and you have unsaved changes to it."),
                &[("Reload", Answer::Reload), ("Keep mine", Answer::KeepMine)],
            ),
            // There is nothing to reload.
            Prompt::ChangedOnDisk {
                path,
                deleted: true,
            } => (
                format!("{path} was deleted on disk. Saving it again recreates it."),
                &[("Keep mine", Answer::KeepMine)],
            ),
            Prompt::Conflict {
                path,
                deleted: false,
            } => (
                format!("{path} changed on disk since bulin read it, so it wasn't saved."),
                &[
                    ("Overwrite", Answer::Overwrite),
                    ("Reload", Answer::Reload),
                    ("Cancel", Answer::Cancel),
                ],
            ),
            Prompt::Conflict {
                path,
                deleted: true,
            } => (
                format!("{path} was deleted on disk, so it wasn't saved."),
                &[("Overwrite", Answer::Overwrite), ("Cancel", Answer::Cancel)],
            ),
            Prompt::Delete { path } => (
                format!("Delete {path}?"),
                &[("Delete", Answer::Delete), ("Cancel", Answer::Cancel)],
            ),
        };

        let buttons = row(answers
            .iter()
            .map(|(label, answer)| button(text(*label)).on_press(*answer).into()))
        .spacing(10);

        container(column![text(question), buttons].spacing(20))
            .width(400)
            .padding(20)
            .style(container::rounded_box)
            .into()
    }
}
