use bd_watcher::model::{ActivityEvent, Component, Issue, Snapshot, Status};
use chrono::{TimeZone, Utc};
use pretty_assertions::assert_eq;

fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

fn issue(id: &str, status: Status) -> Issue {
    Issue {
        id: id.into(),
        title: format!("issue {id}"),
        description: String::new(),
        status,
        priority: 2,
        issue_type: "task".into(),
        owner: None,
        created_at: ts(0),
        updated_at: ts(0),
        external_ref: None,
    }
}

fn snap(at: i64, issues: Vec<Issue>) -> Snapshot {
    let root = issues
        .first()
        .cloned()
        .unwrap_or_else(|| issue("root", Status::Open));
    Snapshot {
        fetched_at: ts(at),
        components: vec![Component {
            root,
            issues,
            dependencies: vec![],
        }],
    }
}

#[test]
fn diff_without_prev_returns_empty() {
    let next = snap(100, vec![issue("a", Status::Open)]);
    let events = bd_watcher::diff::diff(None, &next);
    assert_eq!(events.len(), 0);
}

#[test]
fn diff_detects_status_change() {
    let prev = snap(100, vec![issue("a", Status::Open)]);
    let next = snap(200, vec![issue("a", Status::InProgress)]);
    let events = bd_watcher::diff::diff(Some(&prev), &next);
    assert_eq!(events.len(), 1);
    match &events[0] {
        ActivityEvent::StatusChange {
            id, from, to, at, ..
        } => {
            assert_eq!(id, "a");
            assert_eq!(*from, Status::Open);
            assert_eq!(*to, Status::InProgress);
            assert_eq!(*at, ts(200));
        }
        other => panic!("expected StatusChange, got {other:?}"),
    }
}

#[test]
fn diff_detects_added() {
    let prev = snap(100, vec![issue("a", Status::Open)]);
    let next = snap(
        200,
        vec![issue("a", Status::Open), issue("b", Status::Open)],
    );
    let events = bd_watcher::diff::diff(Some(&prev), &next);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        ActivityEvent::Added { id, .. } if id == "b"
    ));
}

#[test]
fn diff_detects_removed() {
    let prev = snap(
        100,
        vec![issue("a", Status::Open), issue("b", Status::Open)],
    );
    let next = snap(200, vec![issue("a", Status::Open)]);
    let events = bd_watcher::diff::diff(Some(&prev), &next);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        ActivityEvent::Removed { id, .. } if id == "b"
    ));
}

#[test]
fn diff_ignores_unchanged() {
    let prev = snap(
        100,
        vec![issue("a", Status::Open), issue("b", Status::Closed)],
    );
    let next = snap(
        200,
        vec![issue("a", Status::Open), issue("b", Status::Closed)],
    );
    let events = bd_watcher::diff::diff(Some(&prev), &next);
    assert_eq!(events.len(), 0);
}
