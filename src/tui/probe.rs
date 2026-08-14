use super::terminal::{GraphicsCapabilities, TerminalInfo};

/// Device Status Report.
///
/// Guaranteed to be implemented by a capable terminal, and acts as a "fence"
/// if provided as the last command; after a wait, we can check if there was
/// a response for this, meaning the previous commands have been executed.
const FENCE: &str = "\x1b[5n";

/// Kitty graphics support.
///
/// Response is formatted as `APC _Gi=31;OK ST`, or no response is provided if
/// unsupported by the terminal.
const KITTY_GFX: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";

/// Primary Device Attributes. Attribute 4 set in response to indicate sixel.
const DA1: &str = "\x1b[c";

/// Cell size in pixels. Response looks like: `CSI 6 ; height ; width t`.
const CELL_SIZE: &str = "\x1b[16t";

/// Text area size in pixels. Response looks like `CSI 4 ; height ; width t`.
///
/// Text area size divided by grid size (which get during the pty request)
/// gives us the cell size. This is useful for terminals such as iTerm2,
/// which may not implement [`CELL_SIZE`] directly.
const TEXT_AREA: &str = "\x1b[14t";

/// iTerm2's proprietary cell size report.
///
/// Response looks like `OSC 1337;ReportCellSize=<height>;<width>;<scale> ST`,
/// in points rather than pixels, with a scale factor, which is usually 2 on
/// retina displays.
///
/// It is sent terminated, as documented.
const ITERM_CELL_SIZE: &str = "\x1b]1337;ReportCellSize\x1b\\";

/// xterm version. Returns the name and version of the terminal.
///
/// Response looks like: `DCS > | text ST`.
const XTVERSION: &str = "\x1b[>q";

/// Maximum size limit on the on the accumulator buffer. The largest reply we
/// require is ~40 bytes.
const MAX_RUN: usize = 128;

/// Escape-sequence framing.
///
/// We parse a constant stream of all data being returned over the channel,
/// some of which are user keystroke inputs. Therefore, we frame sequences
/// ourselves and interpet only the complete ones, and then replay the rest
/// which do not match.
#[derive(Debug, Default, PartialEq, Eq)]
enum Frame {
    #[default]
    Ground,
    /// Got `ESC` but waiting to see what kind of sequence this is
    Esc,
    /// Inside `ESC [ ... final`, terminated by a byte in `0x40..=0x7E`
    Csi,
    /// Inside `ESC _ ... ESC \` or `ESC P ... ESC \`
    Apc,
    /// Got `ESC` while inside an APC / DCS. Anything after `\` is a literal keystroke
    ApcEsc,
    /// Inside `ESC ] ... BEL` or `ESC ] ... ESC \`
    Osc,
    /// Got `ESC` while inside an OSC
    OscEsc,
}

/// A capability probe for a client which is currently running.
#[derive(Default)]
pub struct Probe {
    frame: Frame,
    /// The escape sequence currently being collected
    run: Vec<u8>,
    /// Keystrokes typed while the probe was open, replayed once it closes
    pending: Vec<u8>,
    /// Terminal size in cells, from `pty-req`. Needed to turn [`TEXT_AREA`] into a cell
    /// size, which is the one thing the client's own replies cannot tell us.
    grid: (u16, u16),
    caps: GraphicsCapabilities,
    /// Cell size in device pixels, from [`CELL_SIZE`]
    window_ops: Option<(u16, u16)>,
    /// Text area in pixels, from [`TEXT_AREA`]
    text_area: Option<(u16, u16)>,
    /// Cell size from iTerm2's three field `ReportCellSize`
    iterm_scaled: Option<(u16, u16)>,
    /// Cell size from iTerm2's two field `ReportCellSize`
    iterm_points: Option<(u16, u16)>,
    /// XTVERSION response
    name: Option<String>,
    /// Set once the `CSI 5 n` fence replies after all commands finished
    fenced: bool,
}

