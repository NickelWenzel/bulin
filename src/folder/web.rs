//! A folder in the browser's library: an entry of IndexedDB's `entries` store, whose files are
//! records of the `files` store keyed by the entry's id and the file's path, and the modal that
//! lists the library's folders.

use std::collections::BTreeMap;
use std::sync::Arc;

use iced::futures::{Stream, StreamExt, stream};
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Center, Element, Fill, Task};
use idb::{
    Database, DatabaseEvent, Factory, KeyPath, KeyRange, ObjectStoreParams, Query, TransactionMode,
    TransactionResult,
};
use js_sys::{Array, Object, Reflect};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::DomException;

use super::opener::Instruction;
use super::{Error, Watched};
use crate::action::Action;
use crate::core::Uniforms;
use crate::core::folder::{
    self, Change, Entry, Expect, MAX_TEXT, Path, SETTINGS, Settings, reads_text,
};
use crate::core::library::{self, Id, Upper};
use crate::core::program::DEFAULT_SOURCE;
use crate::file;

const DATABASE: &str = "bulin";
const VERSION: u32 = 1;
/// The store of the library's entries: `{id, name, kind, modified}`, numbered by IndexedDB.
const ENTRIES: &str = "entries";
/// The store of the entries' files: their texts, keyed by `[id, path]`.
const FILES: &str = "files";
/// The `kind` of an entry that is a folder.
const FOLDER: &str = "folder";
/// The file a new folder starts with, as its main file.
const MAIN: &str = "main.wesl";

/// A folder, by its library entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Handle {
    id: Id,
    name: String,
}

/// The library modal, listing the folders to open.
#[derive(Debug, Default)]
pub struct Opener {
    shown: Option<Section>,
    entries: Vec<(Id, String)>,
    /// The name typed for a new folder.
    new_name: String,
    /// Whether a folder is being created or a project uploaded.
    busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Folders,
    Projects,
}

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Result<Vec<(Id, String)>, Error>),
    Open(Id, String),
    NameChanged(String),
    Create,
    Created(Result<Handle, Error>),
    Upload,
    Uploaded(Result<(file::Name, Arc<String>), file::Error>),
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
}

impl Opener {
    /// Shows the library's folders.
    pub fn open_folder(&mut self) -> Task<Message> {
        self.shown = Some(Section::Folders);
        Task::perform(list(), Message::Listed)
    }

    /// Shows the projects to open.
    pub fn open_project(&mut self) -> Task<Message> {
        self.shown = Some(Section::Projects);
        Task::none()
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
            Message::Open(id, name) => {
                self.shown = None;
                Action::instruction(Instruction::OpenFolder(Handle { id, name }))
            }
            Message::NameChanged(name) => {
                self.new_name = name;
                Action::none()
            }
            Message::Create => {
                let name = self.new_name.trim();
                if name.is_empty() || self.busy {
                    return Action::none();
                }
                self.busy = true;
                Action::task(Task::perform(create(name.to_owned()), Message::Created))
            }
            Message::Created(Ok(handle)) => {
                self.busy = false;
                self.shown = None;
                self.new_name.clear();
                Action::instruction(Instruction::OpenFolder(handle))
            }
            Message::Created(Err(error)) => {
                self.busy = false;
                Action::instruction(Instruction::Failed(format!(
                    "Could not create the folder: {error}"
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
        }
    }

    /// The modal, while it shows.
    pub fn view(&self) -> Option<Element<'_, Message>> {
        let content: Element<'_, Message> = match self.shown? {
            Section::Folders => {
                let entries: Element<'_, Message> = if self.entries.is_empty() {
                    text("No folders yet.").into()
                } else {
                    scrollable(
                        column(self.entries.iter().map(|(id, name)| {
                            row![
                                text(name).width(Fill),
                                button("Open").on_press(Message::Open(*id, name.clone())),
                            ]
                            .spacing(10)
                            .align_y(Center)
                            .into()
                        }))
                        .spacing(5),
                    )
                    .into()
                };
                let can_create = !self.busy && !self.new_name.trim().is_empty();
                let create = row![
                    text_input("Name", &self.new_name)
                        .on_input(Message::NameChanged)
                        .on_submit(Message::Create),
                    button("New Folder").on_press_maybe(can_create.then_some(Message::Create)),
                ]
                .spacing(10);

                column![text("Folders").size(20), entries, create]
                    .spacing(15)
                    .into()
            }
            Section::Projects => column![
                text("Projects").size(20),
                button("Upload a .bulin file").on_press(Message::Upload),
            ]
            .spacing(15)
            .into(),
        };

        Some(
            container(content)
                .width(400)
                .height(iced::Shrink.max(500))
                .padding(20)
                .style(container::rounded_box)
                .into(),
        )
    }

    /// Hides the modal.
    pub fn close(&mut self) {
        self.shown = None;
    }

    /// Whether the modal shows or a folder is being created or a project uploaded.
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
async fn scan(handle: &Handle) -> Result<Watched, Error> {
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

/// The library's folders, by id and name.
async fn list() -> Result<Vec<(Id, String)>, Error> {
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
        .filter(|entry| {
            field(entry, "kind")
                .and_then(|kind| kind.as_string())
                .as_deref()
                == Some(FOLDER)
        })
        .filter_map(|entry| {
            let id = field(entry, "id")?.as_f64()?;
            let name = field(entry, "name")?.as_string()?;
            Some((Id::new(id as u64), name))
        })
        .collect())
}

/// Adds a folder named `name` to the library, holding a main file with the default shader.
async fn create(name: String) -> Result<Handle, Error> {
    let main = Path::new(MAIN).expect("invariant: the main file's path is valid");
    let row = Object::new();
    for (field, value) in [
        ("name", JsValue::from_str(&name)),
        ("kind", JsValue::from_str(FOLDER)),
        ("modified", JsValue::from_f64(js_sys::Date::now())),
    ] {
        Reflect::set(&row, &JsValue::from_str(field), &value)
            .expect("invariant: a plain object takes any field");
    }
    let source = JsValue::from_str(DEFAULT_SOURCE);
    let settings =
        JsValue::from_str(&Settings::new(Some(main.clone()), Uniforms::default(), None).to_json());
    let settings_path = Path::new(SETTINGS).expect("invariant: the settings path is valid");
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
    let files = transaction.object_store(FILES).map_err(error)?;
    files
        .put(&source, Some(&file(id, &main)))
        .map_err(error)?
        .await
        .map_err(error)?;
    files
        .put(&settings, Some(&file(id, &settings_path)))
        .map_err(error)?
        .await
        .map_err(error)?;
    committed(transaction.await)?;

    Ok(Handle { id, name })
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
        | idb::Error::UpdateFailed(value) => value.dyn_ref::<DomException>().cloned(),
        _ => None,
    };
    match exception.as_ref().map(DomException::name).as_deref() {
        Some("QuotaExceededError") => Error::Quota(error.to_string()),
        Some("SecurityError" | "NotAllowedError") => Error::Denied(error.to_string()),
        _ => Error::Io(error.to_string()),
    }
}
