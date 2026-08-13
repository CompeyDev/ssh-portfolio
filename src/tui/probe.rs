use super::terminal::ProbedCapabilities;

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

/// xterm version. Returns the name and version of the terminal. 
/// 
/// Response looks like: `DCS > | text ST`.
const XTVERSION: &str = "\x1b[>q";

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
}

/// A capability probe for a client which is currently running.
#[derive(Default)]
pub struct Probe {
    frame: Frame,
    /// The escape sequence currently being collected
    run: Vec<u8>,
    /// Keystrokes typed while the probe was open, replayed once it closes
    pending: Vec<u8>,
    caps: ProbedCapabilities,
    cell_size: Option<(u16, u16)>,
    /// XTVERSION response
    name: Option<String>, 
    /// Set once the `CSI 5 n` fence replies after all commands finished
    fenced: bool,
}

impl Probe {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the complete query payload.
    /// 
    /// Currently, tmux is unsupported, and this will NOT WORK without passthrough.
    pub fn query() -> Vec<u8> {
        format!("{KITTY_GFX}{DA1}{CELL_SIZE}{XTVERSION}{FENCE}").into_bytes()
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
            }
        }
    }

    /// Interpret one complete escape sequence.
    fn consume_run(&mut self) {
        match self.run.first_chunk::<2>() {
            Some(b"\x1bP") => self.parse_xtversion(),
            Some(b"\x1b_") => self.parse_kitty(),
            Some(b"\x1b[") => self.parse_csi(),
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

    /// Parses sixel support and cell size.
    /// 
    /// Payload: DA1 (`CSI ? ... c`), cell size (`CSI 6 ; h ; w t`), and the DSR fence (`CSI 0 n`)
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
            b't' => {
                if let ["6", height, width] = text.split(';').collect::<Vec<_>>()[..] {
                    if let (Ok(height), Ok(width)) = (height.parse(), width.parse()) {
                        if width > 0 && height > 0 {
                            self.cell_size = Some((width, height));
                        }
                    }
                }
            }

            // Fence: payload fully returned
            b'n' => {
                if text == "0" {
                    self.fenced = true;
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

    /// Finalizes the probe, flushes the remaining user inputs held, and returns the
    /// results of the process.
    pub fn finish(mut self) -> ProbeOutcome {
        if !self.run.is_empty() {
            self.pending.append(&mut self.run);
        }

        ProbeOutcome {
            caps: self.caps,
            cell_size: self.cell_size,
            name: self.name,
            pending_input: self.pending,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProbeOutcome {
    pub caps: ProbedCapabilities,
    pub cell_size: Option<(u16, u16)>,
    pub name: Option<String>,
    pub pending_input: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_with(input: &[u8]) -> ProbeOutcome {
        let mut probe = Probe::new();
        probe.feed(input);
        probe.finish()
    }

    #[test]
    fn parses_kitty_da1_cellsize_and_fence() {
        let mut probe = Probe::new();
        probe.feed(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;4c\x1b[6;17;8t\x1b[0n");
        assert!(probe.is_fenced());

        let out = probe.finish();
        assert!(out.caps.kitty);
        assert!(out.caps.sixel);
        assert_eq!(out.cell_size, Some((8, 17)));
        assert!(out.pending_input.is_empty());
    }

    #[test]
    fn only_report_six_is_a_cell_size() {
        assert_eq!(probe_with(b"\x1b[4;816;1488t\x1b[0n").cell_size, None);
        assert_eq!(probe_with(b"\x1b[8;48;186t\x1b[0n").cell_size, None);
        assert_eq!(probe_with(b"\x1b[6;17;8t\x1b[0n").cell_size, Some((8, 17)));
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
        let mut probe = Probe::new();
        probe.feed(b"");
        assert!(!probe.is_fenced());

        let out = probe.finish();
        assert_eq!(out.caps, ProbedCapabilities::default());
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
