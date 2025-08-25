use std::default::Default;

use default_variant::default;
use serde::{Deserialize, Serialize};
use strum::Display;

#[cfg(feature = "blog")]
use ratatui_image::{
    picker::{Capability, ProtocolType},
    FontSize,
};

#[cfg(feature = "blog")]
pub const DEFAULT_FONT_SIZE: FontSize = (12, 12);

#[derive(Debug, Default, Clone)]
pub struct TerminalInfo {
    kind: TerminalKind,
    #[cfg(feature = "blog")]
    font_size: Option<FontSize>,
}

impl TerminalInfo {
    /// Get the terminal kind.
    pub fn kind(&self) -> &TerminalKind {
        &self.kind
    }

    /// Get the font size.
    #[cfg(feature = "blog")]
    pub fn font_size(&self) -> FontSize {
        self.font_size.unwrap_or(DEFAULT_FONT_SIZE)
    }

    /// Sets the terminal kind, if currently unset (i.e., unprobed).
    pub fn set_kind(&mut self, kind: TerminalKind) {
        if matches!(self.kind, TerminalKind::Unsupported(UnsupportedReason::Unprobed)) {
            self.kind = kind;
        }
    }

    /// Sets the font size.
    #[cfg(feature = "blog")]
    pub fn set_font_size(&mut self, font_size: FontSize) {
        self.font_size = Some(font_size);
    }
}

#[derive(Debug, Deserialize, Serialize, Display, Clone /*, Copy */)]
#[default(Unsupported(UnsupportedReason::default()))]
#[strum(serialize_all = "lowercase")]
pub enum TerminalKind {
    Ghostty,
    Hyper,
    ITerm2,
    Kitty,
    MinTty,
    Rio,
    Tabby,
    Vscode,
    Wezterm,
    Unsupported(UnsupportedReason),
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy)]
pub enum UnsupportedReason {
    /// Terminal emulator does not provide real pixel size, making it impossible to calculate
    /// font size.
    ///
    /// Currently known terminal emulators which exhibit this behavior:
    ///
    /// - VSCode
    Unsized,

    /// Terminal emulator is not known.
    Unknown,

    /// Terminal emulator has not been detected yet. This is only set during SSH initialization.
    #[default]
    Unprobed,
}

impl TerminalKind {
    pub const ALL_SUPPORTED: [Self; 9] = [
        Self::Ghostty,
        Self::Hyper,
        Self::ITerm2,
        Self::Kitty,
        Self::MinTty,
        Self::Rio,
        Self::Tabby,
        Self::Vscode,
        Self::Wezterm,
    ];

    pub fn from_term_program(program: &str) -> Self {
        let terminals = [
            ("ghostty", Self::Ghostty),
            ("iTerm.app", Self::ITerm2),
            ("iTerm2", Self::ITerm2),
            ("WezTerm", Self::Wezterm),
            ("mintty", Self::MinTty),
            ("vscode", Self::Vscode),
            ("Tabby", Self::Tabby),
            ("Hyper", Self::Hyper),
            ("rio", Self::Rio),
        ];

        for (term, variant) in terminals {
            if program.contains(term) {
                return variant;
            }
        }

        Self::Unsupported(UnsupportedReason::Unknown)
    }

    pub fn supported() -> String {
        Self::ALL_SUPPORTED.map(|term| term.to_string()).join(", ")
    }

    #[cfg(feature = "blog")]
    pub fn capabilities(&self) -> Vec<Capability> {
        match *self {
            Self::Hyper | Self::Vscode => vec![Capability::RectangularOps],
            Self::Ghostty => vec![Capability::Kitty, Capability::RectangularOps],
            Self::Tabby | Self::MinTty => vec![Capability::Sixel, Capability::RectangularOps],
            Self::Rio => vec![Capability::Sixel, Capability::RectangularOps],
            Self::ITerm2 | Self::Wezterm => {
                vec![Capability::Sixel, Capability::Kitty, Capability::RectangularOps]
            }
            Self::Kitty => vec![
                Capability::Kitty,
                Capability::RectangularOps,
                Capability::TextSizingProtocol, // !! TODO: THIS COULD BE SO FUCKING COOL FOR MARKDOWN HEADINGS !!
            ],

            Self::Unsupported(_) => vec![],
        }
    }

    #[cfg(feature = "blog")]
    pub fn as_protocol(&self) -> ProtocolType {
        if matches!(
            self,
            Self::ITerm2
                | Self::Wezterm
                | Self::MinTty
                | Self::Vscode
                | Self::Tabby
                | Self::Hyper
                | Self::Rio
        ) {
            return ProtocolType::Iterm2;
        } else if self.capabilities().contains(&Capability::Kitty) {
            return ProtocolType::Kitty;
        } else if self.capabilities().contains(&Capability::Sixel) {
            return ProtocolType::Sixel;
        }

        ProtocolType::Halfblocks
    }
}
