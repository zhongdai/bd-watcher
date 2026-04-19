pub mod computer;
pub mod tv;
pub mod widgets;

use ratatui::Frame;

use crate::app::{App, Mode};

pub fn render(app: &App, frame: &mut Frame) {
    match app.mode {
        Mode::Tv => tv::render(app, frame),
        Mode::Computer => computer::render(app, frame),
    }
}
