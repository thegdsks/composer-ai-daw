use crate::files::FileConfig;
use composer_commands::{Editor, Selection};
use composer_format::{AssetTable, TransportPrefs};
use composer_model::{Change, Id, Project, Tick};
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

struct Entry {
    id: u64,
    tx: Sender<String>,
    subscribed: bool,
}

/// The one open group (D32): who owns it and when it started.
pub(crate) struct OpenGroupInfo {
    pub token: Id,
    pub owner: u64,
    pub opened: Instant,
}

/// One sealed edit for the change feed (D25). `changes` is None when the edit replaced the
/// session or was too large to list, so a client must fetch the full state.
pub(crate) struct Feed {
    pub base_hash: u64,
    pub hash: u64,
    pub changes: Option<Vec<Change>>,
}

pub(crate) struct Inner {
    pub editor: Editor,
    /// Recent feeds, oldest first, bounded by `COMPOSERD_FEED_HISTORY`.
    pub feeds: VecDeque<Feed>,
    /// Session state outside the document: not undoable, not in the state hash.
    pub selection: Selection,
    pub playhead: Tick,
    /// Loop, metronome, count in and follow (D21): saved in file meta, not in the state hash.
    pub prefs: TransportPrefs,
    pub files: FileConfig,
    /// Audio assets the project may name (D39): session state, saved in the asst section.
    pub assets: AssetTable,
    pub open_group: Option<OpenGroupInfo>,
    /// D27: AI proposals waiting for a person; session state, not in the hash.
    pub proposals: crate::proposals::Proposals,
    clients: Vec<Entry>,
    next_client: u64,
}

/// The one live session plus the clients attached to it.
pub struct Hub {
    inner: Mutex<Inner>,
    max_clients: usize,
}

/// A connected client: its id and the queue its writer drains.
#[derive(Clone)]
pub struct ClientHandle {
    pub(crate) id: u64,
    pub(crate) tx: Sender<String>,
}

pub(crate) fn random_id() -> Id {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("os randomness unavailable");
    Id(u128::from_be_bytes(b))
}

impl Hub {
    pub fn new(max_clients: usize) -> Self {
        Self::with_files(max_clients, FileConfig::default())
    }

    /// Like [`Hub::new`] with explicit file and render settings.
    pub fn with_files(max_clients: usize, files: FileConfig) -> Self {
        let project = Project::new(random_id(), "Untitled");
        let mut editor = Editor::new(project);
        editor.track_changes(true);
        Self {
            inner: Mutex::new(Inner {
                editor,
                feeds: VecDeque::new(),
                selection: Selection::default(),
                playhead: 0,
                prefs: TransportPrefs::default(),
                files,
                assets: AssetTable::new(),
                open_group: None,
                proposals: Default::default(),
                clients: Vec::new(),
                next_client: 1,
            }),
            max_clients,
        }
    }

    /// A poisoned lock is recovered: edits are atomic, so state stays consistent.
    pub(crate) fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Attaches a client (subscribed to `state.changed` by default).
    /// Returns None when the client limit is reached.
    pub fn connect(&self) -> Option<(ClientHandle, Receiver<String>)> {
        let mut g = self.lock();
        if g.clients.len() >= self.max_clients {
            return None;
        }
        let (tx, rx) = channel();
        let id = g.next_client;
        g.next_client += 1;
        g.clients.push(Entry {
            id,
            tx: tx.clone(),
            subscribed: true,
        });
        Some((ClientHandle { id, tx }, rx))
    }

    pub fn disconnect(&self, client: &ClientHandle) {
        let mut g = self.lock();
        g.clients.retain(|c| c.id != client.id);
        // A trial left open by a vanished client must not stay applied.
        if g.open_group.as_ref().is_some_and(|o| o.owner == client.id) {
            g.abandon_group("disconnect");
        }
    }

    pub fn client_count(&self) -> usize {
        self.lock().clients.len()
    }
}

impl Inner {
    /// Swaps in a new project and resets everything that referred to the old one.
    pub fn replace_project(&mut self, project: Project) {
        self.editor = Editor::new(project);
        self.editor.track_changes(true);
        self.open_group = None;
        self.proposals = Default::default();
        self.selection = Selection::default();
        self.playhead = 0;
        self.prefs = TransportPrefs::default();
        self.assets = AssetTable::new();
    }

    /// Rolls back the open group, if any, and tells the other clients.
    pub fn abandon_group(&mut self, cause: &str) {
        if self.open_group.take().is_some() {
            let before = self.editor.state_hash();
            let _ = self.editor.rollback_group();
            let feed = self.seal_feed(before);
            let hash = self.editor.state_hash();
            let line = crate::rpc::state_changed_line(hash, cause, feed);
            self.broadcast(&line);
        }
    }

    /// Closes the journal of the edit that started at `before`. Returns the feed (also kept
    /// for `changes.since`) when the project hash moved or changes were recorded.
    pub fn seal_feed(&mut self, before: u64) -> Option<serde_json::Value> {
        let journal = self.editor.take_journal();
        let hash = self.editor.state_hash();
        // An empty journal with a new hash means the session was replaced.
        let replaced = journal.is_empty() && hash != before;
        if journal.is_empty() && !replaced {
            return None;
        }
        let too_big = journal.len() > self.files.max_feed_changes;
        let feed = Feed {
            base_hash: before,
            hash,
            changes: (!replaced && !too_big).then_some(journal),
        };
        let value = crate::rpc::feed_value(&feed);
        self.feeds.push_back(feed);
        while self.feeds.len() > self.files.feed_history {
            self.feeds.pop_front();
        }
        Some(value)
    }

    /// Rolls the group back when it outlived `COMPOSERD_GROUP_TIMEOUT_MS`.
    pub fn expire_group(&mut self) {
        let limit = Duration::from_millis(self.files.group_timeout_ms);
        if self
            .open_group
            .as_ref()
            .is_some_and(|o| o.opened.elapsed() >= limit)
        {
            self.abandon_group("group.timeout");
        }
    }

    pub fn set_subscribed(&mut self, client: u64, on: bool) {
        if let Some(c) = self.clients.iter_mut().find(|c| c.id == client) {
            c.subscribed = on;
        }
    }

    /// Queues a message to every subscribed client; callers hold the lock, so order is global.
    pub fn broadcast(&mut self, msg: &str) {
        self.clients
            .retain(|c| !c.subscribed || c.tx.send(msg.to_string()).is_ok());
    }
}
