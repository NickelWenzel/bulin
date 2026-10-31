//! A folder in the browser's library: an entry of IndexedDB's `entries` store, whose files are
//! records of the `files` store keyed by the entry's id and the file's path, and the modal that
//! lists the library's folders and projects and imports and exports them.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use iced::futures::{Stream, StreamExt, stream};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Center, Element, Fill, Task};
use idb::{
    Database, DatabaseEvent, Factory, KeyPath, KeyRange, ObjectStore, ObjectStoreParams, Query,
    TransactionMode, TransactionResult,
};
use js_sys::{Array, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{DomException, HtmlInputElement};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::opener::Instruction;
use super::{Error, Watched};
use crate::action::Action;
use crate::core::archive::{self, Budget, MAX_COMPRESSED, MAX_ENTRIES};
use crate::core::folder::{
    self, Change, Entry, Expect, MAX_TEXT, Path, SETTINGS, Settings, reads_text,
};
use crate::core::library::{self, Id, Upper};
use crate::core::program::DEFAULT_SOURCE;
use crate::core::{Project, Uniforms};
use crate::file;

const DATABASE: &str = "bulin";
const VERSION: u32 = 1;
/// The store of the library's entries: `{id, name, kind, modified}`, numbered by IndexedDB.
const ENTRIES: &str = "entries";
/// The store of the entries' files: their texts, keyed by `[id, path]`.
const FILES: &str = "files";
/// The file a new folder starts with, as its main file.
const MAIN: &str = "main.wesl";
/// The file holding a project's JSON.
const PROJECT: &str = "project.bulin";

thread_local! {
    /// Whether the browser was asked to keep the library.
    static ASKED: Cell<bool> = const { Cell::new(false) };
    /// Whether the browser refused to keep the library, until the library modal says so.
    static REFUSED: Cell<bool> = const { Cell::new(false) };
}

/// A folder, by its library entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Handle {
    id: Id,
    name: String,
}

/// The library modal, listing the folders or the projects to open, create, rename and delete.
#[derive(Debug, Default)]
pub struct Opener {
    /// The kind of entries the modal lists, while it shows.
    shown: Option<Kind>,
    entries: Vec<Item>,
    /// The name typed for a new entry.
    new_name: String,
    /// The entry being renamed or asked about deleting.
    edit: Option<Edit>,
    /// Whether the browser refused to keep the library, which the modal says this once.
    refused: bool,
    /// A folder just imported and what its import left out, shown until the user opens it.
    imported: Option<(Item, Vec<String>)>,
    /// Whether an entry is being created, read, renamed, deleted, imported or exported, or a
    /// project uploaded.
    busy: bool,
}

/// What a library entry holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Project,
}

/// A library entry, as the modal lists it.
#[derive(Debug, Clone)]
pub struct Item {
    id: Id,
    name: String,
    kind: Kind,
}

/// A change to one entry the user is making.
#[derive(Debug)]
enum Edit {
    Rename { id: Id, name: String },
    Delete(Id),
}

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Vec<Item>, Error>),
    Open(Item),
    ProjectRead(Result<(file::Name, Arc<String>), Error>),
    NameChanged(String),
    Create,
    Created(Kind, Result<Item, Error>),
    Rename(Item),
    Delete(Item),
    EditChanged(String),
    Confirm,
    Cancel,
    Renamed(Result<(), Error>),
    Deleted(Result<(), Error>),
    Upload,
    Uploaded(Result<(file::Name, Arc<String>), file::Error>),
    ImportZip,
    ImportDirectory,
    /// The folder an import stored, with what it left out, or `None` when the user cancelled.
    FolderImported(Result<Option<(Item, Vec<String>)>, Error>),
    ImportProject,
    /// The project an import stored, with its text, or `None` when the user cancelled.
    ProjectImported(Result<Option<(Item, Arc<String>)>, Error>),
    Export(Item),
    Exported(Result<(), Error>),
}

/// A connection to the library, closed once its transactions finish when it is dropped.
struct Connection(Database);

impl Drop for Connection {
    fn drop(&mut self) {
        self.0.close();
    }
}

impl Handle {
    /// The entry's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The library entry holding the folder.
    pub fn library_id(&self) -> Option<Id> {
        Some(self.id)
    }
}

impl Kind {
    /// The entry's `kind` field.
    fn as_str(self) -> &'static str {
        match self {
            Kind::Folder => "folder",
            Kind::Project => "project",
        }
    }
}

impl Opener {
    /// Shows the library's folders.
    pub fn open_folder(&mut self) -> Task<Message> {
        self.shown = Some(Kind::Folder);
        self.edit = None;
        self.imported = None;
        self.refused = REFUSED.replace(false);
        Task::perform(list(), Message::Listed)
    }

    /// Shows the library's projects and the upload of a project file.
    pub fn open_project(&mut self) -> Task<Message> {
        self.shown = Some(Kind::Project);
        self.edit = None;
        self.imported = None;
        self.refused = REFUSED.replace(false);
        Task::perform(list(), Message::Listed)
    }

