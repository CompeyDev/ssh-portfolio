use std::sync::Arc;
use std::time::Instant;

use color_eyre::Result;
use ratatui::layout::Size;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tokio::sync::RwLock;

use super::Component;
use crate::action::Action;
use crate::tui::terminal::{TerminalInfo, TerminalKind, UnsupportedReason};

pub const PANEL_WIDTH: u16 = 21;

pub struct ClientInfo {
    terminal_info: Option<Arc<RwLock<TerminalInfo>>>,
    connected_at: Instant,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self { terminal_info: None, connected_at: Instant::now() }
    }
}

impl ClientInfo {
    pub fn new() -> Self {
        Self::default()
    }

    fn uptime(&self) -> String {
        let secs = self.connected_at.elapsed().as_secs();
        match (secs / 3600, (secs % 3600) / 60, secs % 60) {
            (0, 0, s) => format!("{s}s"),
            (0, m, s) => format!("{m}m {s}s"),
            (h, m, _) => format!("{h}h {m}m"),
        }
    }

    fn terminal_label(&self) -> String {
        let Some(info) = self.terminal_info.as_ref().and_then(|i| i.try_read().ok()) else {
            return "unknown".to_string();
        };

        match info.kind() {
            TerminalKind::Unsupported(UnsupportedReason::Unprobed) => "probing...".into(),
            TerminalKind::Unsupported(UnsupportedReason::Unsized) => "no pixel size".into(),
            TerminalKind::Unsupported(UnsupportedReason::Unknown) => "unknown".into(),
            known => known.to_string(),
        }
    }

    /// Describe the image protocol in use for this client.
    fn image_label(&self) -> String {
        #[cfg(feature = "blog")]
        {
            let Some(info) = self.terminal_info.as_ref().and_then(|i| i.try_read().ok())
            else {
                return "...".to_string();
            };

            match info.kind() {
                TerminalKind::Unsupported(_) => "none".to_string(),
                // `ProtocolType` is `Debug` but not `Display`
                known => format!("{:?}", known.as_protocol()).to_lowercase(),
            }
        }

        #[cfg(not(feature = "blog"))]
        "disabled".to_string()
    }

    fn rows(&self, term_size: Size) -> Vec<Line<'static>> {
        let key = Style::default().fg(Color::DarkGray);
        let val = Style::default().fg(Color::White);

        let row = |label: &'static str, value: String| {
            Line::from(vec![
                Span::styled(format!(" {label:<8}"), key),
                Span::styled(value, val),
            ])
        };

        vec![
            row("term", self.terminal_label()),
            row("cells", format!("{}x{}", term_size.width, term_size.height)),
            row("images", self.image_label()),
            row("session", self.uptime()),
        ]
    }
}

impl Component for ClientInfo {
    fn init(&mut self, term_info: Arc<RwLock<TerminalInfo>>, _: Size) -> Result<()> {
        self.terminal_info = Some(term_info);
        self.connected_at = Instant::now();
        Ok(())
    }

    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            _ => {}
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let term_size = frame.area().as_size();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                " you ",
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(self.rows(term_size)), inner);

        Ok(())
    }
}
