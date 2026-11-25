//! GigCraft commands.
//!
//! Every action is a command: a dotted id (`song.create`, `setlist.add_song`…)
//! with JSON parameters, returning a JSON result and a list of what changed
//! (inspect). The same commands back the UIs, keyboard and MIDI mappings, the
//! CLI, the control channel and the MCP server. Commands never open dialogs.
//!
//! Every change is recorded for undo/redo. Undoing a creation moves the record
//! to the library's trash, so nothing is ever hard-deleted.

pub mod demo;
mod params;
mod perform_cmds;
mod setlist_cmds;
mod song_cmds;
mod specs;

pub use specs::{CommandSpec, ParamSpec, specs};

use gigcraft_model::{Setlist, SetlistId, Song, SongId};
use gigcraft_store::{Library, StoreError};
use serde::Serialize;
use serde_json::Value;

/// How many steps undo remembers.
pub const UNDO_DEPTH: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("unknown command `{0}`")]
    UnknownCommand(String),
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl CommandError {
    /// A stable machine-readable code for clients.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownCommand(_) => "unknown_command",
            Self::InvalidParams(_) => "invalid_params",
            Self::NotFound(_) => "not_found",
            Self::Rejected(_) => "rejected",
            Self::Store(_) => "storage",
        }
    }
}

/// What a command changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeSummary {
    pub kind: &'static str,
    pub id: String,
    pub action: &'static str,
    pub name: String,
}

/// The result of a command.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outcome {
    pub result: Value,
    pub changes: Vec<ChangeSummary>,
}

#[derive(Debug, Clone)]
#[expect(
    clippy::large_enum_variant,
    reason = "undo history is short; boxing only adds noise"
)]
enum Change {
    Song {
        id: SongId,
        before: Option<(Song, String)>,
        after: Option<(Song, String)>,
    },
    Setlist {
        id: SetlistId,
        before: Option<Setlist>,
        after: Option<Setlist>,
    },
}

#[derive(Debug, Clone)]
struct Transaction {
    label: String,
    changes: Vec<Change>,
}

/// An open library plus undo history: the one place commands run.
#[derive(Debug)]
pub struct Engine {
    lib: Library,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    /// The running show in perform mode, if any.
    show: Option<gigcraft_perform::Show>,
}

impl Engine {
    #[must_use]
    pub fn new(lib: Library) -> Self {
        Self {
            lib,
            undo: Vec::new(),
            redo: Vec::new(),
            show: None,
        }
    }

    #[must_use]
    pub fn library(&self) -> &Library {
        &self.lib
    }

    /// The running show (perform mode), if any.
    #[must_use]
    pub fn show(&self) -> Option<&gigcraft_perform::Show> {
        self.show.as_ref()
    }

