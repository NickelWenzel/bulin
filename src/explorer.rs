//! The sidebar listing an open folder: directories expand when clicked and are listed only then,
//! each file opens as the document or becomes the main file, and files and directories are
//! created, renamed inline and deleted.

use std::collections::{BTreeMap, btree_map};

use iced::advanced::text::Position;
use iced::widget::{self, button, column, container, operation, row, scrollable, text, text_input};
use iced::{Center, Element, Fill, Task, padding};

use crate::action::Action;
use crate::core::folder::{Change, Entry, Kind, Path};

/// How wide the sidebar is.
const WIDTH: f32 = 320.0;

/// How far each level of directories is indented.
const INDENT: f32 = 12.0;

/// The name a new file starts with: `.wesl`, since the language server flags imports in `.wgsl`
/// files.
const UNTITLED: &str = "untitled.wesl";

/// The input a name is typed in.
const INPUT: &str = "explorer-name";

pub struct State {
    /// The listing of each expanded directory, keyed by the directory with `None` for the folder
    /// itself, and `None` while it is being listed. A directory without a key is collapsed.
    listings: BTreeMap<Option<Path>, Option<Vec<Entry>>>,
    /// The name being typed, while one is.
    editing: Option<Edit>,
    /// Why the name submitted last can't be used, until it is edited.
    problem: Option<String>,
}

/// A name being typed for a new entry or an existing one.
enum Edit {
    /// A new file or directory in `parent`, `None` being the folder itself.
    New {
        parent: Option<Path>,
        kind: Kind,
        name: String,
    },
    /// A new name for `from`, in the same directory.
    Rename { from: Path, name: String },
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Expand or collapse a directory.
    Toggle(Path),
    Open(Path),
    SetMain(Path),
    /// Start naming a new file in the directory, `None` being the folder itself.
    NewFile(Option<Path>),
    /// Start naming a new directory in the directory, `None` being the folder itself.
    NewFolder(Option<Path>),
    /// Start renaming the file or directory.
    Rename(Path),
    Delete(Path),
    /// The name typed so far.
    Edited(String),
    Submit,
    Cancel,
}

pub enum Instruction {
    /// Open the file as the document.
    Open(Path),
    /// Make the file the main file.
    SetMain(Path),
    /// List the directory, which now shows as expanded.
    Expand(Path),
    /// Create an empty file or directory.
    Create { path: Path, kind: Kind },
    /// Move the file or directory `from` to `to`.
    Rename { from: Path, to: Path },
    /// Delete the file or directory, once the user confirms.
    Delete(Path),
}

impl State {
    /// An explorer whose folder is being listed.
    pub fn new() -> Self {
        Self {
            listings: BTreeMap::from([(None, None)]),
            editing: None,
            problem: None,
        }
    }

    pub fn update(&mut self, message: Message) -> Action<Instruction, Message> {
        match message {
            Message::Toggle(dir) => match self.listings.entry(Some(dir.clone())) {
                btree_map::Entry::Vacant(vacant) => {
                    vacant.insert(None);
                    Action::instruction(Instruction::Expand(dir))
                }
                btree_map::Entry::Occupied(_) => {
                    self.collapse(&dir);
                    Action::none()
                }
            },
            Message::Open(path) => Action::instruction(Instruction::Open(path)),
            Message::SetMain(path) => Action::instruction(Instruction::SetMain(path)),
            Message::NewFile(parent) => self.edit(Edit::New {
                parent,
                kind: Kind::File,
                name: UNTITLED.to_owned(),
            }),
            Message::NewFolder(parent) => self.edit(Edit::New {
                parent,
                kind: Kind::Dir,
                name: String::new(),
            }),
            Message::Rename(from) => {
                let name = from.name().to_owned();
                self.edit(Edit::Rename { from, name })
            }
            Message::Delete(path) => Action::instruction(Instruction::Delete(path)),
            Message::Edited(typed) => {
                if let Some(Edit::New { name, .. } | Edit::Rename { name, .. }) = &mut self.editing
                {
                    *name = typed;
                }
                self.problem = None;
                Action::none()
            }
            Message::Submit => {
                let Some(edit) = &self.editing else {
                    return Action::none();
                };
                let (parent, name) = match edit {
                    Edit::New { parent, name, .. } => (parent.clone(), name),
                    Edit::Rename { from, name } => (from.parent(), name),
                };
                let path = match named(parent.as_ref(), name) {
                    Ok(path) => path,
                    Err(problem) => {
                        self.problem = Some(problem);
                        return Action::none();
                    }
                };
                let instruction = match self.editing.take() {
                    Some(Edit::New { kind, .. }) => Some(Instruction::Create { path, kind }),
                    // The same name moves nothing.
                    Some(Edit::Rename { from, .. }) if from == path => None,
                    Some(Edit::Rename { from, .. }) => Some(Instruction::Rename { from, to: path }),
                    None => None,
                };
                Action {
                    instruction,
                    task: Task::none(),
                }
            }
            Message::Cancel => {
                self.editing = None;
                self.problem = None;
                Action::none()
            }
        }
    }

