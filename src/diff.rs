use std::collections::HashMap;

use crate::model::{ActivityEvent, Issue, Snapshot};

pub fn diff(prev: Option<&Snapshot>, next: &Snapshot) -> Vec<ActivityEvent> {
    let Some(prev) = prev else {
        return Vec::new();
    };

    let prev_map: HashMap<&str, &Issue> = prev.all_issues().map(|i| (i.id.as_str(), i)).collect();
    let next_map: HashMap<&str, &Issue> = next.all_issues().map(|i| (i.id.as_str(), i)).collect();

    let mut events = Vec::new();
    let at = next.fetched_at;

    for (id, issue) in &next_map {
        match prev_map.get(id) {
            None => events.push(ActivityEvent::Added {
                id: issue.id.clone(),
                title: issue.title.clone(),
                status: issue.status,
                at,
            }),
            Some(prev_issue) if prev_issue.status != issue.status => {
                events.push(ActivityEvent::StatusChange {
                    id: issue.id.clone(),
                    title: issue.title.clone(),
                    from: prev_issue.status,
                    to: issue.status,
                    at,
                });
            }
            _ => {}
        }
    }

    for (id, issue) in &prev_map {
        if !next_map.contains_key(id) {
            events.push(ActivityEvent::Removed {
                id: issue.id.clone(),
                at,
            });
        }
    }

    events
}
