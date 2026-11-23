use crate::command::Command;
use crate::error::CommandError;
use composer_model::{Change, HashCache, Project};
use std::cell::RefCell;

/// Who made an edit, for the history list. Not part of the project or its hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    User,
    Ai,
    Template,
    Recipe,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::User => "user",
            Source::Ai => "ai",
            Source::Template => "template",
            Source::Recipe => "recipe",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "user" => Source::User,
            "ai" => Source::Ai,
            "template" => Source::Template,
            "recipe" => Source::Recipe,
            _ => return None,
        })
    }
}

/// One undo step: the changes of one command, or of a whole group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub label: String,
    pub changes: Vec<Change>,
    pub source: Source,
    /// Commands that produced the changes (1 outside a group).
    pub commands: usize,
}

/// One row of the history list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub index: usize,
    pub label: String,
    pub source: Source,
    pub command_count: usize,
}

/// A page of history, oldest first. `cursor` counts the applied steps, so entries
/// with `index < cursor` are applied and the rest can be redone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    pub cursor: usize,
    pub total: usize,
}

#[derive(Debug)]
struct OpenGroup {
    label: String,
    source: Source,
    commands: usize,
    changes: Vec<Change>,
}

/// A project plus its undo and redo history. All edits go through `execute`.
#[derive(Debug)]
pub struct Editor {
    project: Project,
    undo: Vec<Step>,
    redo: Vec<Step>,
    group: Option<OpenGroup>,
    /// Every change applied since the last `take_journal`, in order (D25). Off by default.
    journal: Vec<Change>,
    journal_on: bool,
    /// Incremental state hash (D47), kept in step by `touch` after every applied change.
    hash_cache: RefCell<HashCache>,
}

/// Applies changes in order; if one fails, reverts the ones already applied.
fn apply_all(
    project: &mut Project,
    cache: &mut HashCache,
    changes: &[Change],
) -> Result<(), CommandError> {
    for (i, c) in changes.iter().enumerate() {
        if let Err(e) = project.apply(c) {
            for done in changes[..i].iter().rev() {
                // Inverses of just-applied changes cannot fail; ignore to keep the original error.
                let inv = done.inverse();
                if project.apply(&inv).is_ok() {
                    cache.touch(&inv);
                }
            }
            return Err(e.into());
        }
        cache.touch(c);
    }
    Ok(())
}

fn apply_inverse(
    project: &mut Project,
    cache: &mut HashCache,
    changes: &[Change],
) -> Result<(), CommandError> {
    let inv: Vec<Change> = changes.iter().rev().map(Change::inverse).collect();
    apply_all(project, cache, &inv)
}

