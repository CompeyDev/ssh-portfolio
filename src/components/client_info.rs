use std::sync::Arc;
use std::time::Instant;

use color_eyre::Result;
use ratatui::layout::Size;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use tokio::sync::RwLock;

#[cfg(feature = "blog")]
use ratatui_image::picker::ProtocolType;

use super::Component;
use crate::action::Action;
use crate::tui::terminal::TerminalInfo;

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

    /// Parses an `XTVERSION` response to get just the terminal name, for the rail.
    ///
    /// There is no standard format for the response, and it differs from terminal to
    /// terminal. Usually terminals place the version in parentheses after the name (e.g.
    /// `kitty(0.48.2)`, `foot(1.16.2)`), with notable exceptions being ghostty and
    /// konsole, which separate them by a space (`ghostty 1.3.1-arch2`, `Konsole 26.04.3`).
    ///
    /// The raw response to be provided to this function can be accessed by using
    /// [`TerminalInfo::reported_name`].
    fn short_name(reported: &str) -> String {
        const MAX: usize = 10;
        let name = reported
            .split(['(', ' '])
            .next()
            .unwrap_or(reported)
            .trim_matches(|c: char| !c.is_alphanumeric());

        // Technically the prober already rejects blank names, but just to be extra safe
        let name = match (name.is_empty(), reported.trim()) {
            (false, _) => name,
            (true, fallback) if !fallback.is_empty() => fallback,
            (true, _) => return "unknown".to_string(),
        };

        super::truncate(&name.to_lowercase(), MAX)
    }

    /// Fetches the terminal's label.
    /// 
    /// The process of probing the terminal is asynchronous and not handled by this method.
    /// A value based on the current state of the probe is given, and it may not know the
    /// label yet.
    fn terminal_label(&self) -> String {
        let Some(info) = self.terminal_info.as_ref().and_then(|i| i.try_read().ok()) else {
            return "unknown".to_string();
        };

        match (info.reported_name(), info.graphics()) {
            (Some(name), _) => Self::short_name(name),
            (None, Some(_)) => "unnamed".to_string(),
            (None, None) => "probing...".to_string(),
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

            if !info.supports_images() {
                return "none".to_string();
            }

            match info.protocol() {
                ProtocolType::Halfblocks => "blocks".to_string(),
                other => format!("{other:?}").to_lowercase(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_handles_both_reply_shapes() {
        // Parenthesised
        assert_eq!(ClientInfo::short_name("kitty(0.48.2)"), "kitty");
        assert_eq!(ClientInfo::short_name("foot(1.16.2)"), "foot");
        assert_eq!(ClientInfo::short_name("XTerm(390)"), "xterm");

        // Spaced
        assert_eq!(ClientInfo::short_name("ghostty 1.3.1-arch2"), "ghostty");
        assert_eq!(ClientInfo::short_name("Konsole 26.04.3"), "konsole");
        assert_eq!(ClientInfo::short_name("WezTerm 20240203-110809"), "wezterm");
        assert_eq!(ClientInfo::short_name("tmux 3.7b"), "tmux");
    }

    #[test]
    fn short_name_never_returns_empty_for_odd_input() {
        // Should never have a fully empty string to render
        assert!(!ClientInfo::short_name("(((").is_empty());
        assert!(!ClientInfo::short_name(" ").is_empty());
    }

    #[test]
    fn short_name_fits_the_rail() {
        use unicode_width::UnicodeWidthStr;

        for reply in [
            "kitty(0.32.2)",
            "SomeVeryLongTerminalName 1.2.3",
            "XTerm(390)",
            "日本語のターミナル",
        ] {
            assert!(
                ClientInfo::short_name(reply).width() <= 10,
                "{reply} must fit the ten columns the rail allows"
            );
        }
    }
}
