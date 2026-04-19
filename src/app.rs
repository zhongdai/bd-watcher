use std::collections::VecDeque;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

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
    Detail,
    Filter,
}

pub struct App {
    pub mode: Mode,
    pub view: View,
    pub theme: Theme,
    pub repo: PathBuf,
    pub focus: Option<String>,
    pub interval_secs: u64,

    pub snapshot: Option<Snapshot>,
    pub activity: VecDeque<ActivityEvent>,
    pub selected_epic: usize,
    pub filter: String,
    pub last_error: Option<(DateTime<Utc>, String)>,
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
            snapshot: None,
            activity: VecDeque::with_capacity(ACTIVITY_CAP),
            selected_epic: 0,
            filter: String::new(),
            last_error: None,
            should_quit: false,
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
        let new_pos = (current_pos as isize + delta).rem_euclid(filtered.len() as isize) as usize;
        self.selected_epic = filtered[new_pos];
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
