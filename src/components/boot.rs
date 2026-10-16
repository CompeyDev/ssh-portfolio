use std::net::IpAddr;
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
    greeting: &'static str,
}

impl Boot {
    pub fn new(
        terminal_info: Arc<RwLock<TerminalInfo>>,
        username: Arc<RwLock<Option<String>>>,
        client_ip: Option<IpAddr>,
    ) -> Self {
        Self { terminal_info, username, greeting: greeting::word(client_ip) }
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
        if let Some(greeting) = Greeting::at(t, self.greeting, name) {
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
    fn at(t: f32, greeting: &str, name: Option<String>) -> Option<Self> {
        if t < T_GREET {
            return None;
        }

        let name = name?;
        let text = greeting.replacen("{user}", &truncate(&name, 24), 1);
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

#[cfg(feature = "localtime")]
mod greeting {
    use std::net::IpAddr;
    use std::sync::LazyLock;

    use iptz::Table;
    use jiff::civil::Weekday;
    use jiff::tz::TimeZone;
    use jiff::Zoned;

    const IPTZ_DB: &[u8] = include_bytes!("../../assets/iptz.bin");
    static TABLE: LazyLock<Option<Table>> = LazyLock::new(|| Table::decode(IPTZ_DB).ok());

    struct Entry {
        spicy: bool,
        when: fn(&Zoned) -> bool,
        text: &'static str,
    }

    #[rustfmt::skip]
    const GREETINGS: &[Entry] = &[
        // morning
        Entry { spicy: false, when: |z| (5..=11).contains(&z.hour()), text: "good morning, {user}!" },
        Entry { spicy: false, when: |z| (5..=11).contains(&z.hour()), text: "top of the morning, {user}!" },
        Entry { spicy: false, when: |z| (5..=11).contains(&z.hour()), text: "morning, {user}!" },
        Entry { spicy: false, when: |z| (5..=9).contains(&z.hour()), text: "coffee first, {user}!" },
        Entry { spicy: false, when: |z| (5..=8).contains(&z.hour()), text: "up and at 'em, {user}!" },
        Entry { spicy: false, when: |z| (5..=6).contains(&z.hour()), text: "you're up early, {user}?" },

        // afternoon
        Entry { spicy: false, when: |z| (12..=16).contains(&z.hour()), text: "good afternoon, {user}!" },
        Entry { spicy: false, when: |z| (12..=16).contains(&z.hour()), text: "afternoon, {user}!" },
        Entry { spicy: false, when: |z| z.hour() == 12, text: "high noon, {user}." },
        Entry { spicy: false, when: |z| (13..=16).contains(&z.hour()), text: "back at it, {user}!" },
        Entry { spicy: false, when: |z| (14..=16).contains(&z.hour()), text: "afternoon slump, {user}?" },
        Entry { spicy: false, when: |z| (15..=16).contains(&z.hour()), text: "still at it, {user}?" },

        // evening
        Entry { spicy: false, when: |z| (17..=21).contains(&z.hour()), text: "good evening, {user}!" },
        Entry { spicy: false, when: |z| (17..=21).contains(&z.hour()), text: "evening, {user}!" },
        Entry { spicy: false, when: |z| (17..=18).contains(&z.hour()), text: "clocking off, {user}!" },
        Entry { spicy: false, when: |z| (18..=21).contains(&z.hour()), text: "long day, {user}?" },
        Entry { spicy: false, when: |z| (18..=21).contains(&z.hour()), text: "done for the day, {user}?" },
        Entry { spicy: false, when: |z| (19..=21).contains(&z.hour()), text: "winding down, {user}!" },

        // night
        Entry { spicy: false, when: |z| matches!(z.hour(), 22..=23 | 0..=4), text: "night owl, {user}!" },
        Entry { spicy: false, when: |z| matches!(z.hour(), 23 | 0..=4), text: "still up, {user}?" },
        Entry { spicy: false, when: |z| matches!(z.hour(), 0..=3), text: "burning the midnight oil, {user}?" },
        Entry { spicy: false, when: |z| matches!(z.hour(), 1..=4), text: "can't sleep either, {user}?" },
        Entry { spicy: false, when: |z| matches!(z.hour(), 1..=4), text: "shouldn't you be asleep, {user}?" },

        // easter eggs (higher priority)
        Entry { spicy: true, when: |z| z.hour() == 0, text: "the witching hour, {user}." },
        Entry { spicy: true, when: |z| z.hour() == 3, text: "it's 3am, {user}. you good?" },
        Entry { spicy: true, when: |z| z.weekday() == Weekday::Friday && z.hour() >= 17, text: "it's friday, {user}. go touch grass." },
    ];

    pub fn word(ip: Option<IpAddr>) -> &'static str {
        resolve(ip).unwrap_or_else(super::variation::fallback)
    }

    fn resolve(ip: Option<IpAddr>) -> Option<&'static str> {
        let Some(IpAddr::V4(v4)) = ip else { return None };
        let zone = TABLE.as_ref()?.lookup(v4)?;
        let tz = TimeZone::get(zone).ok()?;
        let now = Zoned::now().with_time_zone(tz);

        // Prefer easter eggs, if any
        let candidates = GREETINGS.iter().filter(|g| (g.when)(&now));
        let use_specials = candidates.clone().any(|g| g.spicy);
        let texts =
            candidates.filter(|g| g.spicy == use_specials).map(|g| g.text).collect::<Vec<_>>();

        super::variation::pick(&texts)
    }
}

#[cfg(not(feature = "localtime"))]
mod greeting {
    use std::net::IpAddr;

    pub fn word(_ip: Option<IpAddr>) -> &'static str {
        super::variation::fallback()
    }
}

pub(super) mod variation {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rand::seq::IndexedRandom;

    // Prevent a greeting being repeated multiple times across visits. Since all our greetings
    // are 'static strings, we can simply track by pointer identity to prevent an additional
    // allocation
    static LAST: AtomicUsize = AtomicUsize::new(0);

    /// Picks a random entry from `options`, preventing a repeated value across invocations
    pub(super) fn pick(options: &[&'static str]) -> Option<&'static str> {
        let last = LAST.load(Ordering::Relaxed);
        let fresh = options
            .iter()
            .copied()
            .filter(|s| s.as_ptr() as usize != last)
            .collect::<Vec<&'static str>>();

        let chosen = fresh
            .choose(&mut rand::rng())
            .or_else(|| options.choose(&mut rand::rng()))
            .copied();

        if let Some(c) = chosen {
            LAST.store(c.as_ptr() as usize, Ordering::Relaxed);
        }

        chosen
    }

    /// Pick a random non customized greeting
    pub(super) fn fallback() -> &'static str {
        pick(&[
            "welcome, {name}!",
            "hiya, {name}!",
            "hey there, {name}!",
            "good to see you, {name}!",
            "howdy, {name}!",
            "well met, {name}!",
        ])
        .unwrap() // NOTE: safe to unwrap since never None, always a populated array
    }
}