impl Editor {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            group: None,
            journal: Vec::new(),
            journal_on: false,
            hash_cache: RefCell::new(HashCache::new()),
        }
    }

    /// Starts or stops recording applied changes for the change feed (D25). Turning it off
    /// drops what was recorded.
    pub fn track_changes(&mut self, on: bool) {
        self.journal_on = on;
        if !on {
            self.journal.clear();
        }
    }

    /// Changes applied since the last call, oldest first. Replaying them on the project as it
    /// was then gives the project as it is now (undo, redo and rollbacks record their inverses).
    pub fn take_journal(&mut self) -> Vec<Change> {
        std::mem::take(&mut self.journal)
    }

    fn record(&mut self, changes: impl IntoIterator<Item = Change>) {
        if self.journal_on {
            self.journal.extend(changes);
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Hash of the current document, equal to `project().state_hash()`.
    pub fn state_hash(&self) -> u64 {
        self.hash_cache.borrow_mut().hash(&self.project)
    }

    pub fn into_project(self) -> Project {
        self.project
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Label of the step `undo` would revert.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    /// True while a group is open.
    pub fn group_open(&self) -> bool {
        self.group.is_some()
    }

    /// Label of an open group.
    pub fn open_group_label(&self) -> Option<&str> {
        self.group.as_ref().map(|g| g.label.as_str())
    }

    /// Steps `offset..offset+limit` of the whole history (undo stack then redo stack, oldest first).
    pub fn history(&self, offset: usize, limit: usize) -> HistoryPage {
        let total = self.undo.len() + self.redo.len();
        let entries = self
            .undo
            .iter()
            .chain(self.redo.iter().rev())
            .enumerate()
            .skip(offset)
            .take(limit)
            .map(|(index, s)| HistoryEntry {
                index,
                label: s.label.clone(),
                source: s.source,
                command_count: s.commands,
            })
            .collect();
        HistoryPage {
            entries,
            cursor: self.undo.len(),
            total,
        }
    }

    /// Undoes up to `steps` steps; returns how many were applied. Stops at the first error.
    pub fn undo_steps(&mut self, steps: usize) -> Result<usize, CommandError> {
        let mut n = 0;
        while n < steps && self.undo()? {
            n += 1;
        }
        Ok(n)
    }

    /// Redoes up to `steps` steps; returns how many were applied.
    pub fn redo_steps(&mut self, steps: usize) -> Result<usize, CommandError> {
        let mut n = 0;
        while n < steps && self.redo()? {
            n += 1;
        }
        Ok(n)
    }

    /// Validates and applies one command atomically. On error the project is unchanged.
    /// Outside a group the command is its own undo step; a no-op command adds no step.
    pub fn execute(&mut self, cmd: &Command) -> Result<(), CommandError> {
        let changes = cmd.plan(&self.project)?;
        if changes.is_empty() {
            return Ok(());
        }
        apply_all(&mut self.project, self.hash_cache.get_mut(), &changes)?;
        if self.journal_on {
            self.journal.extend(changes.iter().cloned());
        }
        self.redo.clear();
        match &mut self.group {
            Some(g) => {
                g.changes.extend(changes);
                g.commands += 1;
            }
            None => self.undo.push(Step {
                label: cmd.name().to_string(),
                changes,
                source: Source::User,
                commands: 1,
            }),
        }
        Ok(())
    }

    /// Starts a group: every command until `commit_group` undoes as one step.
    pub fn begin_group(&mut self, label: &str) -> Result<(), CommandError> {
        self.begin_group_as(label, Source::User)
    }

    /// Like `begin_group` with a source tag for the history list.
    pub fn begin_group_as(&mut self, label: &str, source: Source) -> Result<(), CommandError> {
        if self.group.is_some() {
            return Err(CommandError::GroupAlreadyOpen);
        }
        self.group = Some(OpenGroup {
            label: label.to_string(),
            source,
            commands: 0,
            changes: Vec::new(),
        });
        Ok(())
    }

    /// Closes the group as one undo step (nothing is pushed when it changed nothing).
    pub fn commit_group(&mut self) -> Result<(), CommandError> {
        let g = self.group.take().ok_or(CommandError::NoOpenGroup)?;
        if !g.changes.is_empty() {
            self.undo.push(Step {
                label: g.label,
                changes: g.changes,
                source: g.source,
                commands: g.commands,
            });
        }
        Ok(())
    }

    /// Closes the group and reverts everything it did.
    pub fn rollback_group(&mut self) -> Result<(), CommandError> {
        let g = self.group.take().ok_or(CommandError::NoOpenGroup)?;
        apply_inverse(&mut self.project, self.hash_cache.get_mut(), &g.changes)?;
        self.record(g.changes.iter().rev().map(Change::inverse));
        Ok(())
    }

    /// Runs `f` as one undo step; any error rolls the whole group back.
    pub fn group<T>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Editor) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        self.group_as(label, Source::User, f)
    }

    /// Like `group` with a source tag.
    pub fn group_as<T>(
        &mut self,
        label: &str,
        source: Source,
        f: impl FnOnce(&mut Editor) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        self.begin_group_as(label, source)?;
        match f(self) {
            Ok(v) => {
                self.commit_group()?;
                Ok(v)
            }
            Err(e) => {
                self.rollback_group()?;
                Err(e)
            }
        }
    }

    /// Reverts the latest step. Returns false when there is nothing to undo.
    pub fn undo(&mut self) -> Result<bool, CommandError> {
        if self.group.is_some() {
            return Err(CommandError::GroupAlreadyOpen);
        }
        let Some(step) = self.undo.pop() else {
            return Ok(false);
        };
        if let Err(e) = apply_inverse(&mut self.project, self.hash_cache.get_mut(), &step.changes) {
            self.undo.push(step);
            return Err(e);
        }
        self.record(step.changes.iter().rev().map(Change::inverse));
        self.redo.push(step);
        Ok(true)
    }

    /// Re-applies the latest undone step. Returns false when there is nothing to redo.
    pub fn redo(&mut self) -> Result<bool, CommandError> {
        if self.group.is_some() {
            return Err(CommandError::GroupAlreadyOpen);
        }
        let Some(step) = self.redo.pop() else {
            return Ok(false);
        };
        if let Err(e) = apply_all(&mut self.project, self.hash_cache.get_mut(), &step.changes) {
            self.redo.push(step);
            return Err(e);
        }
        self.record(step.changes.iter().cloned());
        self.undo.push(step);
        Ok(true)
    }
}
