use crate::model::AnalysisState;

/// Bounded undo/redo over analysis snapshots. Snapshots share unchanged tracks through `Arc`,
/// so each step costs roughly the size of the tracks it edited.
#[derive(Clone, Debug)]
pub struct History {
    state: AnalysisState,
    past: Vec<(AnalysisState, String)>,
    future: Vec<(AnalysisState, String)>,
    limit: usize,
    revision: u64,
}

impl History {
    pub fn new(state: AnalysisState) -> Self {
        Self { state, past: Vec::new(), future: Vec::new(), limit: 200, revision: 0 }
    }
    pub fn state(&self) -> &AnalysisState {
        &self.state
    }
    /// Increases on every change to the analysis, including undo and redo.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.past.last().map(|(_, l)| l.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.future.last().map(|(_, l)| l.as_str())
    }
    /// Applies an edit as one undo step. Returns false (and records nothing) if nothing changed.
    pub fn edit(&mut self, label: &str, change: impl FnOnce(&mut AnalysisState)) -> bool {
        let mut next = self.state.clone();
        change(&mut next);
        if next == self.state {
            return false;
        }
        let previous = std::mem::replace(&mut self.state, next);
        self.past.push((previous, label.to_string()));
        if self.past.len() > self.limit {
            self.past.remove(0);
        }
        self.future.clear();
        self.revision += 1;
        true
    }
    /// Amends the current step (used for coalesced tracking deliveries within one run).
    pub fn amend(&mut self, change: impl FnOnce(&mut AnalysisState)) -> bool {
        let mut next = self.state.clone();
        change(&mut next);
        if next == self.state {
            return false;
        }
        self.state = next;
        self.future.clear();
        self.revision += 1;
        true
    }
    pub fn undo(&mut self) -> Option<String> {
        let (state, label) = self.past.pop()?;
        let current = std::mem::replace(&mut self.state, state);
        self.future.push((current, label.clone()));
        self.revision += 1;
        Some(label)
    }
    pub fn redo(&mut self) -> Option<String> {
        let (state, label) = self.future.pop()?;
        let current = std::mem::replace(&mut self.state, state);
        self.past.push((current, label.clone()));
        self.revision += 1;
        Some(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Track;
    use std::sync::Arc;

    #[test]
    fn undo_redo_and_noop_edits() {
        let mut h = History::new(AnalysisState::new(100.0));
        assert!(!h.edit("nothing", |_| {}));
        assert_eq!(h.revision(), 0);
        h.edit("Add track", |s| s.tracks.push(Arc::new(Track::new("a", 0))));
        h.edit("Rename", |s| Arc::make_mut(&mut s.tracks[0]).name = "b".into());
        assert_eq!(h.undo().as_deref(), Some("Rename"));
        assert_eq!(h.state().tracks[0].name, "a");
        assert_eq!(h.redo().as_deref(), Some("Rename"));
        assert_eq!(h.state().tracks[0].name, "b");
        h.undo();
        h.edit("Other", |s| s.tracks.clear());
        assert!(!h.can_redo());
        assert_eq!(h.revision(), 6);
    }

    #[test]
    fn history_is_bounded() {
        let mut h = History::new(AnalysisState::new(100.0));
        h.limit = 3;
        for i in 0..10 {
            h.edit("add", |s| s.tracks.push(Arc::new(Track::new(format!("{i}"), i))));
        }
        let mut undone = 0;
        while h.undo().is_some() {
            undone += 1;
        }
        assert_eq!(undone, 3);
        assert_eq!(h.state().tracks.len(), 7);
    }
}