    pub fn update(&mut self, message: Message) -> Action<Instruction, Message> {
        match message {
            Message::Listed(Ok(entries)) => {
                self.entries = entries;
                Action::none()
            }
            Message::Listed(Err(error)) => {
                self.shown = None;
                Action::instruction(Instruction::Failed(format!(
                    "Could not read the library: {error}"
                )))
            }
            Message::Open(Item {
                id,
                name,
                kind: Kind::Folder,
            }) => {
                self.shown = None;
                self.imported = None;
                Action::instruction(Instruction::OpenFolder(Handle { id, name }))
            }
            Message::Open(Item {
                id,
                name,
                kind: Kind::Project,
            }) => {
                if self.busy {
                    return Action::none();
                }
                self.shown = None;
                self.busy = true;
                let path = Path::new(PROJECT).expect("invariant: the project's path is valid");
                let handle = Handle {
                    id,
                    name: name.clone(),
                };
                let name = file::Name::Library { id, name };
                Action::task(Task::perform(
                    async move {
                        let text = read(handle, path).await?;
                        Ok((name, Arc::new(text)))
                    },
                    Message::ProjectRead,
                ))
            }
            Message::ProjectRead(result) => {
                self.busy = false;
                Action::instruction(match result {
                    Ok((name, text)) => Instruction::OpenProject(name, text),
                    Err(error) => {
                        Instruction::Failed(format!("Could not open the project: {error}"))
                    }
                })
            }
            Message::NameChanged(name) => {
                self.new_name = name;
                Action::none()
            }
            Message::Create => {
                let name = self.new_name.trim().to_owned();
                let Some(kind) = self.shown else {
                    return Action::none();
                };
                if name.is_empty() || self.busy {
                    return Action::none();
                }
                self.busy = true;
                let task = match kind {
                    Kind::Folder => {
                        Task::perform(create(name), move |result| Message::Created(kind, result))
                    }
                    Kind::Project => Task::perform(
                        async move {
                            let json = Project::untitled().to_json();
                            let id = store_project(None, name.clone(), json).await?;
                            Ok(Item { id, name, kind })
                        },
                        move |result| Message::Created(kind, result),
                    ),
                };
                Action::task(task)
            }
            Message::Created(_, Ok(Item { id, name, kind })) => {
                self.busy = false;
                self.shown = None;
                self.new_name.clear();
                Action::instruction(match kind {
                    Kind::Folder => Instruction::OpenFolder(Handle { id, name }),
                    // A new project holds what `Project::untitled` makes, as stored.
                    Kind::Project => Instruction::OpenProject(
                        file::Name::Library { id, name },
                        Arc::new(Project::untitled().to_json()),
                    ),
                })
            }
            Message::Created(kind, Err(error)) => {
                self.busy = false;
                Action::instruction(Instruction::Failed(format!(
                    "Could not create the {}: {error}",
                    kind.as_str()
                )))
            }
            Message::Rename(item) => {
                self.edit = Some(Edit::Rename {
                    id: item.id,
                    name: item.name,
                });
                Action::none()
            }
            Message::Delete(item) => {
                self.edit = Some(Edit::Delete(item.id));
                Action::none()
            }
            Message::EditChanged(changed) => {
                if let Some(Edit::Rename { name, .. }) = &mut self.edit {
                    *name = changed;
                }
                Action::none()
            }
            Message::Confirm => {
                if self.busy {
                    return Action::none();
                }
                let task = match &self.edit {
                    Some(Edit::Rename { id, name }) => {
                        let name = name.trim().to_owned();
                        if name.is_empty() {
                            return Action::none();
                        }
                        Task::perform(rename_entry(*id, name), Message::Renamed)
                    }
                    Some(Edit::Delete(id)) => Task::perform(delete_entry(*id), Message::Deleted),
                    None => return Action::none(),
                };
                self.busy = true;
                Action::task(task)
            }
            Message::Cancel => {
                self.edit = None;
                Action::none()
            }
            Message::Renamed(Ok(())) | Message::Deleted(Ok(())) => {
                self.busy = false;
                self.edit = None;
                Action::task(Task::perform(list(), Message::Listed))
            }
            Message::Renamed(Err(error)) => {
                self.busy = false;
                Action::instruction(Instruction::Failed(format!(
                    "Could not rename the entry: {error}"
                )))
            }
            Message::Deleted(Err(error)) => {
                self.busy = false;
                Action::instruction(Instruction::Failed(format!(
                    "Could not delete the entry: {error}"
                )))
            }
            Message::Upload => {
                self.shown = None;
                self.busy = true;
                Action::task(Task::perform(file::open_file(), Message::Uploaded))
            }
            Message::Uploaded(result) => {
                self.busy = false;
                match result {
                    Ok((name, text)) => Action::instruction(Instruction::OpenProject(name, text)),
                    Err(file::Error::DialogClosed) => Action::none(),
                    Err(error) => Action::instruction(Instruction::Failed(format!(
                        "Could not open the project: {error}"
                    ))),
                }
            }
            Message::ImportZip | Message::ImportDirectory => {
                if self.busy {
                    return Action::none();
                }
                self.busy = true;
                let task = if matches!(message, Message::ImportZip) {
                    Task::perform(import_zip(), Message::FolderImported)
                } else {
                    Task::perform(import_directory(), Message::FolderImported)
                };
                Action::task(task)
            }
            Message::FolderImported(result) => {
                self.busy = false;
                match result {
                    Ok(None) => Action::none(),
                    // What was left out shows before the folder opens, unless the modal is gone.
                    Ok(Some((item, notices))) if !notices.is_empty() && self.shown.is_some() => {
                        self.imported = Some((item, notices));
                        Action::task(Task::perform(list(), Message::Listed))
                    }
                    Ok(Some((Item { id, name, .. }, _))) => {
                        self.shown = None;
                        Action::instruction(Instruction::OpenFolder(Handle { id, name }))
                    }
                    Err(error) => {
                        self.shown = None;
                        Action::instruction(Instruction::Failed(format!(
                            "Could not import the folder: {error}"
                        )))
                    }
                }
            }
            Message::ImportProject => {
                if self.busy {
                    return Action::none();
                }
                self.busy = true;
                Action::task(Task::perform(import_project(), Message::ProjectImported))
            }
            Message::ProjectImported(result) => {
                self.busy = false;
                match result {
                    Ok(None) => Action::none(),
                    Ok(Some((Item { id, name, .. }, text))) => {
                        self.shown = None;
                        Action::instruction(Instruction::OpenProject(
                            file::Name::Library { id, name },
                            text,
                        ))
                    }
                    Err(error) => {
                        self.shown = None;
                        Action::instruction(Instruction::Failed(format!(
                            "Could not import the project: {error}"
                        )))
                    }
                }
            }
            Message::Export(Item { id, name, kind }) => {
                if self.busy {
                    return Action::none();
                }
                self.busy = true;
                let task = match kind {
                    Kind::Folder => Task::perform(export_folder(id, name), Message::Exported),
                    Kind::Project => Task::perform(export_project(id, name), Message::Exported),
                };
                Action::task(task)
            }
            Message::Exported(result) => {
                self.busy = false;
                match result {
                    Ok(()) => Action::none(),
                    Err(error) => {
                        self.shown = None;
                        Action::instruction(Instruction::Failed(format!(
                            "Could not export the entry: {error}"
                        )))
                    }
                }
            }
        }
    }

