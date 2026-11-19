//! Linux-side ratatui terminal UI matching `docs/DESIGN.md §7` (T-M3-04).

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ekran_core::{DeviceInfo, Result, SinkKind, Stats};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::{Frame, Terminal};
use std::io;
use std::time::Duration;

/// Operator action triggered from the TUI keyboard shortcuts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    ShareSelected(usize),
    TogglePause,
    ToggleMute,
    Rescan,
    Quit,
}

/// Live snapshot rendered by the terminal UI.
#[derive(Debug, Clone)]
pub struct TuiState {
    pub version: String,
    pub devices: Vec<DeviceInfo>,
    pub selected: usize,
    pub browser_url: String,
    pub stats: Stats,
    pub status_line: String,
    pub active_device_id: Option<String>,
    pub audio_muted: bool,
    pub remote_mode: bool,
}

impl Default for TuiState {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").into(),
            devices: Vec::new(),
            selected: 0,
            browser_url: "http://127.0.0.1:7700".into(),
            stats: Stats::default(),
            status_line: "Hazır".into(),
            active_device_id: None,
            audio_muted: false,
            remote_mode: false,
        }
    }
}

impl TuiState {
    pub fn move_up(&mut self) {
        let total = self.devices.len() + 1;
        if total > 0 {
            self.selected = (self.selected + total - 1) % total;
        }
    }

    pub fn move_down(&mut self) {
        let total = self.devices.len() + 1;
        if total > 0 {
            self.selected = (self.selected + 1) % total;
        }
    }
}

/// Render a single frame of the TUI following `docs/DESIGN.md §7` (80x24 and compact fallback).
pub fn render_frame(f: &mut Frame<'_>, state: &TuiState) {
    let area = f.area();
    let compact = area.height < 24 || area.width < 80;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(if compact { 5 } else { 8 }),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    // Header
    let mbps = (state.stats.bitrate_kbps as f64) / 1000.0;
    let header_title = format!(
        " ekran {}  |  yayın: {} · {:.1} Mb/s  |  {} ",
        state.version,
        if state.stats.quality.is_empty() {
            "auto"
        } else {
            &state.stats.quality
        },
        mbps,
        state.status_line
    );
    let header = Paragraph::new(Line::from(vec![Span::styled(
        header_title,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().bg(Color::Black)),
    );
    f.render_widget(header, chunks[0]);

    // Device list (+ always-available Browser row)
    let mut items: Vec<ListItem<'_>> = state
        .devices
        .iter()
        .map(|d| {
            let is_active = state.active_device_id.as_deref() == Some(d.id.as_str());
            let (dot, dot_color, label) = match d.kind {
                SinkKind::Cast | SinkKind::Dlna => {
                    if is_active {
                        ("●", Color::Blue, "yayında")
                    } else {
                        ("●", Color::Green, "uygun")
                    }
                }
                SinkKind::Miracast | SinkKind::Airplay => ("○", Color::DarkGray, "destek yok"),
                SinkKind::Browser => ("●", Color::Green, "tarayıcı"),
            };
            let text_style = if matches!(d.kind, SinkKind::Miracast | SinkKind::Airplay) {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:<28} ", d.name), text_style),
                Span::styled(
                    format!("{:<20} ", format!("{}:{}", d.address, d.port)),
                    Style::default().fg(Color::Gray),
                ),
                Span::styled(format!("{dot} {label}"), Style::default().fg(dot_color)),
            ]))
        })
        .collect();

    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            format!(" {:<28} ", "Tarayıcı modu (MSE)"),
            Style::default().fg(Color::White),
        ),
        Span::styled(state.browser_url.clone(), Style::default().fg(Color::Cyan)),
    ])));

    let list_title = format!(" cihazlar ({}) ", state.devices.len() + 1);
    let list = List::new(items)
        .block(
            Block::default()
                .title(list_title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .style(Style::default().bg(Color::Black)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(10, 132, 255))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected.min(state.devices.len())));
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    // Stats bar
    let pause_tag = if state.stats.paused {
        " · DURAKLATILDI"
    } else {
        ""
    };
    let idle_tag = if state.stats.idle { " · BOŞTA" } else { "" };
    let mute_tag = if state.audio_muted || !state.stats.audio {
        "sessiz"
    } else {
        "ses açık"
    };
    let stats_line = format!(
        " istatistik: {:.0} fps · {} istemci · gecikme {} ms · {}{}{} ",
        state.stats.fps, state.stats.clients, state.stats.latency_ms, mute_tag, pause_tag, idle_tag
    );
    let stats_widget = Paragraph::new(stats_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().bg(Color::Black).fg(Color::Gray)),
    );
    f.render_widget(stats_widget, chunks[2]);

    // Footer help bar
    let footer_txt = if state.remote_mode {
        " Kumanda modu etkin — TUI tuşları duraklatıldı "
    } else {
        " ↑↓ seç  ⏎ paylaş  p duraklat  m ses  r tara  q çık "
    };
    let footer = Paragraph::new(footer_txt).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().bg(Color::Black).fg(Color::Gray)),
    );
    f.render_widget(footer, chunks[3]);
}

/// Interactive terminal session guard restoring raw mode & alternate screen on drop.
pub struct TuiSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TuiSession {
    pub fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    pub fn draw(&mut self, state: &TuiState) -> Result<()> {
        self.terminal.draw(|f| render_frame(f, state))?;
        Ok(())
    }

    /// Poll for a single key event up to `timeout` and translate it to a [`TuiAction`].
    pub fn poll_action(
        &mut self,
        state: &mut TuiState,
        timeout: Duration,
    ) -> Result<Option<TuiAction>> {
        if state.remote_mode {
            return Ok(None);
        }
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    return Ok(None);
                }
                return Ok(match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => Some(TuiAction::Quit),
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.move_up();
                        None
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.move_down();
                        None
                    }
                    KeyCode::Enter => Some(TuiAction::ShareSelected(state.selected)),
                    KeyCode::Char('p') => Some(TuiAction::TogglePause),
                    KeyCode::Char('m') => Some(TuiAction::ToggleMute),
                    KeyCode::Char('r') => Some(TuiAction::Rescan),
                    _ => None,
                });
            }
        }
        Ok(None)
    }
}

impl Drop for TuiSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ekran_core::Capabilities;
    use ratatui::backend::TestBackend;

    #[test]
    fn renders_at_80x24_and_compact() {
        let mut state = TuiState::default();
        state.devices.push(DeviceInfo {
            id: "cast-1".into(),
            name: "LG OLED (10.9.46.60)".into(),
            kind: SinkKind::Cast,
            address: "10.9.46.60".into(),
            port: 8009,
            model: "OLED".into(),
            capabilities: Capabilities::default(),
            note: None,
        });
        state.devices.push(DeviceInfo {
            id: "mira-1".into(),
            name: "Fire TV".into(),
            kind: SinkKind::Miracast,
            address: "10.9.46.62".into(),
            port: 7231,
            model: "Fire".into(),
            capabilities: Capabilities::default(),
            note: Some("tarayıcı gerekli".into()),
        });

        for (w, h) in [(80, 24), (60, 14)] {
            let backend = TestBackend::new(w, h);
            let mut term = Terminal::new(backend).unwrap();
            term.draw(|f| render_frame(f, &state)).unwrap();
            let buf = term.backend().buffer().clone();
            let all_text: String = buf.content().iter().map(|c| c.symbol()).collect();
            assert!(all_text.contains("istatistik:"));
            assert!(all_text.contains("Tarayıcı modu"));
        }
    }
}
