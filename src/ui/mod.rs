pub mod computer;
pub mod widgets;

use ratatui::Frame;

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame) {
    computer::render(app, frame);
}
