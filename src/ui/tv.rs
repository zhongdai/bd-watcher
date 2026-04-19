use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use crate::app::App;
use crate::ui::widgets;

pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    if area.width < 80 || area.height < 24 {
        widgets::too_small_placeholder(frame, &app.theme, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if app.last_error.is_some() { 5 } else { 4 }),
            Constraint::Percentage(55),
            Constraint::Min(6),
        ])
        .split(area);

    widgets::render_header(app, frame, chunks[0]);
    widgets::render_epics(app, frame, chunks[1], false);
    widgets::render_activity(app, frame, chunks[2]);
}
