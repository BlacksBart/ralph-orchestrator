use crate::state::TuiState;
use ratatui::{
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::time::Duration;

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

pub fn render(state: &TuiState) -> Paragraph<'static> {
    let hat_display = state.get_pending_hat_display();
    let iteration = state.iteration + 1;
    let loop_time = state
        .get_loop_elapsed()
        .map(format_duration)
        .unwrap_or_else(|| "—".to_string());
    let iter_time = state
        .get_iteration_elapsed()
        .map(format_duration)
        .unwrap_or_else(|| "—".to_string());

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("    Next Hat:      "),
            Span::raw(hat_display),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("    Iteration:     "),
            Span::raw(iteration.to_string()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("    Loop Time:     "),
            Span::raw(loop_time),
        ]),
        Line::from(vec![
            Span::raw("    This Iteration: "),
            Span::raw(iter_time),
        ]),
        Line::from(""),
    ];

    Paragraph::new(lines).block(Block::default().borders(Borders::ALL))
}
