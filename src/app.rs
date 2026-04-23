use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use chrono::{DateTime, Utc};

use crate::model::{ActivityEvent, Component, Issue, Snapshot};
use crate::theme::Theme;
use crate::ui::widgets;

pub const ACTIVITY_CAP: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Main,
    Filter,
    /// Centered modal showing full details of the sub-bead at
    /// `selected_sub`. Opened by Enter in focused-epic mode; closed by
    /// Enter or Esc.
    BeadDetail,
}

pub struct App {
    pub view: View,
    pub theme: Theme,
    pub repo: PathBuf,
    pub focus: Option<String>,
    pub interval_secs: u64,

    pub snapshot: Option<Snapshot>,
    pub activity: VecDeque<ActivityEvent>,
    pub selected_epic: usize,
    /// Index into the visual order of the focused epic's sub-beads
    /// (see `widgets::visual_sub_order`). Only meaningful when
    /// `focus.is_some()` and a snapshot with children has been loaded.
    pub selected_sub: usize,
    pub filter: String,
    pub last_error: Option<(DateTime<Utc>, String)>,
    /// Transient status message shown in the footer (e.g. "copied demo-abc").
    pub toast: Option<(Instant, String)>,
    /// First half of a vim-style `gg` chord. Cleared by any other key.
    pub pending_g: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new(theme: Theme, repo: PathBuf, focus: Option<String>, interval_secs: u64) -> Self {
        Self {
            view: View::Main,
            theme,
            repo,
            focus,
            interval_secs,
            snapshot: None,
            activity: VecDeque::with_capacity(ACTIVITY_CAP),
            selected_epic: 0,
            selected_sub: 0,
            filter: String::new(),
            last_error: None,
            toast: None,
            pending_g: false,
            should_quit: false,
        }
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((Instant::now(), msg.into()));
    }

    /// Returns the toast message if it's still within the display window.
    pub fn active_toast(&self) -> Option<&str> {
        self.toast.as_ref().and_then(|(at, msg)| {
            if at.elapsed() < std::time::Duration::from_secs(2) {
                Some(msg.as_str())
            } else {
                None
            }
        })
    }

    /// Returns the id of the currently-selected epic, or None if no snapshot
    /// or selection is out of range for the filtered list.
    pub fn selected_epic_id(&self) -> Option<String> {
        let snap = self.snapshot.as_ref()?;
        let filtered = self.filtered_epic_indices(snap);
        if filtered.contains(&self.selected_epic) {
            snap.components
                .get(self.selected_epic)
                .map(|c| c.root.id.clone())
        } else {
            None
        }
    }

    pub fn apply_snapshot(&mut self, snapshot: Snapshot, events: Vec<ActivityEvent>) {
        for event in events.into_iter() {
            if matches!(event, ActivityEvent::StatusChange { .. }) {
                self.push_activity(event);
            }
        }
        let epic_count = snapshot.components.len();
        self.snapshot = Some(snapshot);
        self.last_error = None;
        if epic_count > 0 {
            self.selected_epic = self.selected_epic.min(epic_count - 1);
        } else {
            self.selected_epic = 0;
        }
        // Clamp sub-bead selection to the new focused epic's child count.
        let sub_len = self.focused_sub_order_len();
        if sub_len == 0 {
            self.selected_sub = 0;
        } else if self.selected_sub >= sub_len {
            self.selected_sub = sub_len - 1;
        }
    }

    pub fn apply_error(&mut self, err: String) {
        self.last_error = Some((Utc::now(), err));
    }

    fn push_activity(&mut self, event: ActivityEvent) {
        if self.activity.len() >= ACTIVITY_CAP {
            self.activity.pop_front();
        }
        self.activity.push_back(event);
    }

    pub fn move_selection(&mut self, delta: isize) {
        let Some(snap) = &self.snapshot else { return };
        let filtered = self.filtered_epic_indices(snap);
        if filtered.is_empty() {
            return;
        }
        let current_pos = filtered
            .iter()
            .position(|&i| i == self.selected_epic)
            .unwrap_or(0);
        let last = filtered.len() as isize - 1;
        let new_pos = (current_pos as isize + delta).clamp(0, last) as usize;
        self.selected_epic = filtered[new_pos];
    }

    pub fn jump_to_top(&mut self) {
        let Some(snap) = &self.snapshot else { return };
        let filtered = self.filtered_epic_indices(snap);
        if let Some(&first) = filtered.first() {
            self.selected_epic = first;
        }
    }

    pub fn jump_to_bottom(&mut self) {
        let Some(snap) = &self.snapshot else { return };
        let filtered = self.filtered_epic_indices(snap);
        if let Some(&last) = filtered.last() {
            self.selected_epic = last;
        }
    }

    /// Returns the focused-epic component (first component, when a
    /// focus id is set and a snapshot has loaded). None otherwise.
    pub fn focused_component(&self) -> Option<&Component> {
        self.focus.as_ref()?;
        self.snapshot.as_ref()?.components.first()
    }

