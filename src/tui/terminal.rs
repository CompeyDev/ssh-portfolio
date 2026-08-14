use std::default::Default;

#[cfg(feature = "blog")]
use ratatui_image::{picker::ProtocolType, FontSize};

/// Cell size in pixels. Fallback for when a probe fails.
///
/// NOTE: Not square because most terminals have a size ratio of roughly 1:2 (tested
/// against ghostty, kitty and konsole).
#[cfg(feature = "blog")]
pub const DEFAULT_FONT_SIZE: FontSize = (10, 20);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TerminalGeometry {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

impl TerminalGeometry {
    /// Calculates the cell size in pixels. `None` when the client reported a dimension of zero.
    pub fn font_size(&self) -> Option<(u16, u16)> {
        match (self.cols, self.rows, self.pixel_width, self.pixel_height) {
            (0, _, _, _) | (_, 0, _, _) | (_, _, 0, _) | (_, _, _, 0) => None, // RFC 4254 §6.2 allows zero
            (cols, rows, width, height) => Some((width / cols, height / rows)),
        }
    }
}

/// Graphics capabilities reported by the client
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsCapabilities {
    /// Supports image rendering using the kitty graphics protocol
    pub kitty: bool,
    /// Supports image rendering using the six pixel bitmap format
    pub sixel: bool,
    /// Supports image rendering using iTerm2's inline image protocol (OSC 1337)
    pub iterm2: bool,
}

#[derive(Debug, Default, Clone)]
pub struct TerminalInfo {
    #[cfg(feature = "blog")]
    font_size: Option<FontSize>,
    /// `Some` once the probe has finished, whether or not the client answered
    graphics: Option<GraphicsCapabilities>,
    /// The client's terminal name and version, if reported
    reported_name: Option<String>,
}

impl TerminalInfo {
    /// Get the font size, falling back to [`DEFAULT_FONT_SIZE`] if unreported by
    /// the client.
    ///
    /// See [`TerminalInfo::has_font_size`] to know whether the client reported its
    /// own cell size.
    #[cfg(feature = "blog")]
    pub fn font_size(&self) -> FontSize {
        self.font_size.unwrap_or(DEFAULT_FONT_SIZE)
    }

    /// Whether the client reported a real cell size.
    #[cfg(feature = "blog")]
    pub fn has_font_size(&self) -> bool {
        self.font_size.is_some()
    }

    /// Whether images can be rendered for this client.
    #[cfg(feature = "blog")]
    pub fn supports_images(&self) -> bool {
        match self.protocol() {
            ProtocolType::Halfblocks => true,
            _ => self.has_font_size(),
        }
    }

    /// Whether we are connected over a multipler session (currently only detects tmux).
    #[cfg(feature = "blog")]
    pub fn is_multiplexer(&self) -> bool {
        // TODO: screen, zellij, etc.
        self.leading_token().is_some_and(|token| token.eq_ignore_ascii_case("tmux"))
    }

    /// Splits and returns the "leading token", which is usually the name.
    ///
    /// The format is not guaranteed, and hence a space or left parenthesis separate is
    /// attempted for the split.
    pub fn leading_token(&self) -> Option<&str> {
        self.reported_name.as_deref().map(Self::leading_token_inner)
    }

    pub(super) fn leading_token_inner(reported_name: &str) -> &str {
        reported_name.split(['(', ' ']).next().unwrap_or(reported_name)
    }

    /// The graphics protocol to use for this client.
    #[cfg(feature = "blog")]
    pub fn protocol(&self) -> ProtocolType {
        if self.is_multiplexer() {
            // tmux returns information about whether it can handle an image type,
            // and not whether the underlying terminal can. We default to halfblocks
            // conservatively
            return ProtocolType::Halfblocks;
        }

        match self.graphics {
            Some(caps) if caps.iterm2 => ProtocolType::Iterm2,
            Some(caps) if caps.kitty => ProtocolType::Kitty,
            Some(caps) if caps.sixel => ProtocolType::Sixel,
            Some(_) => ProtocolType::Halfblocks, // probed, no support
            None => ProtocolType::Halfblocks,    // unprobed
        }
    }

    /// The graphics capabilities of the terminal, or `None` if the probe
    /// hasn't finalized.
    pub fn graphics(&self) -> Option<GraphicsCapabilities> {
        self.graphics
    }

    /// The full `XTVERSION` response, including the terminal's name and version,
    /// if any.
    pub fn reported_name(&self) -> Option<&str> {
        self.reported_name.as_deref()
    }

    /// Records the terminal's self-reported name, as returned by an `XTVERSION`
    /// query.
    pub fn set_reported_name(&mut self, name: String) {
        self.reported_name = Some(name);
    }

    /// Records the probe outcome.
    pub fn set_probed(&mut self, caps: GraphicsCapabilities) {
        self.graphics = Some(caps);
    }

    /// Sets the font size.
    #[cfg(feature = "blog")]
    pub fn set_font_size(&mut self, font_size: FontSize) {
        self.font_size = Some(font_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> TerminalInfo {
        let mut info = TerminalInfo::default();
        info.set_reported_name(name.to_string());
        info
    }

    #[test]
    fn the_leading_token_drops_the_version_whichever_separator_is_used() {
        assert_eq!(named("kitty(0.48.2)").leading_token(), Some("kitty"));
        assert_eq!(named("ghostty 1.3.1-arch2").leading_token(), Some("ghostty"));
        assert_eq!(named("Konsole 26.04.3").leading_token(), Some("Konsole"));
        assert_eq!(named("tmux 3.7b").leading_token(), Some("tmux"));
        assert_eq!(TerminalInfo::default().leading_token(), None);
    }

    #[cfg(feature = "blog")]
    #[test]
    fn pixel_protocols_need_a_cell_size() {
        let mut info = TerminalInfo::default();
        info.set_probed(GraphicsCapabilities { kitty: true, sixel: false, iterm2: false });
        assert!(!info.supports_images(), "no cell size yet");

        info.set_font_size((8, 17));
        assert!(info.supports_images());
    }

    #[cfg(feature = "blog")]
    #[test]
    fn halfblocks_needs_neither_a_capability_nor_a_cell_size() {
        let mut info = TerminalInfo::default();
        info.set_probed(GraphicsCapabilities { kitty: false, sixel: false, iterm2: false });

        assert_eq!(info.protocol(), ProtocolType::Halfblocks);
        assert!(!info.has_font_size(), "no cell size arrived");
        assert!(info.supports_images(), "halfblocks must not require a cell size");
    }

    #[cfg(feature = "blog")]
    #[test]
    fn a_probed_client_gets_what_it_reported() {
        let mut sixel = TerminalInfo::default();
        sixel.set_probed(GraphicsCapabilities { kitty: false, sixel: true, iterm2: false });
        assert_eq!(sixel.protocol(), ProtocolType::Sixel);

        // kitty should be preferred over sixel
        let mut both = TerminalInfo::default();
        both.set_probed(GraphicsCapabilities { kitty: true, sixel: true, iterm2: false });
        assert_eq!(both.protocol(), ProtocolType::Kitty);
    }

    #[cfg(feature = "blog")]
    #[test]
    fn iterm2_outranks_even_kitty() {
        let mut iterm = TerminalInfo::default();
        iterm.set_probed(GraphicsCapabilities { kitty: true, sixel: true, iterm2: true });
        iterm.set_font_size((7, 17));

        assert_eq!(iterm.protocol(), ProtocolType::Iterm2);
        assert!(iterm.supports_images());
    }

    #[test]
    fn the_leading_token_is_shared_by_name_derived_checks() {
        assert_eq!(TerminalInfo::leading_token_inner("iTerm2 3.6.9"), "iTerm2");
        assert_eq!(TerminalInfo::leading_token_inner("kitty(0.48.2)"), "kitty");
        assert_eq!(TerminalInfo::leading_token_inner("tmux 3.7b"), "tmux");
        assert_eq!(TerminalInfo::leading_token_inner("bare"), "bare");
    }

    #[cfg(feature = "blog")]
    #[test]
    fn an_unprobed_client_defaults_to_halfblocks() {
        assert_eq!(TerminalInfo::default().protocol(), ProtocolType::Halfblocks);
        assert_eq!(named("kitty(0.48.2)").protocol(), ProtocolType::Halfblocks);
    }

    #[cfg(feature = "blog")]
    #[test]
    fn a_multiplexer_is_pinned_to_halfblocks_whatever_it_claims() {
        let mut info = named("tmux 3.7b");
        info.set_probed(GraphicsCapabilities { kitty: true, sixel: true, iterm2: false });
        info.set_font_size((16, 32));

        assert!(info.is_multiplexer());
        assert_eq!(
            info.protocol(),
            ProtocolType::Halfblocks,
            "multiplexers should default to halfblocks"
        );
        assert!(info.supports_images(), "halfblocks should be supported everywhere");
    }

    #[cfg(feature = "blog")]
    #[test]
    fn multiplexer_detection_matches_the_whole_leading_token() {
        assert!(named("tmux 3.7b").is_multiplexer());
        assert!(!named("tmuxinator 1.0").is_multiplexer());
        assert!(!named("kitty(0.48.2)").is_multiplexer());
        assert!(!named("someterm 1.0-tmux").is_multiplexer());
        assert!(!TerminalInfo::default().is_multiplexer());
    }
}
