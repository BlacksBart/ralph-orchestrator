use crate::state::TuiState;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render(state: &TuiState) -> Paragraph<'static> {
    let status = if state.pending_hat.is_some() {
        Span::styled("[LIVE]", Style::default().fg(Color::Green))
    } else {
        Span::styled("[DONE]", Style::default().fg(Color::Blue))
    };

    let line = Line::from(vec![
        Span::raw("🎩 RALPH ORCHESTRATOR"),
        Span::raw("                          "),
        status,
    ]);

    Paragraph::new(line).block(Block::default().borders(Borders::ALL))
}