    /// The modal, while it shows. The entry `current` is open, so it can't be renamed or deleted.
    pub fn view(&self, current: Option<Id>) -> Option<Element<'_, Message>> {
        let kind = self.shown?;
        let (title, empty, new) = match kind {
            Kind::Folder => ("Folders", "No folders yet.", "New Folder"),
            Kind::Project => ("Projects", "No projects yet.", "New Project"),
        };

        let items = self
            .entries
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| {
                let entry = match &self.edit {
                    Some(Edit::Rename { id, name }) if *id == item.id => {
                        let can_rename = !self.busy && !name.trim().is_empty();
                        row![
                            text_input("Name", name)
                                .on_input(Message::EditChanged)
                                .on_submit(Message::Confirm),
                            button("Rename").on_press_maybe(can_rename.then_some(Message::Confirm)),
                            button("Cancel")
                                .style(button::secondary)
                                .on_press(Message::Cancel),
                        ]
                    }
                    Some(Edit::Delete(id)) if *id == item.id => row![
                        text(format!("Delete {}?", item.name)).width(Fill),
                        button("Delete")
                            .style(button::danger)
                            .on_press_maybe((!self.busy).then_some(Message::Confirm)),
                        button("Cancel")
                            .style(button::secondary)
                            .on_press(Message::Cancel),
                    ],
                    Some(Edit::Rename { .. } | Edit::Delete(_)) | None => {
                        let is_open = current == Some(item.id);
                        let can_change = !self.busy && !is_open;
                        row![
                            text(&item.name).width(Fill),
                            is_open.then(|| text("open").size(14)),
                            button("Open").on_press(Message::Open(item.clone())),
                            button("Rename")
                                .style(button::secondary)
                                .on_press_maybe(can_change.then(|| Message::Rename(item.clone()))),
                            button("Delete")
                                .style(button::secondary)
                                .on_press_maybe(can_change.then(|| Message::Delete(item.clone()))),
                        ]
                    }
                };
                entry.spacing(10).align_y(Center).into()
            })
            .collect::<Vec<Element<'_, Message>>>();
        let entries: Element<'_, Message> = if items.is_empty() {
            text(empty).into()
        } else {
            scrollable(column(items).spacing(5)).into()
        };

        let can_create = !self.busy && !self.new_name.trim().is_empty();
        let create = row![
            text_input("Name", &self.new_name)
                .on_input(Message::NameChanged)
                .on_submit(Message::Create),
            button(new).on_press_maybe(can_create.then_some(Message::Create)),
        ]
        .spacing(10);

        let imported = self.imported.as_ref().map(|(item, notices)| {
            column![
                text(format!("Imported {}, leaving out:", item.name)),
                scrollable(
                    column(notices.iter().map(|notice| text(notice).size(14).into())).spacing(5)
                )
                .height(iced::Shrink.max(150)),
                button("Open").on_press(Message::Open(item.clone())),
            ]
            .spacing(10)
        });

        // Export takes the open entry as stored, without what the editor holds unsaved.
        let idle = !self.busy;
        let export = current
            .and_then(|id| {
                self.entries
                    .iter()
                    .find(|item| item.id == id && item.kind == kind)
            })
            .map(|item| {
                button("Export as saved")
                    .style(button::secondary)
                    .on_press_maybe(idle.then(|| Message::Export(item.clone())))
            });
        let transfer = match kind {
            Kind::Folder => row![
                button("Import a zip").on_press_maybe(idle.then_some(Message::ImportZip)),
                button("Import a directory")
                    .on_press_maybe(idle.then_some(Message::ImportDirectory)),
                export,
            ],
            Kind::Project => row![
                button("Import a .bulin file")
                    .on_press_maybe(idle.then_some(Message::ImportProject)),
                button("Upload a .bulin file")
                    .style(button::secondary)
                    .on_press(Message::Upload),
                export,
            ],
        }
        .spacing(10);

        let content = column![
            self.refused.then(|| text(
                "The browser may clear stored folders and projects when space runs low. \
                 Export them to keep a copy."
            )),
            imported,
            text(title).size(20),
            entries,
            create,
            transfer,
        ]
        .spacing(15);

        Some(
            container(content)
                .width(560)
                .height(iced::Shrink.max(500))
                .padding(20)
                .style(container::rounded_box)
                .into(),
        )
    }

    /// Hides the modal.
    pub fn close(&mut self) {
        self.shown = None;
        self.edit = None;
        self.imported = None;
    }

    /// Whether the modal shows, or an entry is being created, read, imported or exported, or a
    /// project uploaded.
    pub fn is_busy(&self) -> bool {
        self.shown.is_some() || self.busy
    }
}

