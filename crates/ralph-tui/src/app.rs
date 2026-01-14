//! Main application loop for the TUI.

use crate::state::TuiState;
use crate::widgets::{footer, header, status};
use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    Terminal,
};
use std::io;
use std::sync::{Arc, Mutex};
use tokio::time::{interval, Duration};

/// Main TUI application.
pub struct App {
    state: Arc<Mutex<TuiState>>,
}

impl App {
    /// Creates a new App with shared state.
    pub fn new(state: Arc<Mutex<TuiState>>) -> Self {
        Self { state }
    }

    /// Runs the TUI event loop.
    pub async fn run(&self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut tick = interval(Duration::from_millis(100));

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let state = self.state.lock().unwrap();
                    terminal.draw(|f| {
                        let chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Length(3),
                                Constraint::Min(0),
                                Constraint::Length(3),
                            ])
                            .split(f.area());

                        f.render_widget(header::render(&state), chunks[0]);
                        f.render_widget(status::render(&state), chunks[1]);
                        f.render_widget(footer::render(&state), chunks[2]);
                    })?;
                }
                _ = tokio::signal::ctrl_c() => {
                    break;
                }
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

        Ok(())
    }
}
