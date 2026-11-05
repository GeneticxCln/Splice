use std::time::{Duration, Instant};

use crate::model::Project;

const MAX_UNDO: usize = 120;
const COALESCE_WINDOW: Duration = Duration::from_millis(650);

/// Snapshot-based undo/redo shared by every editing operation. Rapid changes
/// (dragging a slider, typing in a field) coalesce into one checkpoint so the
/// user does not have to undo the same gesture hundreds of times.
#[derive(Default)]
pub struct History {
    undo: Vec<Project>,
    redo: Vec<Project>,
    last_checkpoint: Option<Instant>,
}

impl History {
    pub fn checkpoint(&mut self, snapshot: Project) {
        self.checkpoint_at(snapshot, Instant::now());
    }

    pub fn checkpoint_at(&mut self, snapshot: Project, now: Instant) {
        let coalesce = self
            .last_checkpoint
            .map(|last| now.duration_since(last) < COALESCE_WINDOW)
            .unwrap_or(false);
        if !(coalesce && !self.undo.is_empty()) {
            self.undo.push(snapshot);
            if self.undo.len() > MAX_UNDO {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_checkpoint = Some(now);
    }

    pub fn undo(&mut self, current: &mut Project) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(current, previous));
        self.last_checkpoint = None;
        true
    }

    pub fn redo(&mut self, current: &mut Project) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(current, next));
        self.last_checkpoint = None;
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_checkpoint = None;
    }

    /// Ends the coalescing window without discarding undo history, used when a
    /// pointer drag finishes.
    pub fn break_coalescing(&mut self) {
        self.last_checkpoint = None;
    }

    #[cfg(test)]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_round_trip() {
        let mut history = History::default();
        let start = Instant::now();
        let original = Project::new("Original");
        // Snapshot before the edit, as push_undo does in the app.
        history.checkpoint_at(original.clone(), start);
        let mut project = original.clone();
        project.name = "Edited".into();
        assert!(history.can_undo());
        assert!(history.undo(&mut project));
        assert_eq!(project.name, "Original");
        assert!(history.can_redo());
        assert!(history.redo(&mut project));
        assert_eq!(project.name, "Edited");
        assert!(!history.can_redo());
    }

    #[test]
    fn rapid_changes_coalesce_into_one_checkpoint() {
        let mut history = History::default();
        let start = Instant::now();
        history.checkpoint_at(Project::new("A"), start);
        history.checkpoint_at(Project::new("B"), start + Duration::from_millis(100));
        assert_eq!(history.undo_len(), 1);
        let mut project = Project::new("C");
        assert!(history.undo(&mut project));
        assert_eq!(project.name, "A");
    }

    #[test]
    fn new_checkpoint_discards_redo() {
        let mut history = History::default();
        let start = Instant::now();
        history.checkpoint_at(Project::new("A"), start);
        let mut project = Project::new("B");
        assert!(history.undo(&mut project));
        assert!(history.can_redo());
        history.checkpoint_at(Project::new("C"), start + Duration::from_secs(1));
        assert!(!history.can_redo());
    }
}
