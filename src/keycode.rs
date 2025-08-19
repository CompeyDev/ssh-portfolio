use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub const CTRL_C: char = 3 as char;
pub const CTRL_D: char = 4 as char;
pub const CTRL_Z: char = 26 as char;

pub trait KeyCodeExt {
    /// Create a [KeyCode] from a keystroke byte.
    fn from(code: u8) -> KeyCode;
    /// Create a [KeyCode] from a xterm byte sequence of keystroke data.
    fn from_xterm_seq(seq: &[u8]) -> KeyCode;
    /// Consume the [KeyCode], converting it into a [KeyEvent].
    fn into_key_event(self) -> KeyEvent;
}

impl KeyCodeExt for KeyCode {
    fn from(code: u8) -> KeyCode {
        match code {
            // Control sequences (non exhaustive)
            3 => Self::Char(CTRL_C),
            4 => Self::Char(CTRL_D),
            26 => Self::Char(CTRL_Z),

            // Common control characters
            8 => KeyCode::Backspace,
            9 => KeyCode::Tab,
            13 => KeyCode::Enter,
            27 => KeyCode::Esc,

            // Arrow keys
            37 => KeyCode::Left,
            38 => KeyCode::Up,
            39 => KeyCode::Right,
            40 => KeyCode::Down,

            // Navigation keys
            33 => KeyCode::PageUp,
            34 => KeyCode::PageDown,
            36 => KeyCode::Home,
            35 => KeyCode::End,
            45 => KeyCode::Insert,
            46 => KeyCode::Delete,

            // Menu key
            93 => KeyCode::Menu,

            // Printable ASCII characters
            b' '..=b'~' => KeyCode::Char(code as char),

            // Special characters
            127 => KeyCode::Backspace,

            // Caps/Num/Scroll Lock
            20 => KeyCode::CapsLock,
            144 => KeyCode::NumLock,
            145 => KeyCode::ScrollLock,

            // Pause/Break
            19 => KeyCode::Pause,

            // Anything else
            _ => KeyCode::Null,
        }
    }

    #[rustfmt::skip]
    fn from_xterm_seq(seq: &[u8]) -> Self {
        let codes = seq
            .iter()
            .map(|&b| <Self as KeyCodeExt>::from(b))
            .collect::<Vec<Self>>();
        
        match codes.as_slice() {
            [Self::Esc, Self::Char('['), Self::Char('A')] => Self::Up,
            [Self::Esc, Self::Char('['), Self::Char('B')] => Self::Down,
            [Self::Esc, Self::Char('['), Self::Char('C')] => Self::Right,
            [Self::Esc, Self::Char('['), Self::Char('D')] => Self::Left,
            [Self::Esc, Self::Char('['), Self::Char('1'), Self::Char('~')] => Self::Home,
            [Self::Esc, Self::Char('['), Self::Char('4'), Self::Char('~')] => Self::End,
            [Self::Esc, Self::Char('['), Self::Char('3'), Self::Char('~')] => Self::Delete,
            [Self::Esc, Self::Char('['), Self::Char('5'), Self::Char('~')] => Self::PageUp,
            [Self::Esc, Self::Char('['), Self::Char('6'), Self::Char('~')] => Self::PageDown,
            [Self::Esc, Self::Char('O'), Self::Char('P')] => Self::F(1),
            [Self::Esc, Self::Char('O'), Self::Char('Q')] => Self::F(2),
            [Self::Esc, Self::Char('O'), Self::Char('R')] => Self::F(3),
            [Self::Esc, Self::Char('O'), Self::Char('S')] => Self::F(4),
            [Self::Esc, Self::Char('['), Self::Char('1'), Self::Char('5'), Self::Char('~')] => Self::F(5),
            [Self::Esc, Self::Char('['), Self::Char('1'), Self::Char('7'), Self::Char('~')] => Self::F(6),
            [Self::Esc, Self::Char('['), Self::Char('1'), Self::Char('8'), Self::Char('~')] => Self::F(7),
            [Self::Esc, Self::Char('['), Self::Char('1'), Self::Char('9'), Self::Char('~')] => Self::F(8),
            [Self::Esc, Self::Char('['), Self::Char('2'), Self::Char('0'), Self::Char('~')] => Self::F(9),
            [Self::Esc, Self::Char('['), Self::Char('2'), Self::Char('1'), Self::Char('~')] => Self::F(10),
            [Self::Esc, Self::Char('['), Self::Char('2'), Self::Char('3'), Self::Char('~')] => Self::F(11),
            [Self::Esc, Self::Char('['), Self::Char('2'), Self::Char('4'), Self::Char('~')] => Self::F(12),
            [single] => *single,
            _ => KeyCode::Null,
        }
    }

