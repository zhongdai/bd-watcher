use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Open,
    InProgress,
    Blocked,
    Closed,
    Deferred,
}

impl Status {
    pub fn icon(self) -> &'static str {
        match self {
            Status::Open => "○",
            Status::InProgress => "◐",
            Status::Blocked => "●",
            Status::Closed => "✓",
            Status::Deferred => "❄",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in_progress",
            Status::Blocked => "blocked",
            Status::Closed => "closed",
            Status::Deferred => "deferred",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Issue {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub status: Status,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub issue_type: String,
    #[serde(default)]
    pub owner: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub external_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DepType {
    ParentChild,
    Blocks,
    Related,
    Discovered,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dependency {
    pub issue_id: String,
    pub depends_on_id: String,
    #[serde(rename = "type")]
    pub dep_type: DepType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Component {
    #[serde(rename = "Root")]
    pub root: Issue,
    #[serde(rename = "Issues", default, deserialize_with = "null_to_default")]
    pub issues: Vec<Issue>,
    #[serde(rename = "Dependencies", default, deserialize_with = "null_to_default")]
    pub dependencies: Vec<Dependency>,
}

fn null_to_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    let opt = Option::<T>::deserialize(d)?;
    Ok(opt.unwrap_or_default())
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub components: Vec<Component>,
    pub fetched_at: DateTime<Utc>,
}

impl Snapshot {
    pub fn all_issues(&self) -> impl Iterator<Item = &Issue> {
        self.components.iter().flat_map(|c| c.issues.iter())
    }

    pub fn total_counts(&self) -> StatusCounts {
        let mut counts = StatusCounts::default();
        let mut seen = std::collections::HashSet::new();
        for issue in self.all_issues() {
            if seen.insert(&issue.id) {
                counts.add(issue.status);
            }
        }
        counts
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StatusCounts {
    pub open: usize,
    pub in_progress: usize,
    pub blocked: usize,
    pub closed: usize,
    pub deferred: usize,
}

impl StatusCounts {
    pub fn add(&mut self, s: Status) {
        match s {
            Status::Open => self.open += 1,
            Status::InProgress => self.in_progress += 1,
            Status::Blocked => self.blocked += 1,
            Status::Closed => self.closed += 1,
            Status::Deferred => self.deferred += 1,
        }
    }

    pub fn total(&self) -> usize {
        self.open + self.in_progress + self.blocked + self.closed + self.deferred
    }

    pub fn done_fraction(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.closed as f64 / total as f64
        }
    }
}

#[derive(Debug, Clone)]
pub enum ActivityEvent {
    StatusChange {
        id: String,
        title: String,
        from: Status,
        to: Status,
        at: DateTime<Utc>,
    },
    Added {
        id: String,
        title: String,
        status: Status,
        at: DateTime<Utc>,
    },
    Removed {
        id: String,
        at: DateTime<Utc>,
    },
}

impl ActivityEvent {
    pub fn at(&self) -> DateTime<Utc> {
        match self {
            ActivityEvent::StatusChange { at, .. }
            | ActivityEvent::Added { at, .. }
            | ActivityEvent::Removed { at, .. } => *at,
        }
    }
}