/// The text of the file `path` in the folder at `handle`.
pub async fn read(handle: Handle, path: Path) -> Result<String, Error> {
    let database = open().await?;
    let transaction = database
        .0
        .transaction(&[FILES], TransactionMode::ReadOnly)
        .map_err(error)?;
    let value = transaction
        .object_store(FILES)
        .map_err(error)?
        .get(file(handle.id, &path))
        .map_err(error)?
        .await
        .map_err(error)?;
    let text = value
        .and_then(|value| value.as_string())
        .ok_or(Error::Missing)?;
    if text.len() > MAX_TEXT {
        return Err(Error::Io(format!("{path} is larger than 1 MiB")));
    }
    Ok(text)
}

/// The entries of the directory `dir` in the folder at `handle`, `None` being the folder itself,
/// sorted with directories first.
pub async fn scan_dir(handle: Handle, dir: Option<Path>) -> Result<Vec<Entry>, Error> {
    let range = range(handle.id, dir.as_ref())?;
    let database = open().await?;
    let transaction = database
        .0
        .transaction(&[FILES], TransactionMode::ReadOnly)
        .map_err(error)?;
    let keys = transaction
        .object_store(FILES)
        .map_err(error)?
        .get_all_keys(Some(Query::KeyRange(range)), None)
        .map_err(error)?
        .await
        .map_err(error)?;

    let paths = keys.into_iter().filter_map(path_of).collect::<Vec<_>>();
    Ok(library::collapse(
        dir.as_ref(),
        paths.iter().map(String::as_str),
    ))
}

/// Writes `text` to the file `path` in the folder at `handle`, if the file is what `expect` says.
pub async fn write(
    handle: Handle,
    path: Path,
    text: String,
    expect: Expect,
) -> Result<Vec<Change>, Error> {
    let key = file(handle.id, &path);
    let value = JsValue::from_str(&text);
    let database = open().await?;

    // A transaction commits as soon as no request of its own is pending, so between opening it
    // and committing nothing awaits anything else: what it stores is computed beforehand.
    let transaction = database
        .0
        .transaction(&[FILES], TransactionMode::ReadWrite)
        .map_err(error)?;
    let files = transaction.object_store(FILES).map_err(error)?;
    let disk = files
        .get(key.clone())
        .map_err(error)?
        .await
        .map_err(error)?
        .and_then(|disk| disk.as_string());
    if !expect.matches(disk.as_deref()) {
        let _ = transaction.abort();
        return Err(Error::Conflict { disk });
    }
    files
        .put(&value, Some(&key))
        .map_err(error)?
        .await
        .map_err(error)?;
    committed(transaction.await)?;

    Ok(vec![Change::Changed {
        text: reads_text(&path).then_some(text),
        path,
    }])
}

/// Creates the directory `path` in the folder at `handle`, refusing one that exists. A marker key
/// keeps it while it holds no file.
pub async fn create_dir(handle: Handle, path: Path) -> Result<(), Error> {
    let file = file(handle.id, &path);
    let dir = range(handle.id, Some(&path))?;
    let marker = key(handle.id, &library::marker_key(&path));
    let database = open().await?;

    // As in `write`, the transaction awaits nothing but its own requests.
    let transaction = database
        .0
        .transaction(&[FILES], TransactionMode::ReadWrite)
        .map_err(error)?;
    let files = transaction.object_store(FILES).map_err(error)?;
    if holds(&files, file, dir).await? {
        let _ = transaction.abort();
        return Err(Error::Io(format!("{path} already exists")));
    }
    files
        .put(&JsValue::from_str(""), Some(&marker))
        .map_err(error)?
        .await
        .map_err(error)?;
    committed(transaction.await)
}

/// Moves the file or directory `from` in the folder at `handle` to `to`, refusing a `to` that
/// exists. Reports `from` removed and each file now under `to` changed, with its text as the scan
/// reads it.
///
/// Every key under `from` moves in one transaction, so the folder never holds half a rename.
pub async fn rename(handle: Handle, from: Path, to: Path) -> Result<Vec<Change>, Error> {
    if to == from || to.strip_prefix(&from).is_some() {
        return Err(Error::Io(format!("{from} can't move into itself")));
    }
    let source = file(handle.id, &from);
    let sources = range(handle.id, Some(&from))?;
    let target = file(handle.id, &to);
    let targets = range(handle.id, Some(&to))?;
    let database = open().await?;

    // As in `write`, the transaction awaits nothing but its own requests: the new keys are
    // computed from what it reads without awaiting anything else.
    let transaction = database
        .0
        .transaction(&[FILES], TransactionMode::ReadWrite)
        .map_err(error)?;
    let files = transaction.object_store(FILES).map_err(error)?;
    if holds(&files, target, targets).await? {
        let _ = transaction.abort();
        return Err(Error::Io(format!("{to} already exists")));
    }
    let text = files
        .get(source.clone())
        .map_err(error)?
        .await
        .map_err(error)?;
    let keys = files
        .get_all_keys(Some(Query::KeyRange(sources.clone())), None)
        .map_err(error)?
        .await
        .map_err(error)?;
    let values = files
        .get_all(Some(Query::KeyRange(sources.clone())), None)
        .map_err(error)?
        .await
        .map_err(error)?;

    // Each moved key's new path and value; both lists follow the keys' order.
    let moved = text
        .map(|text| (to.as_str().to_owned(), text))
        .into_iter()
        .chain(keys.into_iter().zip(values).filter_map(|(key, value)| {
            let rest = path_of(key)?.strip_prefix(from.as_str())?.to_owned();
            Some((format!("{to}{rest}"), value))
        }))
        .collect::<Vec<_>>();
    if moved.is_empty() {
        let _ = transaction.abort();
        return Err(Error::Missing);
    }
    for (path, value) in &moved {
        files
            .put(value, Some(&key(handle.id, path)))
            .map_err(error)?
            .await
            .map_err(error)?;
    }
    for old in [Query::Key(source), Query::KeyRange(sources)] {
        files.delete(old).map_err(error)?.await.map_err(error)?;
    }
    committed(transaction.await)?;

    // Markers end in `/`, which no file path does.
    let changed = moved.into_iter().filter_map(|(path, value)| {
        let path = Path::new(&path)
            .ok()
            .filter(|path| !folder::is_skipped(path))?;
        let text = reads_text(&path)
            .then(|| value.as_string().filter(|text| text.len() <= MAX_TEXT))
            .flatten();
        Some(Change::Changed { path, text })
    });
    Ok(std::iter::once(Change::Removed(from))
        .chain(changed)
        .collect())
}