    /// Takes in the listing of `dir`, `None` being the folder, unless it was collapsed since.
    pub fn listed(&mut self, dir: Option<Path>, entries: Vec<Entry>) {
        if let Some(listing) = self.listings.get_mut(&dir) {
            *listing = Some(entries);
        }
    }

    /// Shows the new entry `path` of the given `kind` when its directory is expanded.
    pub fn created(&mut self, path: Path, kind: Kind) {
        if let Some(Some(entries)) = self.listings.get_mut(&path.parent()) {
            insert(entries, Entry { kind, path });
        }
    }

    /// Brings the expanded listings in line with `change`. A new file appears in the deepest
    /// listed directory holding it, as itself or as the directory leading to it.
    pub fn changed(&mut self, change: &Change) {
        match change {
            Change::Changed { path, .. } => {
                let mut child = path.clone();
                loop {
                    let parent = child.parent();
                    match self.listings.get_mut(&parent) {
                        Some(Some(entries)) => {
                            let kind = if child == *path {
                                Kind::File
                            } else {
                                Kind::Dir
                            };
                            insert(entries, Entry { kind, path: child });
                            return;
                        }
                        // The listing on its way shows the file already.
                        Some(None) => return,
                        None => match parent {
                            Some(parent) => child = parent,
                            None => return,
                        },
                    }
                }
            }
            Change::Removed(path) => {
                if let Some(Some(entries)) = self.listings.get_mut(&path.parent()) {
                    entries.retain(|entry| entry.path != *path);
                }
                self.collapse(path);
            }
        }
    }

    /// The listing of `dir`, `None` being the folder itself, when it is expanded and listed.
    #[cfg(test)]
    pub fn listing(&self, dir: Option<&Path>) -> Option<&[Entry]> {
        self.listings.get(&dir.cloned())?.as_deref()
    }

    /// The sidebar, highlighting `document`, the open file, and marking `main`, the main file.
    pub fn view(&self, document: Option<&Path>, main: Option<&Path>) -> Element<'_, Message> {
        let header = row![
            text("Files").size(14).width(Fill),
            action("+ File", Message::NewFile(None)),
            action("+ Folder", Message::NewFolder(None)),
        ]
        .spacing(5)
        .align_y(Center);
        let mut rows = vec![header.into()];
        if let Some(Edit::New { parent: None, .. }) = &self.editing {
            rows.push(self.input(0));
        }
        self.rows(None, 0, document, main, &mut rows);

