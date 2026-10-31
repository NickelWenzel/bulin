//! The sidebar listing an open folder: directories expand when clicked and are listed only then,
//! and each file opens as the document or becomes the main file.

use std::collections::{BTreeMap, btree_map};

use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Center, Element, Fill, padding};

use crate::core::folder::{Change, Entry, Kind, Path};

/// How wide the sidebar is.
const WIDTH: f32 = 260.0;

/// How far each level of directories is indented.
const INDENT: f32 = 12.0;

pub struct State {
    /// The listing of each expanded directory, keyed by the directory with `None` for the folder
    /// itself, and `None` while it is being listed. A directory without a key is collapsed.
    listings: BTreeMap<Option<Path>, Option<Vec<Entry>>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Expand or collapse a directory.
    Toggle(Path),
    Open(Path),
    SetMain(Path),
}

pub enum Instruction {
    /// Open the file as the document.
    Open(Path),
    /// Make the file the main file.
    SetMain(Path),
    /// List the directory, which now shows as expanded.
    Expand(Path),
}

impl State {
    /// An explorer whose folder is being listed.
    pub fn new() -> Self {
        Self {
            listings: BTreeMap::from([(None, None)]),
        }
    }

    pub fn update(&mut self, message: Message) -> Option<Instruction> {
        match message {
            Message::Toggle(dir) => match self.listings.entry(Some(dir.clone())) {
                btree_map::Entry::Vacant(vacant) => {
                    vacant.insert(None);
                    Some(Instruction::Expand(dir))
                }
                btree_map::Entry::Occupied(_) => {
                    self.collapse(&dir);
                    None
                }
            },
            Message::Open(path) => Some(Instruction::Open(path)),
            Message::SetMain(path) => Some(Instruction::SetMain(path)),
        }
    }

    /// Takes in the listing of `dir`, `None` being the folder, unless it was collapsed since.
    pub fn listed(&mut self, dir: Option<Path>, entries: Vec<Entry>) {
        if let Some(listing) = self.listings.get_mut(&dir) {
            *listing = Some(entries);
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
                            let entry = Entry { kind, path: child };
                            if !entries.iter().any(|listed| listed.path == entry.path) {
                                let at = entries.partition_point(|listed| *listed < entry);
                                entries.insert(at, entry);
                            }
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

    /// The sidebar, highlighting `document`, the open file, and marking `main`, the main file.
    pub fn view(&self, document: Option<&Path>, main: Option<&Path>) -> Element<'_, Message> {
        let mut rows = Vec::new();
        self.rows(None, 0, document, main, &mut rows);

        container(scrollable(column(rows).spacing(2)).height(Fill))
            .width(WIDTH)
            .height(Fill)
            .padding(10)
            .into()
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
            let name = entry.path.name();
            match entry.kind {
                Kind::Dir => {
                    let expanded = self.listings.contains_key(&Some(entry.path.clone()));
                    let marker = if expanded { '-' } else { '+' };
                    rows.push(
                        container(
                            button(text!("{marker} {name}/").size(14))
                                .style(button::text)
                                .width(Fill)
                                .on_press(Message::Toggle(entry.path.clone())),
                        )
                        .padding(indent)
                        .into(),
                    );
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
                        button(text("Set as main").size(12))
                            .style(button::text)
                            .on_press(Message::SetMain(entry.path.clone()))
                            .into()
                    };
                    rows.push(
                        row![file, role]
                            .spacing(5)
                            .align_y(Center)
                            .padding(indent)
                            .into(),
                    );
                }
            }
        }
    }
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

    fn listing<'a>(state: &'a State, dir: Option<&str>) -> Option<&'a Vec<Entry>> {
        state.listings.get(&dir.map(path))?.as_ref()
    }

    #[test]
    fn a_listing_arriving_after_collapse_is_dropped() {
        let mut state = State::new();
        state.listed(None, vec![entry(Kind::Dir, "sub")]);

        assert!(matches!(
            state.update(Message::Toggle(path("sub"))),
            Some(Instruction::Expand(dir)) if dir == path("sub")
        ));
        assert!(state.update(Message::Toggle(path("sub"))).is_none());
        state.listed(Some(path("sub")), vec![entry(Kind::File, "sub/a.wesl")]);

        assert!(!state.listings.contains_key(&Some(path("sub"))));
        assert!(matches!(
            state.update(Message::Toggle(path("sub"))),
            Some(Instruction::Expand(_))
        ));
        state.listed(Some(path("sub")), vec![entry(Kind::File, "sub/a.wesl")]);
        assert_eq!(
            listing(&state, Some("sub")),
            Some(&vec![entry(Kind::File, "sub/a.wesl")])
        );
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
            listing(&state, None),
            Some(&vec![entry(Kind::Dir, "x"), entry(Kind::File, "a.wesl")])
        );

        let _ = state.update(Message::Toggle(path("x")));
        state.listed(Some(path("x")), vec![entry(Kind::File, "x/y.wesl")]);
        state.changed(&Change::Changed {
            path: path("x/z.md"),
            text: None,
        });
        assert_eq!(
            listing(&state, Some("x")),
            Some(&vec![
                entry(Kind::File, "x/y.wesl"),
                entry(Kind::File, "x/z.md")
            ])
        );

        state.changed(&Change::Removed(path("x")));
        assert_eq!(
            listing(&state, None),
            Some(&vec![entry(Kind::File, "a.wesl")])
        );
        assert!(!state.listings.contains_key(&Some(path("x"))));
    }
}