    fn into_key_event(self) -> KeyEvent {
        match self {
            Self::Char(CTRL_C) => KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Self::Char(CTRL_D) => KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
            Self::Char(CTRL_Z) => KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL),
            other => KeyEvent::new(other, KeyModifiers::empty()),
        }
    }
}

#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_keycode_from_control_chars() {
        assert_eq!(<KeyCode as KeyCodeExt>::from(8), KeyCode::Backspace);
        assert_eq!(<KeyCode as KeyCodeExt>::from(9), KeyCode::Tab);
        assert_eq!(<KeyCode as KeyCodeExt>::from(13), KeyCode::Enter);
        assert_eq!(<KeyCode as KeyCodeExt>::from(27), KeyCode::Esc);
    }

    #[test]
    fn test_keycode_from_printable_ascii() {
        assert_eq!(<KeyCode as KeyCodeExt>::from(b'a'), KeyCode::Char('a'));
        assert_eq!(<KeyCode as KeyCodeExt>::from(b'Z'), KeyCode::Char('Z'));
        assert_eq!(<KeyCode as KeyCodeExt>::from(b'0'), KeyCode::Char('0'));
        assert_eq!(<KeyCode as KeyCodeExt>::from(b'~'), KeyCode::Char('~'));
    }

    #[test]
    fn test_keycode_from_special_keys() {
        assert_eq!(<KeyCode as KeyCodeExt>::from(20), KeyCode::CapsLock);
        assert_eq!(<KeyCode as KeyCodeExt>::from(144), KeyCode::NumLock);
        assert_eq!(<KeyCode as KeyCodeExt>::from(145), KeyCode::ScrollLock);
        assert_eq!(<KeyCode as KeyCodeExt>::from(19), KeyCode::Pause);
        assert_eq!(<KeyCode as KeyCodeExt>::from(0), KeyCode::Null);
    }

    #[test]
    fn test_keycode_from_invalid() {
        assert_eq!(<KeyCode as KeyCodeExt>::from(255), KeyCode::Null);
        assert_eq!(<KeyCode as KeyCodeExt>::from(200), KeyCode::Null);
    }

    #[test]
    fn test_keycode_from_seq() {
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[65]), KeyCode::Char('A'));
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27]), KeyCode::Esc);
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[0]), KeyCode::Null);

        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 68]), KeyCode::Left);
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 67]), KeyCode::Right);
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 65]), KeyCode::Up);
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 66]), KeyCode::Down); 

        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 53, 126]), KeyCode::PageUp);
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 54, 126]), KeyCode::PageDown);
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 51, 126]), KeyCode::Delete);
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 52, 126]), KeyCode::End);

        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 49, 56, 126]), KeyCode::F(7));
        assert_eq!(<KeyCode as KeyCodeExt>::from_xterm_seq(&[27, 91, 49, 57, 126]), KeyCode::F(8));
    }

    #[test]
    fn test_into_key_event() {
        let key_code = KeyCode::Char('a');
        let key_event = <KeyCode as KeyCodeExt>::into_key_event(key_code);
        assert_eq!(key_event, KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()));
    }
}
