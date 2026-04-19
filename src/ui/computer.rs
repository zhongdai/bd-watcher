use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use crate::app::{App, View};
use crate::ui::widgets;

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    if area.width < 80 || area.height < 24 {
        widgets::too_small_placeholder(frame, &app.theme, area);
        return;
    }

    if app.view == View::Detail {
        widgets::render_detail(app, frame, area);
        let footer = ratatui::layout::Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        widgets::render_footer(app, frame, footer, "esc back · q quit");
        return;
    }

    let has_filter = app.view == View::Filter || !app.filter.is_empty();
    let mut constraints = vec![
        Constraint::Length(if app.last_error.is_some() { 5 } else { 4 }),
        Constraint::Percentage(55),
        Constraint::Min(6),
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
    widgets::render_epics(app, frame, chunks[i], true);
    i += 1;
    widgets::render_activity(app, frame, chunks[i]);
    i += 1;
    let hint = match app.view {
        View::Filter => "type to filter · esc cancel · enter accept",
        _ => "q quit · r refresh · ↑↓ select · ↵ detail · / filter",
    };
    widgets::render_footer(app, frame, chunks[i], hint);
}