    /// Forget undo and redo (after loading demo content or reloading).
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Label of the step `edit.undo` would revert.
    #[must_use]
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|t| t.label.as_str())
    }

    #[must_use]
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|t| t.label.as_str())
    }

    /// Run a command. `params` must be a JSON object or null.
    pub fn run(&mut self, id: &str, params: &Value) -> Result<Outcome, CommandError> {
        let p = params::Params::new(params)?;
        let mut tx = Tx {
            label: id.to_string(),
            changes: Vec::new(),
        };
        let result = match id {
            "edit.undo" => return self.undo(),
            "edit.redo" => return self.redo(),
            _ => self.dispatch(id, &p, &mut tx)?,
        };
        let changes = tx.changes.iter().map(summary).collect();
        if !tx.changes.is_empty() {
            self.undo.push(Transaction {
                label: tx.label,
                changes: tx.changes,
            });
            if self.undo.len() > UNDO_DEPTH {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        Ok(Outcome { result, changes })
    }

    fn dispatch(
        &mut self,
        id: &str,
        p: &params::Params<'_>,
        tx: &mut Tx,
    ) -> Result<Value, CommandError> {
        match id {
            "library.info" => Ok(self.library_info()),
            "library.reload" => {
                self.lib.reload()?;
                self.undo.clear();
                self.redo.clear();
                Ok(self.library_info())
            }
            _ if id.starts_with("song.") => song_cmds::run(self, id, p, tx),
            _ if id.starts_with("perform.") => perform_cmds::run(self, id, p),
            _ if id.starts_with("setlist.") => setlist_cmds::run(self, id, p, tx),
            _ => Err(CommandError::UnknownCommand(id.to_string())),
        }
    }

    fn library_info(&self) -> Value {
        let info = self.lib.info();
        serde_json::json!({
            "id": info.id.to_string(),
            "name": info.name,
            "root": self.lib.root().display().to_string(),
            "songs": self.lib.songs().len(),
            "setlists": self.lib.setlists().len(),
            "problems": self.lib.problems().iter()
                .map(|pr| serde_json::json!({"path": pr.path.display().to_string(), "message": pr.message}))
                .collect::<Vec<_>>(),
            "undo": self.undo_label(),
            "redo": self.redo_label(),
        })
    }

    // ---- recorded mutations -------------------------------------------------

    pub(crate) fn put_song(
        &mut self,
        tx: &mut Tx,
        song: Song,
        chart: Option<&str>,
    ) -> Result<(), CommandError> {
        let id = song.id;
        let before = self.snapshot_song(id);
        let chart_after = chart
            .map(str::to_string)
            .or_else(|| before.as_ref().map(|b| b.1.clone()));
        self.lib.save_song(song.clone(), chart)?;
        tx.changes.push(Change::Song {
            id,
            before,
            after: Some((song, chart_after.unwrap_or_default())),
        });
        Ok(())
    }

    pub(crate) fn remove_song(&mut self, tx: &mut Tx, id: SongId) -> Result<(), CommandError> {
        let before = self.snapshot_song(id);
        self.lib.trash_song(id)?;
        tx.changes.push(Change::Song {
            id,
            before,
            after: None,
        });
        Ok(())
    }

    pub(crate) fn put_setlist(
        &mut self,
        tx: &mut Tx,
        setlist: Setlist,
    ) -> Result<(), CommandError> {
        let id = setlist.id;
        let before = self.lib.setlist(id).cloned();
        self.lib.save_setlist(setlist.clone())?;
        tx.changes.push(Change::Setlist {
            id,
            before,
            after: Some(setlist),
        });
        Ok(())
    }

    pub(crate) fn remove_setlist(
        &mut self,
        tx: &mut Tx,
        id: SetlistId,
    ) -> Result<(), CommandError> {
        let before = self.lib.setlist(id).cloned();
        self.lib.trash_setlist(id)?;
        tx.changes.push(Change::Setlist {
            id,
            before,
            after: None,
        });
        Ok(())
    }

    fn snapshot_song(&self, id: SongId) -> Option<(Song, String)> {
        let song = self.lib.song(id)?.clone();
        Some((song, self.lib.chart(id).unwrap_or_default().to_string()))
    }

    // ---- undo / redo --------------------------------------------------------

    fn apply(
        &mut self,
        changes: &[Change],
        forward: bool,
    ) -> Result<Vec<ChangeSummary>, CommandError> {
        let mut out = Vec::new();
        let ordered: Box<dyn Iterator<Item = &Change>> = if forward {
            Box::new(changes.iter())
        } else {
            Box::new(changes.iter().rev())
        };
        for change in ordered {
            let inverse = match change {
                Change::Song { id, before, after } => {
                    let target = if forward { after } else { before };
                    let current = self.snapshot_song(*id);
                    match (target, current.is_some()) {
                        (Some((song, chart)), _) => {
                            self.lib.save_song(song.clone(), Some(chart))?
                        }
                        (None, true) => self.lib.trash_song(*id)?,
                        (None, false) => {}
                    }
                    Change::Song {
                        id: *id,
                        before: current,
                        after: target.clone(),
                    }
                }
                Change::Setlist { id, before, after } => {
                    let target = if forward { after } else { before };
                    let current = self.lib.setlist(*id).cloned();
                    match (target, current.is_some()) {
                        (Some(s), _) => self.lib.save_setlist(s.clone())?,
                        (None, true) => self.lib.trash_setlist(*id)?,
                        (None, false) => {}
                    }
                    Change::Setlist {
                        id: *id,
                        before: current,
                        after: target.clone(),
                    }
                }
            };
            out.push(summary(&inverse));
        }
        Ok(out)
    }

    fn undo(&mut self) -> Result<Outcome, CommandError> {
        let tx = self
            .undo
            .pop()
            .ok_or_else(|| CommandError::Rejected("nothing to undo".into()))?;
        let changes = match self.apply(&tx.changes, false) {
            Ok(c) => c,
            Err(e) => {
                self.undo.push(tx);
                return Err(e);
            }
        };
        let label = tx.label.clone();
        self.redo.push(tx);
        Ok(Outcome {
            result: serde_json::json!({ "undone": label }),
            changes,
        })
    }

    fn redo(&mut self) -> Result<Outcome, CommandError> {
        let tx = self
            .redo
            .pop()
            .ok_or_else(|| CommandError::Rejected("nothing to redo".into()))?;
        let changes = match self.apply(&tx.changes, true) {
            Ok(c) => c,
            Err(e) => {
                self.redo.push(tx);
                return Err(e);
            }
        };
        let label = tx.label.clone();
        self.undo.push(tx);
        Ok(Outcome {
            result: serde_json::json!({ "redone": label }),
            changes,
        })
    }
}

/// Changes collected while one command runs.
#[derive(Debug)]
pub(crate) struct Tx {
    label: String,
    changes: Vec<Change>,
}

fn summary(change: &Change) -> ChangeSummary {
    match change {
        Change::Song { id, before, after } => ChangeSummary {
            kind: "song",
            id: id.to_string(),
            action: action(before.is_some(), after.is_some()),
            name: after
                .as_ref()
                .or(before.as_ref())
                .map(|(s, _)| s.title.clone())
                .unwrap_or_default(),
        },
        Change::Setlist { id, before, after } => ChangeSummary {
            kind: "setlist",
            id: id.to_string(),
            action: action(before.is_some(), after.is_some()),
            name: after
                .as_ref()
                .or(before.as_ref())
                .map(|s| s.name.clone())
                .unwrap_or_default(),
        },
    }
}

fn action(before: bool, after: bool) -> &'static str {
    match (before, after) {
        (false, true) => "created",
        (true, true) => "updated",
        (true, false) => "trashed",
        (false, false) => "unchanged",
    }
}

#[cfg(test)]
mod tests;