/// Deletes the file or directory `path` in the folder at `handle`, a directory with every key
/// under it.
pub async fn remove(handle: Handle, path: Path) -> Result<Vec<Change>, Error> {
    let file = file(handle.id, &path);
    let dir = range(handle.id, Some(&path))?;
    let database = open().await?;

    // As in `write`, the transaction awaits nothing but its own requests.
    let transaction = database
        .0
        .transaction(&[FILES], TransactionMode::ReadWrite)
        .map_err(error)?;
    let files = transaction.object_store(FILES).map_err(error)?;
    if !holds(&files, file.clone(), dir.clone()).await? {
        let _ = transaction.abort();
        return Err(Error::Missing);
    }
    for gone in [Query::Key(file), Query::KeyRange(dir)] {
        files.delete(gone).map_err(error)?.await.map_err(error)?;
    }
    committed(transaction.await)?;
    Ok(vec![Change::Removed(path)])
}

/// The folder at `handle`: a scan of its files. Nobody else writes to the library while bulin
/// runs, and bulin learns of its own writes when they finish, so nothing follows.
pub fn stream(handle: Handle) -> impl Stream<Item = Watched> {
    stream::once(async move {
        scan(&handle).await.unwrap_or_else(|error| {
            tracing::warn!("cannot read the folder {}: {error}", handle.name);
            Watched::Gone
        })
    })
    .chain(stream::pending())
}

/// Every file of the folder at `handle` whose text storage reads, or `Gone` when the library no
/// longer has the folder.
pub async fn scan(handle: &Handle) -> Result<Watched, Error> {
    let database = open().await?;
    let transaction = database
        .0
        .transaction(&[ENTRIES, FILES], TransactionMode::ReadOnly)
        .map_err(error)?;
    let row = transaction
        .object_store(ENTRIES)
        .map_err(error)?
        .get(entry(handle.id))
        .map_err(error)?
        .await
        .map_err(error)?;
    if row.is_none() {
        return Ok(Watched::Gone);
    }

    let range = range(handle.id, None)?;
    let files = transaction.object_store(FILES).map_err(error)?;
    let keys = files
        .get_all_keys(Some(Query::KeyRange(range.clone())), None)
        .map_err(error)?
        .await
        .map_err(error)?;
    let texts = files
        .get_all(Some(Query::KeyRange(range)), None)
        .map_err(error)?
        .await
        .map_err(error)?;

    // Both lists follow the keys' order.
    let scan = keys
        .into_iter()
        .zip(texts)
        .filter_map(|(key, text)| {
            let path = Path::new(&path_of(key)?)
                .ok()
                .filter(|path| !folder::is_skipped(path) && reads_text(path))?;
            let text = text.as_string().filter(|text| text.len() <= MAX_TEXT);
            Some((path, text))
        })
        .collect::<BTreeMap<_, _>>();
    Ok(Watched::Scan(scan))
}

/// The library's entries.
async fn list() -> Result<Vec<Item>, Error> {
    let database = open().await?;
    let transaction = database
        .0
        .transaction(&[ENTRIES], TransactionMode::ReadOnly)
        .map_err(error)?;
    let entries = transaction
        .object_store(ENTRIES)
        .map_err(error)?
        .get_all(None, None)
        .map_err(error)?
        .await
        .map_err(error)?;

    let field = |entry: &JsValue, name: &str| Reflect::get(entry, &JsValue::from_str(name)).ok();
    Ok(entries
        .iter()
        .filter_map(|entry| {
            let kind = field(entry, "kind")?.as_string()?;
            let kind = [Kind::Folder, Kind::Project]
                .into_iter()
                .find(|known| known.as_str() == kind)?;
            let id = field(entry, "id")?.as_f64()?;
            let name = field(entry, "name")?.as_string()?;
            Some(Item {
                id: Id::new(id as u64),
                name,
                kind,
            })
        })
        .collect())
}

/// Adds a folder named `name` to the library, holding a main file with the default shader.
async fn create(name: String) -> Result<Item, Error> {
    let main = Path::new(MAIN).expect("invariant: the main file's path is valid");
    let settings = Settings::new(Some(main.clone()), Uniforms::default(), None).to_json();
    let settings_path = Path::new(SETTINGS).expect("invariant: the settings path is valid");
    store_folder(
        name,
        vec![(main, DEFAULT_SOURCE.to_owned()), (settings_path, settings)],
        Vec::new(),
    )
    .await
}

/// Adds a folder named `name` to the library, holding `files` with their texts and the
/// directories `dirs`, each kept by a marker key.
async fn store_folder(
    name: String,
    files: Vec<(Path, String)>,
    dirs: Vec<Path>,
) -> Result<Item, Error> {
    let row = record(None, &name, Kind::Folder);
    let values = files
        .iter()
        .map(|(path, text)| (path, JsValue::from_str(text)))
        .collect::<Vec<_>>();
    let markers = dirs.iter().map(library::marker_key).collect::<Vec<_>>();
    let empty = JsValue::from_str("");
    persist().await;
    let database = open().await?;

    // As in `write`, the transaction awaits nothing but its own requests.
    let transaction = database
        .0
        .transaction(&[ENTRIES, FILES], TransactionMode::ReadWrite)
        .map_err(error)?;
    let id = transaction
        .object_store(ENTRIES)
        .map_err(error)?
        .add(&row, None)
        .map_err(error)?
        .await
        .map_err(error)?;
    let id = Id::new(
        id.as_f64()
            .ok_or_else(|| Error::Io("the library didn't number the folder".to_owned()))?
            as u64,
    );
    let store = transaction.object_store(FILES).map_err(error)?;
    for (path, value) in &values {
        store
            .put(value, Some(&file(id, path)))
            .map_err(error)?
            .await
            .map_err(error)?;
    }
    for marker in &markers {
        store
            .put(&empty, Some(&key(id, marker)))
            .map_err(error)?
            .await
            .map_err(error)?;
    }
    committed(transaction.await)?;

    Ok(Item {
        id,
        name,
        kind: Kind::Folder,
    })
}

