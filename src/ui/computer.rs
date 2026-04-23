use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use crate::app::{App, View};
use crate::ui::widgets;

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    if area.width < 80 || area.height < 24 {
        widgets::too_small_placeholder(frame, &app.theme, area);
        return;
    }

    if app.focus.is_some() {
        render_single_epic(app, frame);
    } else {
        render_all_epics(app, frame);
    }
}

fn render_single_epic(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if app.last_error.is_some() { 5 } else { 4 }),
            Constraint::Min(12),
            Constraint::Length(9),
            Constraint::Length(1),
        ])
        .split(area);

    widgets::render_header(app, frame, chunks[0]);
    widgets::render_single_epic_dag(app, frame, chunks[1]);
    widgets::render_activity(app, frame, chunks[2]);

    let hint_owned;
    let hint: &str = if let Some(t) = app.active_toast() {
        t
    } else {
        hint_owned = match app.view {
            View::BeadDetail => "↑↓ jk scroll · enter/esc close".to_string(),
            _ => "q quit · tab switch · ↑↓ jk move · enter detail · v open PR · y copy id"
                .to_string(),
        };
        hint_owned.as_str()
    };
    widgets::render_footer(app, frame, chunks[3], hint);

    // Popup overlay on top of everything else when toggled on.
    if app.view == View::BeadDetail {
        widgets::render_bead_detail_popup(app, frame);
    }
}

fn render_all_epics(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let has_filter = app.view == View::Filter || !app.filter.is_empty();
    let mut constraints = vec![
        Constraint::Length(if app.last_error.is_some() { 5 } else { 4 }),
        Constraint::Min(12),
        Constraint::Length(9),
        Constraint::Length(1),
    ];
    if has_filter {
        constraints.insert(1, Constraint::Length(1));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut i = 0;
    widgets::render_header(app, frame, chunks[i]);
    i += 1;
    if has_filter {
        widgets::render_filter(app, frame, chunks[i]);
        i += 1;
    }

    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[i]);
    widgets::render_epics(app, frame, middle[0], true);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(middle[1]);
    widgets::render_detail_header(app, frame, right[0]);
    widgets::render_detail_children(app, frame, right[1]);
    i += 1;

    widgets::render_activity(app, frame, chunks[i]);
    i += 1;

    let hint_owned;
    let hint: &str = if let Some(t) = app.active_toast() {
        t
    } else {
        hint_owned = match app.view {
            View::Filter => "type to filter · esc cancel · enter accept".to_string(),
            _ => "q quit · r refresh · ↑↓ jk select · gg/G top/bottom · y copy id · / filter"
                .to_string(),
        };
        hint_owned.as_str()
    };
    widgets::render_footer(app, frame, chunks[i], hint);
}
