use crate::state::TuiState;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub fn render(state: &TuiState) -> Paragraph<'static> {
    let last_event = state
        .last_event
        .as_ref()
        .map(|e| format!("Last: {}", e))
        .unwrap_or_else(|| "Last: —".to_string());

    let indicator = if state.pending_hat.is_none() {
        Span::styled("■ done", Style::default().fg(Color::Blue))
    } else if state.is_active() {
        Span::styled("◉ active", Style::default().fg(Color::Green))
    } else {
        Span::styled(
            "◯ idle",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
        )
    };

    let line = Line::from(vec![
        Span::raw(" "),
        Span::raw(last_event),
        Span::raw("                              "),
        indicator,
        Span::raw(" "),
    ]);

    Paragraph::new(line).block(Block::default().borders(Borders::ALL))
}
