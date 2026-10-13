use std::sync::Arc;

use ratatui::prelude::*;
use tokio::sync::RwLock;

use super::cat::{art, WIDTH as CAT_WIDTH};
use super::content::truncate;
use crate::tui::terminal::TerminalInfo;

// Palette as RGB, useful for the fade out effect where the brightness is dropped.
// TODO: apply theming from config?
const MAGENTA: (u8, u8, u8) = (199, 146, 234);
const GRAY: (u8, u8, u8) = (110, 110, 118);
const WHITE: (u8, u8, u8) = (210, 210, 214);

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// Timeline in seconds
const T_MEOW: f32 = 3.10;
const T_MEOW_END: f32 = 3.45;
const T_GREET: f32 = 3.15;
const T_TYPED: f32 = 3.65;
const FADE_START: f32 = 4.60;

/// How long the whole boot runs before handing off to the TUI.
pub const BOOT_TOTAL: f32 = 4.90;

// Once the greeting is fully typed out, how long it should pulse and between what 
// minimum value for the brightness
const SHIMMER_FLOOR: f32 = 0.78;
const SHIMMER_SPEED: f32 = 4.5;

fn style(c: (u8, u8, u8), fade: f32, bold: bool) -> Style {
    let s = Style::default().fg(Color::Rgb(
        (c.0 as f32 * fade) as u8,
        (c.1 as f32 * fade) as u8,
        (c.2 as f32 * fade) as u8,
    ));

    if bold {
        s.add_modifier(Modifier::BOLD)
    } else {
        s
    }
}

/// Stages of Debbie's wake-up animation
#[derive(Clone, Copy)]
enum Wake {
    Asleep,
    Squash,
    Half,
    Bow,
    Loaf,
}

impl Wake {
    fn body(self, eyes: &str, muzzle: &str) -> Vec<String> {
        match self {
            Wake::Asleep => art::flat(eyes, muzzle, ",", true, false),
            Wake::Squash => art::flat(eyes, muzzle, ",", false, false),
            Wake::Half => art::half(eyes, muzzle, "~,"),
            Wake::Bow => art::bow(eyes, muzzle, "~,"),
            Wake::Loaf => art::loaf(eyes, muzzle, ",", "~,"),
        }
    }
}

/// Describes the appearance (pose, eyes, muzzle and z's) for an instant in time from the
/// start of the timer, in seconds 
struct Keyframe {
    until: f32,
    pose: Wake,
    eyes: &'static str,
    muzzle: &'static str,
    zs: bool,
}

const WAKE: &[Keyframe] = &[
    Keyframe { until: 1.50, pose: Wake::Asleep, eyes: "-.-", muzzle: "- Y -", zs: true }, // asleep
    Keyframe { until: 1.75, pose: Wake::Asleep, eyes: "o.o", muzzle: "- Y -", zs: false }, // stir, eyes open
    Keyframe { until: 1.95, pose: Wake::Squash, eyes: "-.-", muzzle: "- Y -", zs: false },
    Keyframe { until: 2.25, pose: Wake::Half, eyes: "-.-", muzzle: "- Y -", zs: false },
    Keyframe { until: 2.75, pose: Wake::Bow, eyes: "-.-", muzzle: "- o -", zs: false }, // bow + yawn
    Keyframe { until: 2.95, pose: Wake::Half, eyes: "-.-", muzzle: "- Y -", zs: false },
    Keyframe { until: T_MEOW, pose: Wake::Squash, eyes: "-.-", muzzle: "- Y -", zs: false }, // land
    Keyframe { until: T_MEOW_END, pose: Wake::Loaf, eyes: "^.^", muzzle: "> w <", zs: false }, // greet meow
];

/// What pose to display from the wake animation at `t` seconds and whether
/// the sleepy z's needs to show at the moment
fn cat_pose(t: f32) -> (Vec<String>, bool) {
    match WAKE.iter().find(|k| t < k.until) {
        Some(k) => (k.pose.body(k.eyes, k.muzzle), k.zs),
        None => (Wake::Loaf.body("o.o", "> Y <"), false), // settled loaf
    }
}

pub struct Boot {
    terminal_info: Arc<RwLock<TerminalInfo>>,
    username: Arc<RwLock<Option<String>>>,
}