        container(scrollable(column(rows).spacing(2)).height(Fill))
            .width(WIDTH)
            .height(Fill)
            .padding(10)
            .into()
    }

    /// Starts typing the name `edit` holds, in a focused input with the name's stem selected.
    fn edit(&mut self, edit: Edit) -> Action<Instruction, Message> {
        let name = match &edit {
            Edit::New { name, .. } | Edit::Rename { name, .. } => name,
        };
        let stem = match name.rsplit_once('.') {
            Some((stem, _)) if !stem.is_empty() => stem.len(),
            Some(_) | None => name.len(),
        };
        self.editing = Some(edit);
        self.problem = None;

        let id = widget::Id::new(INPUT);
        let start = Position { line: 0, index: 0 };
        let end = Position {
            line: 0,
            index: stem,
        };
        Action::task(operation::focus(id.clone()).chain(operation::select_range(id, start, end)))
    }

    /// Drops the listings of `dir` and of every directory under it.
    fn collapse(&mut self, dir: &Path) {
        self.listings.retain(|listed, _| {
            listed
                .as_ref()
                .is_none_or(|listed| listed != dir && listed.strip_prefix(dir).is_none())
        });
    }

    /// Appends to `rows` a row for each entry of `dir`, indented `depth` levels, followed by the
    /// rows of each expanded directory among them.
    fn rows<'a>(
        &'a self,
        dir: Option<&Path>,
        depth: u16,
        document: Option<&Path>,
        main: Option<&Path>,
        rows: &mut Vec<Element<'a, Message>>,
    ) {
        let Some(Some(entries)) = self.listings.get(&dir.cloned()) else {
            return;
        };
        let indent = padding::left(f32::from(depth) * INDENT);

        for entry in entries {
            if matches!(&self.editing, Some(Edit::Rename { from, .. }) if *from == entry.path) {
                rows.push(self.input(depth));
                continue;
            }
            let name = entry.path.name();
            // bulin writes these files itself, so they are left as they are.
            let generated = is_generated(&entry.path);
            let changes = (!generated).then(|| {
                row![
                    action("Rename", Message::Rename(entry.path.clone())),
                    action("Delete", Message::Delete(entry.path.clone())),
                ]
                .spacing(2)
            });
            match entry.kind {
                Kind::Dir => {
                    let expanded = self.listings.contains_key(&Some(entry.path.clone()));
                    let marker = if expanded { '-' } else { '+' };
                    let toggle = button(text!("{marker} {name}/").size(14))
                        .style(button::text)
                        .width(Fill)
                        .on_press(Message::Toggle(entry.path.clone()));
                    let create = (!generated).then(|| {
                        row![
                            action("+ File", Message::NewFile(Some(entry.path.clone()))),
                            action("+ Folder", Message::NewFolder(Some(entry.path.clone()))),
                        ]
                        .spacing(2)
                    });
                    rows.push(
                        row![toggle]
                            .push(create)
                            .push(changes)
                            .spacing(2)
                            .align_y(Center)
                            .padding(indent)
                            .into(),
                    );
                    if matches!(&self.editing, Some(Edit::New { parent: Some(parent), .. }) if *parent == entry.path)
                    {
                        rows.push(self.input(depth + 1));
                    }
                    if expanded {
                        self.rows(Some(&entry.path), depth + 1, document, main, rows);
                    }
                }
                Kind::File => {
                    let open = document == Some(&entry.path);
                    let file = button(text(name).size(14))
                        .style(move |theme, status| {
                            if open {
                                button::primary(theme, status)
                            } else {
                                button::text(theme, status)
                            }
                        })
                        .width(Fill)
                        .on_press(Message::Open(entry.path.clone()));
                    let role: Element<'a, Message> = if main == Some(&entry.path) {
                        text("main").size(12).into()
                    } else {
                        action("Set as main", Message::SetMain(entry.path.clone()))
                    };
                    rows.push(
                        row![file, role]
                            .push(changes)
                            .spacing(5)
                            .align_y(Center)
                            .padding(indent)
                            .into(),
                    );
                }
            }
        }
    }

    /// The input for the name being typed, indented `depth` levels, with why the last one
    /// submitted can't be used.
    fn input(&self, depth: u16) -> Element<'_, Message> {
        let name = match &self.editing {
            Some(Edit::New { name, .. } | Edit::Rename { name, .. }) => name.as_str(),
            None => "",
        };
        let field = row![
            text_input("Name", name)
                .id(widget::Id::new(INPUT))
                .on_input(Message::Edited)
                .on_submit(Message::Submit)
                .size(14)
                .padding(2),
            action("Cancel", Message::Cancel),
        ]
        .spacing(5)
        .align_y(Center);

        column![field]
            .push(
                self.problem
                    .as_deref()
                    .map(|problem| text(problem).size(12).style(text::danger)),
            )
            .spacing(2)
            .padding(padding::left(f32::from(depth) * INDENT))
            .into()
    }
}

/// A small text button sending `message`.
fn action<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    button(text(label).size(12))
        .style(button::text)
        .padding([2, 4])
        .on_press(message)
        .into()
}

/// The entry `name` of the directory `parent`, `None` being the folder itself, or why `name` can't
/// be one.
fn named(parent: Option<&Path>, name: &str) -> Result<Path, String> {
    if name.is_empty() {
        return Err("A name can't be empty.".to_owned());
    }
    if name.contains('/') {
        return Err(format!("`{name}` contains `/`, but a name is one entry."));
    }
    Path::in_dir(parent, name).map_err(|error| format!("That name doesn't work: {error}."))
}