impl Probe {
    /// Create a `Probe`, given the the terminal size in cells.
    pub fn new(grid: (u16, u16)) -> Self {
        Self { grid, ..Self::default() }
    }

    /// Builds the complete query payload.
    ///
    /// Currently, tmux is unsupported, and this will NOT WORK without passthrough.
    pub fn query() -> Vec<u8> {
        format!("{KITTY_GFX}{DA1}{CELL_SIZE}{TEXT_AREA}{ITERM_CELL_SIZE}{XTVERSION}{FENCE}")
            .into_bytes()
    }

    /// Whether the client has answered the fence.
    ///
    /// Once the fence has been answered, we can stop waiting for any more responses
    /// and let go of any data we held onto in the accumulator.
    pub fn is_fenced(&self) -> bool {
        self.fenced
    }

    /// Feed bytes from the client. Call [`Probe::take_input`] to collect the keystrokes
    /// that were interleaved with the replies.
    pub fn feed(&mut self, data: &[u8]) {
        for &byte in data {
            self.feed_byte(byte);
        }
    }

    fn feed_byte(&mut self, byte: u8) {
        if self.run.len() >= MAX_RUN {
            // Bound the accumulator by a maximum size, flush inputs to not let it
            // grow forever
            self.flush_run_as_input();
        }

        match self.frame {
            Frame::Ground => {
                if byte == 0x1b {
                    self.frame = Frame::Esc;
                    self.run.push(byte);
                } else {
                    self.pending.push(byte);
                }
            }
            Frame::Esc => {
                self.run.push(byte);
                match byte {
                    b'[' => self.frame = Frame::Csi,
                    b'_' | b'P' => self.frame = Frame::Apc,
                    b']' => self.frame = Frame::Osc,
                    _ => self.flush_run_as_input(), // user input
                }
            }
            Frame::Csi => {
                self.run.push(byte);
                if (0x40..=0x7e).contains(&byte) {
                    self.consume_run();
                }
            }
            Frame::Apc => {
                self.run.push(byte);
                if byte == 0x1b {
                    self.frame = Frame::ApcEsc;
                }
            }
            Frame::ApcEsc => {
                self.run.push(byte);
                if byte == b'\\' {
                    self.consume_run();
                } else {
                    self.frame = Frame::Apc;
                }
            }
            Frame::Osc => {
                self.run.push(byte);
                match byte {
                    0x07 => self.consume_run(),
                    0x1b => self.frame = Frame::OscEsc,
                    _ => {}
                }
            }
            Frame::OscEsc => {
                self.run.push(byte);
                if byte == b'\\' {
                    self.consume_run();
                } else {
                    self.frame = Frame::Osc;
                }
            }
        }
    }

    /// Interpret one complete escape sequence.
    fn consume_run(&mut self) {
        match self.run.first_chunk::<2>() {
            Some(b"\x1bP") => self.parse_xtversion(),
            Some(b"\x1b_") => self.parse_kitty(),
            Some(b"\x1b[") => self.parse_csi(),
            Some(b"\x1b]") => self.parse_osc(),
            _ => {}
        }

        self.run.clear();
        self.frame = Frame::Ground;
    }

    /// Parses the accumulator to see if it was a successful kitty graphics
    /// protocol response.
    fn parse_kitty(&mut self) {
        if self.run == b"\x1b_Gi=31;OK\x1b\\" {
            self.caps.kitty = true;
        }
    }