impl Boot {
    pub fn new(
        terminal_info: Arc<RwLock<TerminalInfo>>,
        username: Arc<RwLock<Option<String>>>,
    ) -> Self {
        Self { terminal_info, username }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, t: f32) {
        let fade = 1.0 - ((t - FADE_START) / (BOOT_TOTAL - FADE_START)).clamp(0.0, 1.0);

        let w = area.width;
        let h = area.height;
        let cx = area.x + w / 2;

        let log_w: u16 = 34;
        let gap: u16 = 6;
        let pair_left = cx.saturating_sub((log_w + gap + CAT_WIDTH) / 2);
        let cat_left = pair_left + log_w + gap;
        let steps_top = area.y + h / 2 - 2;
        let bottom = steps_top + 3;
        let heading_row = steps_top - 2;
        let greet_row = bottom + 3;

        // Heading
        frame.buffer_mut().set_string(
            pair_left,
            heading_row,
            "  booting ",
            style(MAGENTA, fade, true),
        );

        for x in (pair_left + 10)..(pair_left + log_w) {
            frame.buffer_mut().set_string(x, heading_row, "─", style(GRAY, fade, false));
        }

        // Probe's state
        let info = self.terminal_info.try_read().ok();
        let probe_done = info.as_ref().is_some_and(|i| i.graphics().is_some());
        let term_value = info
            .as_ref()
            .and_then(|i| i.reported_name())
            .map(str::to_string)
            .unwrap_or_else(|| "unnamed".to_string());

        #[cfg(feature = "blog")]
        let gfx_value = info.as_ref().map(|i| {
            if !i.supports_images() {
                "text".to_string()
            } else {
                let (cw, ch) = i.font_size();
                format!("{} · {cw}x{ch}", format!("{:?}", i.protocol()).to_lowercase())
            }
        });

        drop(info);

        // Display the stages of initialization
        let mut log = Log { frame: &mut *frame, x: pair_left, y: steps_top, t, fade };
        log.row(0.15, t >= 0.45, "channel", "established");
        log.row(0.55, probe_done, "terminal", &term_value);
        #[cfg(feature = "blog")]
        log.row(1.00, probe_done, "graphics", gfx_value.as_deref().unwrap_or("text"));
        log.row(1.50, t >= 3.10, "debbie", "awake");

        // Debbie!
        let (art, show_zs) = cat_pose(t);
        let art_top = bottom - (art.len() as u16 - 1);
        let pad = CAT_WIDTH - art.iter().map(|l| l.chars().count() as u16).max().unwrap_or(0);

        for (i, line) in art.iter().enumerate() {
            frame.buffer_mut().set_string(
                cat_left + pad,
                art_top + i as u16,
                line,
                style(MAGENTA, fade, true),
            );
        }

        if show_zs {
            frame.buffer_mut().set_string(
                cat_left + CAT_WIDTH - 6,
                bottom - 6,
                "Z",
                style(GRAY, fade, false),
            );

            frame.buffer_mut().set_string(
                cat_left + CAT_WIDTH - 8,
                bottom - 5,
                "z",
                style(MAGENTA, fade, false),
            );
        }

        // Greet the user by their SSH username
        let name = self.username.try_read().ok().and_then(|g| g.clone());
        if let Some(greeting) = Greeting::at(t, name) {
            greeting.draw(frame, cx, greet_row, fade);
        }

        let hint = "press any key to skip";
        frame.buffer_mut().set_string(
            cx.saturating_sub(hint.len() as u16 / 2),
            area.y + h - 2,
            hint,
            style(GRAY, fade, false),
        );
    }
}

/// The typewriter-style animated greeting for the visitor. It is a pure function of the
/// instant in time it is to do drawn. Similar to [`cat::Moment`].
struct Greeting {
    text: String,
    shown: usize,
    shimmer: f32,
    cursor: bool,
}

impl Greeting {
    /// The state of the greeting to be displayed at `t`, or `None` if no username was provided
    fn at(t: f32, name: Option<String>) -> Option<Self> {
        if t < T_GREET {
            return None;
        }

        let name = name?;
        let text = format!("hiya, {}!", truncate(&name, 24));
        let count = text.chars().count() as f32;
        let typed = ((t - T_GREET) / (T_TYPED - T_GREET)).clamp(0.0, 1.0);

        let shimmer = if t >= T_TYPED {
            let pulse = 0.5 + 0.5 * (t * SHIMMER_SPEED).sin();
            SHIMMER_FLOOR + (1.0 - SHIMMER_FLOOR) * pulse
        } else {
            1.0
        };

        Some(Self {
            shown: (count * typed) as usize,
            cursor: (t * 2.0) as i32 % 2 == 0,
            shimmer,
            text,
        })
    }

    fn draw(&self, frame: &mut Frame, cx: u16, row: u16, fade: f32) {
        let chars: Vec<char> = self.text.chars().collect();
        let left = cx.saturating_sub(chars.len() as u16 / 2);
        let paint = style(MAGENTA, fade * self.shimmer, true);

        let visible: String = chars[..self.shown].iter().collect();
        frame.buffer_mut().set_string(left, row, &visible, paint);

        let cursor = if self.cursor { "█" } else { " " };
        frame.buffer_mut().set_string(left + self.shown as u16, row, cursor, paint);
    }
}

/// Writes logs one row at a time to describe the state of the initialization and handles
/// fading of the text once the initialization has bee finalized
struct Log<'f, 'b> {
    frame: &'f mut Frame<'b>,
    x: u16,
    y: u16,
    t: f32,
    fade: f32,
}

impl Log<'_, '_> {
    fn row(&mut self, appear: f32, resolved: bool, label: &str, value: &str) {
        let (x, y, fade) = (self.x, self.y, self.fade);
        self.y += 1;
        if self.t < appear {
            return;
        }

        if resolved {
            self.frame.buffer_mut().set_string(x, y, "[", style(GRAY, fade, false));
            self.frame.buffer_mut().set_string(x + 1, y, "ok", style(MAGENTA, fade, true));
            self.frame.buffer_mut().set_string(x + 3, y, "]", style(GRAY, fade, false));
            self.frame.buffer_mut().set_string(x + 17, y, value, style(WHITE, fade, false));
        } else {
            let spin = SPINNER[(self.t * 12.0) as usize % SPINNER.len()];
            self.frame.buffer_mut().set_string(
                x,
                y,
                format!("[{spin} ]"),
                style(MAGENTA, fade, true),
            );
        }

        self.frame.buffer_mut().set_string(x + 6, y, label, style(GRAY, fade, false));
    }
}