    /// Number of sub-beads in the focused epic, in visual order.
    /// Zero when there's no focused component.
    pub fn focused_sub_order_len(&self) -> usize {
        self.focused_component()
            .map(|c| widgets::visual_sub_order(c).len())
            .unwrap_or(0)
    }

    /// The currently-selected sub-bead, if focus mode is active and
    /// the selection index points at a real child.
    pub fn selected_sub_bead(&self) -> Option<&Issue> {
        let comp = self.focused_component()?;
        let order = widgets::visual_sub_order(comp);
        let &idx = order.get(self.selected_sub)?;
        comp.issues.get(idx)
    }

    pub fn move_sub_selection(&mut self, delta: isize) {
        let len = self.focused_sub_order_len();
        if len == 0 {
            return;
        }
        let last = len as isize - 1;
        self.selected_sub = (self.selected_sub as isize + delta).clamp(0, last) as usize;
    }

    pub fn jump_first_sub(&mut self) {
        if self.focused_sub_order_len() > 0 {
            self.selected_sub = 0;
        }
    }

    pub fn jump_last_sub(&mut self) {
        let len = self.focused_sub_order_len();
        if len > 0 {
            self.selected_sub = len - 1;
        }
    }

    pub fn filtered_epic_indices(&self, snap: &Snapshot) -> Vec<usize> {
        let q = self.filter.to_lowercase();
        snap.components
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                q.is_empty()
                    || c.root.id.to_lowercase().contains(&q)
                    || c.root.title.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Component, DepType, Dependency, Issue, Status};
    use crate::theme::{self, ThemeName};
    use std::path::PathBuf;

    fn issue(id: &str, status: Status, issue_type: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: id.to_string(),
            description: String::new(),
            status,
            priority: 0,
            issue_type: issue_type.to_string(),
            owner: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            external_ref: None,
        }
    }

    fn app_with_focused_epic(children: &[&str]) -> App {
        let root = issue("ep", Status::Open, "epic");
        let mut issues = vec![root.clone()];
        for id in children {
            issues.push(issue(id, Status::Open, "task"));
        }
        // Simple chain of dependencies so compute_layers sees the
        // children in Layer 0, Layer 1, ... matching the given order.
        let mut deps: Vec<Dependency> = Vec::new();
        for window in children.windows(2) {
            deps.push(Dependency {
                issue_id: window[1].to_string(),
                depends_on_id: window[0].to_string(),
                dep_type: DepType::Blocks,
            });
        }
        let snap = Snapshot {
            components: vec![Component {
                root,
                issues,
                dependencies: deps,
            }],
            fetched_at: Utc::now(),
        };
        let mut app = App::new(
            theme::resolve(Some(ThemeName::Default), None),
            PathBuf::from("/tmp"),
            Some("ep".to_string()),
            5,
        );
        app.apply_snapshot(snap, Vec::new());
        app
    }

    #[test]
    fn move_sub_selection_clamps_at_both_ends() {
        let mut app = app_with_focused_epic(&["ep.1", "ep.2", "ep.3"]);
        assert_eq!(app.selected_sub, 0);

        app.move_sub_selection(-1); // can't go below 0
        assert_eq!(app.selected_sub, 0);

        app.move_sub_selection(1);
        assert_eq!(app.selected_sub, 1);
        app.move_sub_selection(1);
        assert_eq!(app.selected_sub, 2);

        app.move_sub_selection(1); // can't go past last
        assert_eq!(app.selected_sub, 2);
    }

    #[test]
    fn jump_first_and_last_sub_work() {
        let mut app = app_with_focused_epic(&["ep.1", "ep.2", "ep.3"]);
        app.jump_last_sub();
        assert_eq!(app.selected_sub, 2);
        app.jump_first_sub();
        assert_eq!(app.selected_sub, 0);
    }

    #[test]
    fn selected_sub_bead_returns_correct_issue() {
        let mut app = app_with_focused_epic(&["ep.a", "ep.b"]);
        app.jump_last_sub();
        let sel = app.selected_sub_bead().expect("has selection");
        assert_eq!(sel.id, "ep.b");
    }

    #[test]
    fn apply_snapshot_clamps_out_of_range_selection() {
        let mut app = app_with_focused_epic(&["ep.1", "ep.2", "ep.3"]);
        app.jump_last_sub();
        assert_eq!(app.selected_sub, 2);

        // New snapshot with only one child — selection must clamp.
        let root = issue("ep", Status::Open, "epic");
        let snap = Snapshot {
            components: vec![Component {
                root: root.clone(),
                issues: vec![root, issue("ep.1", Status::Open, "task")],
                dependencies: Vec::new(),
            }],
            fetched_at: Utc::now(),
        };
        app.apply_snapshot(snap, Vec::new());
        assert_eq!(app.selected_sub, 0);
    }

    #[test]
    fn move_sub_selection_noop_when_no_focus() {
        let mut app = App::new(
            theme::resolve(Some(ThemeName::Default), None),
            PathBuf::from("/tmp"),
            None,
            5,
        );
        app.move_sub_selection(1);
        assert_eq!(app.selected_sub, 0);
    }
}