    /// Parses sixel support, cell size, text area and the fence.
    ///
    /// Responses: DA1 (`CSI ? ... c`), cell size (`CSI 6 ; h ; w t`), text area
    /// (`CSI 4 ; h ; w t`) and the DSR fence (`CSI 0 n`).
    fn parse_csi(&mut self) {
        // Only called if this is a valid escaped sequence, fine to slice here
        let Some((&final_byte, body)) = self.run[2..].split_last() else {
            return;
        };

        let text = String::from_utf8_lossy(body);

        match final_byte {
            // PDA Attribute 4 = sixel
            b'c' => {
                if let Some(params) = text.strip_prefix('?') {
                    self.caps.sixel = params.split(';').any(|param| param == "4");
                }
            }

            // Window manipulation: cell size at 6, text area in pixels and cells at 4 and 8
            b't' => match text.split(';').collect::<Vec<_>>()[..] {
                // Cell size, directly usable
                ["6", height, width] => {
                    if let (Ok(height), Ok(width)) = (height.parse(), width.parse()) {
                        if width > 0 && height > 0 {
                            self.window_ops = Some((width, height));
                        }
                    }
                }

                // Text area in pixels; need to find grid size to calculate cell size
                ["4", height, width] => {
                    if let (Ok(height), Ok(width)) = (height.parse(), width.parse()) {
                        if width > 0 && height > 0 {
                            self.text_area = Some((width, height));
                        }
                    }
                }

                _ => {}
            },

            // Fence: every earlier query has been answered or ignored by now
            b'n' => {
                if text == "0" {
                    self.fenced = true;
                }
            }

            _ => {}
        }
    }

    /// Parses iTerm2's cell size report.
    ///
    /// Response: `OSC 1337;ReportCellSize=<height>;<width>;<scale> ST`.
    fn parse_osc(&mut self) {
        let Some(body) = self.run.strip_prefix(b"\x1b]").and_then(|rest| {
            rest.strip_suffix(b"\x07").or_else(|| rest.strip_suffix(b"\x1b\\"))
        }) else {
            return;
        };

        let text = String::from_utf8_lossy(body);
        let Some(params) = text.strip_prefix("1337;ReportCellSize=") else {
            return;
        };

        // Implausible values would distort every image scale
        let plausible = |width: f64, height: f64| {
            ((1.0..=512.0).contains(&width) && (1.0..=512.0).contains(&height))
                .then_some((width.round() as u16, height.round() as u16))
        };

        match params.split(';').collect::<Vec<_>>()[..] {
            // Three field form, includes scale, higher priority
            [height, width, scale] => {
                if let (Ok(height), Ok(width), Ok(scale)) =
                    (height.parse::<f64>(), width.parse::<f64>(), scale.parse::<f64>())
                {
                    self.iterm_scaled = plausible(width * scale, height * scale);
                }
            }

            // Two field form, from older versions, lower priority
            [height, width] => {
                if let (Ok(height), Ok(width)) = (height.parse::<f64>(), width.parse::<f64>())
                {
                    self.iterm_points = plausible(width, height);
                }
            }

            _ => {}
        }
    }

    /// Parses an `XTVERSION` response to get the response text, performing sanity checks.  
    fn parse_xtversion(&mut self) {
        let Some(body) =
            self.run.strip_prefix(b"\x1bP>|").and_then(|rest| rest.strip_suffix(b"\x1b\\"))
        else {
            return;
        };

        let name: String = String::from_utf8_lossy(body)
            .chars()
            .filter(|c| !c.is_control())
            .take(32)
            .collect();

        // Empty values are not valid, and should remain as `None`
        let name = name.trim();
        if !name.is_empty() {
            // iTerm2 graphics support is not detected using a capabilities query, but
            // rather the terminal name
            self.caps.iterm2 =
                TerminalInfo::leading_token_inner(name).eq_ignore_ascii_case("iterm2");
            self.name = Some(name.to_string());
        }
    }

    /// Flush the remaining data in the accumulator as user input to be replayed.
    fn flush_run_as_input(&mut self) {
        self.pending.append(&mut self.run);
        self.frame = Frame::Ground;
    }

    /// Drains all user input from the probe, returning it.
    pub fn take_input(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }

    /// Returns the cell size of the terminal, in the order of priority:
    ///
    ///   1. [`CELL_SIZE`] - integers, device pixels, no ambiguity
    ///   2. [`ITERM_CELL_SIZE`] with a scale, measured in points
    ///   3. [`TEXT_AREA`] / `grid` - pixels with small rounding errors
    ///   4. [`ITERM_CELL_SIZE`] without any known scale
    fn ranked_cell_size(&self) -> Option<(u16, u16)> {
        let derived = || {
            let (px_width, px_height) = self.text_area?;
            let (cols, rows) = match self.grid {
                (0, _) | (_, 0) => return None,
                grid => grid,
            };

            // A stale grid or an odd report would distort every image scaled against it
            match (px_width / cols, px_height / rows) {
                (w, h) if (4..=64).contains(&w) && (4..=128).contains(&h) => Some((w, h)),
                (w, h) => {
                    tracing::debug!("Ignoring implausible derived cell size {w}x{h}");
                    None
                }
            }
        };

        let (size, source) = self
            .window_ops
            .map(|size| (size, "CSI 16 t"))
            .or(self.iterm_scaled.map(|size| (size, "ReportCellSize+scale")))
            .or_else(|| derived().map(|size| (size, "CSI 14 t / grid")))
            .or(self.iterm_points.map(|size| (size, "ReportCellSize")))?;

        tracing::debug!("Cell size {size:?} taken from {source}");
        Some(size)
    }