/// Puts `entry` into the sorted listing `entries`, unless it lists the path already.
fn insert(entries: &mut Vec<Entry>, entry: Entry) {
    if !entries.iter().any(|listed| listed.path == entry.path) {
        let at = entries.partition_point(|listed| *listed < entry);
        entries.insert(at, entry);
    }
}

/// Whether `path` is bulin's `.bulin/` directory or lies in it.
fn is_generated(path: &Path) -> bool {
    path.as_str().split('/').next() == Some(".bulin")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> Path {
        Path::new(text).unwrap()
    }

    fn entry(kind: Kind, file: &str) -> Entry {
        Entry {
            kind,
            path: path(file),
        }
    }

    #[test]
    fn a_listing_arriving_after_collapse_is_dropped() {
        let mut state = State::new();
        state.listed(None, vec![entry(Kind::Dir, "sub")]);

        assert!(matches!(
            state.update(Message::Toggle(path("sub"))).instruction,
            Some(Instruction::Expand(dir)) if dir == path("sub")
        ));
        assert!(
            state
                .update(Message::Toggle(path("sub")))
                .instruction
                .is_none()
        );
        state.listed(Some(path("sub")), vec![entry(Kind::File, "sub/a.wesl")]);

        assert!(!state.listings.contains_key(&Some(path("sub"))));
        assert!(matches!(
            state.update(Message::Toggle(path("sub"))).instruction,
            Some(Instruction::Expand(_))
        ));
        state.listed(Some(path("sub")), vec![entry(Kind::File, "sub/a.wesl")]);
        assert_eq!(
            state.listing(Some(&path("sub"))),
            Some(&[entry(Kind::File, "sub/a.wesl")][..])
        );
    }

    #[test]
    fn an_invalid_name_keeps_the_input_open() {
        let mut state = State::new();
        let _ = state.update(Message::NewFile(Some(path("dir"))));
        assert!(matches!(&state.editing, Some(Edit::New { name, .. }) if name == UNTITLED));

        for name in ["a/b", "..", ""] {
            let _ = state.update(Message::Edited(name.to_owned()));
            assert!(
                state.update(Message::Submit).instruction.is_none(),
                "{name}"
            );
            assert!(state.editing.is_some(), "{name}");
            assert!(state.problem.is_some(), "{name}");
        }

        let _ = state.update(Message::Edited("x.wesl".to_owned()));
        assert!(state.problem.is_none());
        assert!(matches!(
            state.update(Message::Submit).instruction,
            Some(Instruction::Create { path: created, kind: Kind::File }) if created == path("dir/x.wesl")
        ));
        assert!(state.editing.is_none());
    }

    #[test]
    fn renaming_keeps_the_directory() {
        let mut state = State::new();
        let _ = state.update(Message::Rename(path("dir/a.wesl")));
        assert!(
            state.update(Message::Submit).instruction.is_none(),
            "the same name"
        );
        assert!(state.editing.is_none());

        let _ = state.update(Message::Rename(path("dir/a.wesl")));
        let _ = state.update(Message::Edited("b.wesl".to_owned()));
        assert!(matches!(
            state.update(Message::Submit).instruction,
            Some(Instruction::Rename { from, to }) if from == path("dir/a.wesl") && to == path("dir/b.wesl")
        ));
    }

    #[test]
    fn changes_update_expanded_listings() {
        let mut state = State::new();
        state.listed(None, vec![entry(Kind::File, "a.wesl")]);

        state.changed(&Change::Changed {
            path: path("x/y.wesl"),
            text: Some("y".to_owned()),
        });
        assert_eq!(
            state.listing(None),
            Some(&[entry(Kind::Dir, "x"), entry(Kind::File, "a.wesl")][..])
        );

        let _ = state.update(Message::Toggle(path("x")));
        state.listed(Some(path("x")), vec![entry(Kind::File, "x/y.wesl")]);
        state.changed(&Change::Changed {
            path: path("x/z.md"),
            text: None,
        });
        assert_eq!(
            state.listing(Some(&path("x"))),
            Some(&[entry(Kind::File, "x/y.wesl"), entry(Kind::File, "x/z.md")][..])
        );

        state.changed(&Change::Removed(path("x")));
        assert_eq!(
            state.listing(None),
            Some(&[entry(Kind::File, "a.wesl")][..])
        );
        assert!(!state.listings.contains_key(&Some(path("x"))));
    }
}
