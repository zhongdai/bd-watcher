use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::Instant;

use chrono::{DateTime, Utc};

use crate::gh::GhRepo;
use crate::model::{ActivityEvent, Snapshot};
use crate::theme::Theme;

pub const ACTIVITY_CAP: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Tv,
    Computer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Main,
    Filter,
}

pub struct App {
    pub mode: Mode,
    pub view: View,
    pub theme: Theme,
    pub repo: PathBuf,
    pub focus: Option<String>,
    pub interval_secs: u64,
    /// GitHub owner+repo for the local checkout, used to render
    /// clickable PR hyperlinks. `None` when the repo isn't on github.com
    /// or `origin` isn't configured.
    pub gh_repo: Option<GhRepo>,

    pub snapshot: Option<Snapshot>,
    pub activity: VecDeque<ActivityEvent>,
    /// Time we last observed a status change for a given issue id.
    /// Drives TV-mode epic ordering so epics with recent activity float up.
    pub last_status_change: HashMap<String, DateTime<Utc>>,
    pub selected_epic: usize,
    pub filter: String,
    pub last_error: Option<(DateTime<Utc>, String)>,
    /// Transient status message shown in the footer (e.g. "copied demo-abc").
    pub toast: Option<(Instant, String)>,
    /// First half of a vim-style `gg` chord. Cleared by any other key.
    pub pending_g: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new(
        mode: Mode,
        theme: Theme,
        repo: PathBuf,
        focus: Option<String>,
        interval_secs: u64,
    ) -> Self {
        Self {
            mode,
            view: View::Main,
            theme,
            repo,
            focus,
            interval_secs,
            gh_repo: None,
            snapshot: None,
            activity: VecDeque::with_capacity(ACTIVITY_CAP),
            last_status_change: HashMap::new(),
            selected_epic: 0,
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
            if let ActivityEvent::StatusChange { id, at, .. } = &event {
                self.last_status_change.insert(id.clone(), *at);
            }
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