    /// Finalizes the probe, flushes the remaining user inputs held, and returns the
    /// results of the process.
    pub fn finish(mut self) -> ProbeOutcome {
        if !self.run.is_empty() {
            self.pending.append(&mut self.run);
        }

        ProbeOutcome {
            caps: self.caps,
            cell_size: self.ranked_cell_size(),
            name: self.name,
            pending_input: self.pending,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub caps: GraphicsCapabilities,
    pub cell_size: Option<(u16, u16)>,
    pub name: Option<String>,
    pub pending_input: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: (u16, u16) = (150, 40);

    fn probe_with(input: &[u8]) -> ProbeOutcome {
        let mut probe = Probe::new(GRID);
        probe.feed(input);
        probe.finish()
    }

    #[test]
    fn parses_kitty_da1_cellsize_and_fence() {
        let mut probe = Probe::new(GRID);
        probe.feed(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;4c\x1b[6;17;8t\x1b[0n");
        assert!(probe.is_fenced());

        let out = probe.finish();
        assert!(out.caps.kitty);
        assert!(out.caps.sixel);
        assert_eq!(out.cell_size, Some((8, 17)));
        assert!(out.pending_input.is_empty());
    }

    #[test]
    fn the_window_ops_report_number_is_honoured() {
        // 6 is a cell size, used as-is
        assert_eq!(probe_with(b"\x1b[6;17;8t\x1b[0n").cell_size, Some((8, 17)));
        // 4 is a text area in pixels, usable only once divided by the grid
        assert_eq!(probe_with(b"\x1b[4;680;1050t\x1b[0n").cell_size, Some((7, 17)));
        // 8 is a text area in cells
        assert_eq!(probe_with(b"\x1b[8;48;186t\x1b[0n").cell_size, None);
    }

    #[test]
    fn zero_dimensions_are_not_a_cell_size() {
        assert_eq!(probe_with(b"\x1b[6;0;8t\x1b[0n").cell_size, None);
        assert_eq!(probe_with(b"\x1b[6;17;0t\x1b[0n").cell_size, None);
    }

    #[test]
    fn keystrokes_interleaved_with_replies_are_preserved() {
        assert_eq!(probe_with(b"a\x1b[?62;4cb\x1b[0nc").pending_input, b"abc");
    }

    #[test]
    fn da1_without_attribute_four_is_not_sixel() {
        // ghostty and kitty omit attribute 4
        assert!(!probe_with(b"\x1b[?62;22;52c\x1b[0n").caps.sixel);
        assert!(!probe_with(b"\x1b[?62;52;c\x1b[0n").caps.sixel);

        // tmux with sixel reports it
        assert!(probe_with(b"\x1b[?1;2;4c\x1b[0n").caps.sixel);
    }

    #[test]
    fn a_parameter_merely_containing_four_is_not_sixel() {
        // `?64;14c` has no standalone 4
        assert!(!probe_with(b"\x1b[?64;14c\x1b[0n").caps.sixel);
    }

    #[test]
    fn only_the_exact_kitty_success_reply_counts() {
        assert!(probe_with(b"\x1b_Gi=31;OK\x1b\\\x1b[0n").caps.kitty);
        assert!(!probe_with(b"\x1b_Gi=31;ENOENT:bad\x1b\\\x1b[0n").caps.kitty);
        assert!(!probe_with(b"\x1b_Gi=99;OK\x1b\\\x1b[0n").caps.kitty);
    }

    #[test]
    fn a_bare_escape_keypress_is_returned_as_input() {
        // The escape itself is user input, followed by q, should not be parsed
        assert_eq!(probe_with(b"\x1bq").pending_input, b"\x1bq");
    }

    #[test]
    fn an_unterminated_sequence_is_returned_rather_than_swallowed() {
        assert_eq!(probe_with(b"\x1b[6;17").pending_input, b"\x1b[6;17");
    }

    #[test]
    fn silence_yields_no_capabilities_and_no_fence() {
        let mut probe = Probe::new(GRID);
        probe.feed(b"");
        assert!(!probe.is_fenced());

        let out = probe.finish();
        assert_eq!(out.caps, GraphicsCapabilities::default());
        assert_eq!(out.cell_size, None);
        assert_eq!(out.name, None);
        assert!(out.pending_input.is_empty());
    }

    #[test]
    fn xtversion_reply_yields_the_terminal_name() {
        assert_eq!(
            probe_with(b"\x1bP>|kitty(0.48.2)\x1b\\\x1b[0n").name.as_deref(),
            Some("kitty(0.48.2)")
        );
        assert_eq!(
            probe_with(b"\x1bP>|ghostty 1.3.1-arch2\x1b\\\x1b[0n").name.as_deref(),
            Some("ghostty 1.3.1-arch2")
        );
        assert_eq!(
            probe_with(b"\x1bP>|Konsole 26.04.3\x1b\\\x1b[0n").name.as_deref(),
            Some("Konsole 26.04.3")
        );
    }

    #[test]
    fn xtversion_and_capability_replies_coexist() {
        let out = probe_with(
            b"\x1bP>|foot(1.16.2)\x1b\\\x1b_Gi=31;OK\x1b\\\x1b[?62;4c\x1b[6;17;8t\x1b[0n",
        );
        assert_eq!(out.name.as_deref(), Some("foot(1.16.2)"));
        assert!(out.caps.kitty);
        assert!(out.caps.sixel);
        assert_eq!(out.cell_size, Some((8, 17)));
    }

    #[test]
    fn a_blank_name_is_not_recorded() {
        assert_eq!(probe_with(b"\x1bP>|   \x1b\\\x1b[0n").name, None);
        assert_eq!(probe_with(b"\x1bP>|\x1b\\\x1b[0n").name, None);
    }

    #[test]
    fn a_malformed_dcs_is_ignored_rather_than_guessed_at() {
        assert_eq!(probe_with(b"\x1bPsomething\x1b\\\x1b[0n").name, None);
    }

    #[test]
    fn a_scaled_iterm2_cell_size_is_converted_to_device_pixels() {
        // Retina display: 7x17 points at scale 2 is a 14x34 pixel cell
        assert_eq!(
            probe_with(b"\x1b]1337;ReportCellSize=17;7;2\x1b\\\x1b[0n").cell_size,
            Some((14, 34)),
            "height comes first, and the scale is applied"
        );

        // Verbatim example from documentation
        assert_eq!(
            probe_with(b"\x1b]1337;ReportCellSize=17.50;8.00;2.0\x1b\\\x1b[0n").cell_size,
            Some((16, 35))
        );
    }

    #[test]
    fn better_sourced_cell_sizes_win() {
        let scaled = b"\x1b]1337;ReportCellSize=17;7;2\x1b\\"; // 14x34 device px
        let points = b"\x1b]1337;ReportCellSize=17;7\x1b\\"; //  7x17, scale unknown
        let area = b"\x1b[4;680;1050t"; //  7x17 once divided
        let cells = b"\x1b[6;17;8t"; //  8x17 device px

        let sized = |parts: &[&[u8]]| {
            let mut probe = Probe::new(GRID);
            for part in parts {
                probe.feed(part);
            }
            probe.finish().cell_size
        };

        assert_eq!(sized(&[cells, scaled, area, points]), Some((8, 17)));
        assert_eq!(sized(&[scaled, area, points]), Some((14, 34)));
        assert_eq!(sized(&[area, points]), Some((7, 17)));
        assert_eq!(sized(&[points]), Some((7, 17)));
    }

    #[test]
    fn implausible_iterm2_cell_sizes_are_rejected() {
        for reply in [
            &b"\x1b]1337;ReportCellSize=0;0;1\x1b\\"[..],
            &b"\x1b]1337;ReportCellSize=99999;99999;2\x1b\\"[..],
            &b"\x1b]1337;ReportCellSize=abc;def;1\x1b\\"[..],
            &b"\x1b]1337;ReportCellSize=17\x1b\\"[..],
        ] {
            let mut probe = Probe::new(GRID);
            probe.feed(reply);
            let out = probe.finish();
            assert_eq!(out.cell_size, None, "{reply:?}");
        }
    }

    #[test]
    fn a_derived_cell_size_needs_a_usable_grid() {
        let sized_for = |grid| {
            let mut probe = Probe::new(grid);
            probe.feed(b"\x1b[4;680;1050t\x1b[0n");
            probe.finish().cell_size
        };

        assert_eq!(sized_for((150, 40)), Some((7, 17)));

        // Implausible results
        assert_eq!(sized_for((0, 40)), None);
        assert_eq!(sized_for((150, 0)), None);
        assert_eq!(sized_for((2, 2)), None, "525x340 is not a cell");
    }

    #[test]
    fn a_zero_text_area_is_rejected() {
        assert_eq!(probe_with(b"\x1b[4;0;387t\x1b[0n").cell_size, None);
        assert_eq!(probe_with(b"\x1b[4;570;0t\x1b[0n").cell_size, None);
    }

    #[test]
    fn osc_replies_are_framed_and_never_become_keystrokes() {
        assert!(probe_with(b"\x1b]1337;Capabilities=TFMB\x07\x1b[0n")
            .pending_input
            .is_empty());
        assert!(probe_with(b"\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\\x1b[0n")
            .pending_input
            .is_empty());
        assert_eq!(probe_with(b"\x1b]0;title\x07hi\x1b[0n").pending_input, b"hi");
    }

    #[test]
    fn an_endless_sequence_is_bounded_and_given_back() {
        let mut flood = vec![0x1b, b'['];
        flood.extend(std::iter::repeat_n(b';', 4096));

        let out = probe_with(&flood);
        assert!(
            out.pending_input.len() >= 4000,
            "bytes must be returned, not dropped: got {}",
            out.pending_input.len()
        );
    }

    #[test]
    fn the_query_keeps_the_fence_last() {
        let query = Probe::query();
        assert!(query.ends_with(FENCE.as_bytes()), "fence must terminate the query");
        for part in [KITTY_GFX, DA1, CELL_SIZE, XTVERSION] {
            assert!(
                query.windows(part.len()).any(|w| w == part.as_bytes()),
                "{part:?} must be present"
            );
        }
    }
}