/// Imports a zip archive the user picks as a new folder, named after the archive. Returns the
/// folder and what the import left out, or `None` when the user cancels.
async fn import_zip() -> Result<Option<(Item, Vec<String>)>, Error> {
    let Some(picked) = rfd::AsyncFileDialog::new()
        .set_title("Import a folder from a zip archive")
        .add_filter("Zip archive", &["zip"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    if picked.inner().size() > MAX_COMPRESSED as f64 {
        return Err(refused(archive::Error::TooLarge));
    }
    let bytes = picked.read().await;

    let invalid =
        |error: zip::result::ZipError| Error::Io(format!("it isn't a valid zip ({error})"));
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(invalid)?;
    if zip.len() > MAX_ENTRIES {
        return Err(refused(archive::Error::TooManyEntries));
    }
    // Reading one byte past the budget is what tells an entry that overruns it, so an archive
    // that unpacks without bound stops there.
    let mut budget = Budget::new();
    let mut entries = Vec::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(invalid)?;
        let name = entry.name().map_err(invalid)?.into_owned();
        if archive::skipped(&name) {
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .take(budget.limit())
            .read_to_end(&mut bytes)
            .map_err(|error| Error::Io(format!("{name} can't be unpacked ({error})")))?;
        budget.spend(bytes.len() as u64).map_err(refused)?;
        entries.push((name, bytes));
    }

    let imported = archive::import(entries).map_err(refused)?;
    let item = store_folder(stem(&picked.file_name()), imported.files, imported.dirs).await?;
    Ok(Some((item, imported.notices)))
}

/// Imports a directory the user picks as a new folder of the same name. Returns the folder and
/// what the import left out, or `None` when the user cancels.
async fn import_directory() -> Result<Option<(Item, Vec<String>)>, Error> {
    let Some(files) = pick_directory().await? else {
        return Ok(None);
    };
    if files.len() > MAX_ENTRIES {
        return Err(refused(archive::Error::TooManyEntries));
    }
    if files.iter().map(|file| file.size()).sum::<f64>() > MAX_COMPRESSED as f64 {
        return Err(refused(archive::Error::TooLarge));
    }

    // Each file's path starts with the picked directory's name, which names the folder.
    let mut name = None;
    let mut budget = Budget::new();
    let mut entries = Vec::new();
    for file in files {
        let relative = Reflect::get(&file, &JsValue::from_str("webkitRelativePath"))
            .ok()
            .and_then(|path| path.as_string())
            .unwrap_or_else(|| file.name());
        let Some((dir, path)) = relative.split_once('/') else {
            continue;
        };
        name.get_or_insert_with(|| dir.to_owned());
        if archive::skipped(path) {
            continue;
        }
        let buffer = JsFuture::from(file.array_buffer())
            .await
            .map_err(|_| Error::Io(format!("{path} can't be read")))?;
        let bytes = Uint8Array::new(&buffer).to_vec();
        budget.spend(bytes.len() as u64).map_err(refused)?;
        entries.push((path.to_owned(), bytes));
    }
    let name = name.ok_or_else(|| Error::Io("the directory holds no files".to_owned()))?;

    let imported = archive::import(entries).map_err(refused)?;
    let item = store_folder(name, imported.files, imported.dirs).await?;
    Ok(Some((item, imported.notices)))
}

/// The files under a directory the user picks, or `None` when they cancel.
///
/// The picker is a hidden directory input, which reports a pick through `change` and a dismissal
/// through `cancel`, so waiting for it always ends.
async fn pick_directory() -> Result<Option<Vec<web_sys::File>>, Error> {
    let unavailable = || Error::Io("the page can't show a directory picker".to_owned());
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(unavailable)?;
    let body = document.body().ok_or_else(unavailable)?;
    let input = document
        .create_element("input")
        .ok()
        .and_then(|element| element.dyn_into::<HtmlInputElement>().ok())
        .ok_or_else(unavailable)?;
    input.set_type("file");
    input.set_webkitdirectory(true);
    input.set_hidden(true);
    body.append_with_node_1(&input).map_err(|_| unavailable())?;

    let mut resolve = None;
    let picked = Promise::new(&mut |done, _| resolve = Some(done));
    let resolve = resolve.expect("invariant: a promise runs its executor at once");
    let listeners = [("change", true), ("cancel", false)].map(|(event, picked)| {
        let resolve = resolve.clone();
        let listener = Closure::<dyn FnMut()>::new(move || {
            let _ = resolve.call1(&JsValue::NULL, &JsValue::from_bool(picked));
        });
        let added =
            input.add_event_listener_with_callback(event, listener.as_ref().unchecked_ref());
        (added, listener)
    });
    let result = if listeners.iter().all(|(added, _)| added.is_ok()) {
        input.click();
        let picked = JsFuture::from(picked).await;
        Ok(picked.ok().and_then(|picked| picked.as_bool()) == Some(true))
    } else {
        Err(unavailable())
    };
    input.remove();
    drop(listeners);

    if !result? {
        return Ok(None);
    }
    let Some(list) = input.files() else {
        return Ok(None);
    };
    Ok(Some(
        (0..list.length())
            .filter_map(|index| list.get(index))
            .collect(),
    ))
}

/// Imports a project file the user picks as a new library project, named after the file.
/// Returns the project and its text, or `None` when the user cancels.
async fn import_project() -> Result<Option<(Item, Arc<String>)>, Error> {
    let Some(picked) = rfd::AsyncFileDialog::new()
        .set_title("Import a project")
        .add_filter("bulin project", &["bulin"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    if picked.inner().size() > MAX_COMPRESSED as f64 {
        return Err(refused(archive::Error::TooLarge));
    }
    let text = String::from_utf8(picked.read().await)
        .map_err(|_| Error::Io("it isn't UTF-8 text".to_owned()))?;
    Project::from_json(&text).map_err(|error| Error::Io(error.to_string()))?;

    let name = stem(&picked.file_name());
    let id = store_project(None, name.clone(), text.clone()).await?;
    let item = Item {
        id,
        name,
        kind: Kind::Project,
    };
    Ok(Some((item, Arc::new(text))))
}

/// Downloads the folder `id`, as stored, as the zip archive `<name>.zip`, without the files
/// bulin regenerates.
async fn export_folder(id: Id, name: String) -> Result<(), Error> {
    let range = range(id, None)?;
    let database = open().await?;
    let transaction = database
        .0
        .transaction(&[FILES], TransactionMode::ReadOnly)
        .map_err(error)?;
    let files = transaction.object_store(FILES).map_err(error)?;
    let keys = files
        .get_all_keys(Some(Query::KeyRange(range.clone())), None)
        .map_err(error)?
        .await
        .map_err(error)?;
    let values = files
        .get_all(Some(Query::KeyRange(range)), None)
        .map_err(error)?
        .await
        .map_err(error)?;

    let invalid =
        |error: zip::result::ZipError| Error::Io(format!("the zip can't be written ({error})"));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    // Both lists follow the keys' order. Markers end in `/`, which no file path does.
    for (key, value) in keys.into_iter().zip(values) {
        let Some(path) = path_of(key).filter(|path| !archive::skipped(path)) else {
            continue;
        };
        if path.ends_with('/') {
            zip.add_directory(path, options).map_err(invalid)?;
        } else {
            zip.start_file(&path, options).map_err(invalid)?;
            zip.write_all(value.as_string().unwrap_or_default().as_bytes())
                .map_err(|error| Error::Io(format!("{path} can't be packed ({error})")))?;
        }
    }
    let bytes = zip.finish().map_err(invalid)?.into_inner();

    downloaded(file::save_bytes(&format!("{name}.zip"), bytes).await)
}

/// Downloads the project `id`, as stored, as the file `<name>.bulin`.
async fn export_project(id: Id, name: String) -> Result<(), Error> {
    let path = Path::new(PROJECT).expect("invariant: the project's path is valid");
    let json = read(
        Handle {
            id,
            name: name.clone(),
        },
        path,
    )
    .await?;
    let saved = file::save_file(None, &format!("{name}.bulin"), json).await;
    downloaded(saved.map(|_| ()))
}

/// Stores the project `json` named `name` under the library entry `id`, or under a new entry when
/// `None`, and returns the entry's id.
pub(crate) async fn store_project(id: Option<Id>, name: String, json: String) -> Result<Id, Error> {
    let row = record(id, &name, Kind::Project);
    let json = JsValue::from_str(&json);
    persist().await;
    let database = open().await?;

    // A transaction commits as soon as no request of its own is pending, so between opening it
    // and committing nothing awaits anything else: what it stores is computed beforehand.
    let transaction = database
        .0
        .transaction(&[ENTRIES, FILES], TransactionMode::ReadWrite)
        .map_err(error)?;
    let id = transaction
        .object_store(ENTRIES)
        .map_err(error)?
        .put(&row, None)
        .map_err(error)?
        .await
        .map_err(error)?;
    let id = Id::new(
        id.as_f64()
            .ok_or_else(|| Error::Io("the library didn't number the project".to_owned()))?
            as u64,
    );
    transaction
        .object_store(FILES)
        .map_err(error)?
        .put(&json, Some(&key(id, PROJECT)))
        .map_err(error)?
        .await
        .map_err(error)?;
    committed(transaction.await)?;
    Ok(id)
}

/// Renames the library entry `id` to `name`.
async fn rename_entry(id: Id, name: String) -> Result<(), Error> {
    let name = JsValue::from_str(&name);
    let modified = JsValue::from_f64(js_sys::Date::now());
    let database = open().await?;

    // As in `write`, the transaction awaits nothing but its own requests.
    let transaction = database
        .0
        .transaction(&[ENTRIES], TransactionMode::ReadWrite)
        .map_err(error)?;
    let entries = transaction.object_store(ENTRIES).map_err(error)?;
    let Some(row) = entries
        .get(entry(id))
        .map_err(error)?
        .await
        .map_err(error)?
    else {
        let _ = transaction.abort();
        return Err(Error::Missing);
    };
    for (field, value) in [("name", &name), ("modified", &modified)] {
        Reflect::set(&row, &JsValue::from_str(field), value)
            .map_err(|_| Error::Io("the library's entry isn't an object".to_owned()))?;
    }
    entries
        .put(&row, None)
        .map_err(error)?
        .await
        .map_err(error)?;
    committed(transaction.await)
}

/// Deletes the library entry `id` with every file it holds.
async fn delete_entry(id: Id) -> Result<(), Error> {
    let files = range(id, None)?;
    let database = open().await?;

    // As in `write`, the transaction awaits nothing but its own requests.
    let transaction = database
        .0
        .transaction(&[ENTRIES, FILES], TransactionMode::ReadWrite)
        .map_err(error)?;
    transaction
        .object_store(ENTRIES)
        .map_err(error)?
        .delete(Query::Key(entry(id)))
        .map_err(error)?
        .await
        .map_err(error)?;
    transaction
        .object_store(FILES)
        .map_err(error)?
        .delete(Query::KeyRange(files))
        .map_err(error)?
        .await
        .map_err(error)?;
    committed(transaction.await)
}

/// Asks the browser, the first time, to keep the library even when space runs low. A refusal waits
/// for the library modal to show it.
async fn persist() {
    if ASKED.replace(true) {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    let kept = match window.navigator().storage().persist() {
        Ok(promise) => JsFuture::from(promise)
            .await
            .ok()
            .and_then(|kept| kept.as_bool()),
        Err(_) => None,
    };
    if kept != Some(true) {
        REFUSED.set(true);
    }
}

/// The `entries` row of an entry named `name` holding a `kind`, modified now: the entry `id`, or a
/// new entry IndexedDB numbers when `None`.
fn record(id: Option<Id>, name: &str, kind: Kind) -> Object {
    let row = Object::new();
    let fields = [
        ("name", JsValue::from_str(name)),
        ("kind", JsValue::from_str(kind.as_str())),
        ("modified", JsValue::from_f64(js_sys::Date::now())),
    ];
    for (field, value) in id.map(|id| ("id", entry(id))).into_iter().chain(fields) {
        Reflect::set(&row, &JsValue::from_str(field), &value)
            .expect("invariant: a plain object takes any field");
    }
    row
}

/// Opens the library, creating its stores the first time.
async fn open() -> Result<Connection, Error> {
    let mut request = Factory::new()
        .map_err(error)?
        .open(DATABASE, Some(VERSION))
        .map_err(error)?;
    request.on_upgrade_needed(|event| {
        let Ok(database) = event.database() else {
            return;
        };
        let mut entries = ObjectStoreParams::new();
        entries
            .auto_increment(true)
            .key_path(Some(KeyPath::new_single("id")));
        for (store, params) in [(ENTRIES, entries), (FILES, ObjectStoreParams::new())] {
            if let Err(error) = database.create_object_store(store, params) {
                tracing::warn!("cannot create the library's {store}: {error}");
            }
        }
    });
    request.await.map(Connection).map_err(error)
}

/// Why an import was refused, for the user.
fn refused(error: archive::Error) -> Error {
    Error::Io(error.to_string())
}

/// The outcome of a download: done also when the user dismissed it.
fn downloaded(result: Result<(), file::Error>) -> Result<(), Error> {
    match result {
        Ok(()) | Err(file::Error::DialogClosed) => Ok(()),
        Err(error) => Err(Error::Io(error.to_string())),
    }
}

/// The name of the library entry an imported file becomes: its name without the extension.
fn stem(file_name: &str) -> String {
    match file_name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_owned(),
        Some(_) | None => file_name.to_owned(),
    }
}

/// Whether the transaction that ended with `result` stored its changes.
fn committed(result: Result<TransactionResult, idb::Error>) -> Result<(), Error> {
    match result {
        Ok(TransactionResult::Committed) => Ok(()),
        Ok(TransactionResult::Aborted) => {
            Err(Error::Io("the browser didn't store the change".to_owned()))
        }
        Err(error) => Err(self::error(error)),
    }
}

/// Whether `files` holds the key `file` or any key in `dir`, the range under it: whether a file or
/// a directory has that path.
async fn holds(files: &ObjectStore, file: JsValue, dir: KeyRange) -> Result<bool, Error> {
    for query in [Query::Key(file), Query::KeyRange(dir)] {
        let count = files
            .count(Some(query))
            .map_err(error)?
            .await
            .map_err(error)?;
        if count > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The keys of the files under the directory `dir` of the entry `id`, `None` being its root.
fn range(id: Id, dir: Option<&Path>) -> Result<KeyRange, Error> {
    let (lower, upper) = library::range(dir);
    let upper = match upper {
        Upper::Before(path) => key(id, &path),
        Upper::AfterStrings => Array::of2(&entry(id), &Array::new()).into(),
    };
    KeyRange::bound(&key(id, &lower), &upper, Some(false), Some(true)).map_err(error)
}

/// The path part of the file key `key`.
fn path_of(key: JsValue) -> Option<String> {
    key.dyn_into::<Array>().ok()?.get(1).as_string()
}

/// The key of the file `path` in the entry `id`.
fn file(id: Id, path: &Path) -> JsValue {
    let (id, path) = library::file_key(id, path);
    key(id, &path)
}

/// The array key `[id, path]`.
fn key(id: Id, path: &str) -> JsValue {
    Array::of2(&entry(id), &JsValue::from_str(path)).into()
}

/// The key of the entry `id`.
fn entry(id: Id) -> JsValue {
    JsValue::from_f64(id.get() as f64)
}

/// The storage error behind `error`, by the name of the `DOMException` it carries.
fn error(error: idb::Error) -> Error {
    let exception = match &error {
        idb::Error::DomException(exception) => Some(exception.clone()),
        idb::Error::IndexedDbOpenFailed(value)
        | idb::Error::IndexedDbNotFound(value)
        | idb::Error::TransactionOpenFailed(value)
        | idb::Error::GetFailed(value)
        | idb::Error::GetAllFailed(value)
        | idb::Error::GetAllKeysFailed(value)
        | idb::Error::AddFailed(value)
        | idb::Error::UpdateFailed(value)
        | idb::Error::DeleteFailed(value)
        | idb::Error::CountFailed(value) => value.dyn_ref::<DomException>().cloned(),
        _ => None,
    };
    match exception.as_ref().map(DomException::name).as_deref() {
        Some("QuotaExceededError") => Error::Quota(error.to_string()),
        Some("SecurityError" | "NotAllowedError") => Error::Denied(error.to_string()),
        _ => Error::Io(error.to_string()),
    }
}
