use std::time::Instant;

use color_eyre::Result;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::Component;
use crate::action::Action;

// Dimensions
const WIDTH: usize = 24;
const HEIGHT: u16 = 5;

/// Rows above Debbie, shared by the sleep z's and the meow bubble
const EEPY_ROWS: u16 = 3;

/// Describes any of Debbie's habits in terms of its intervals.
///
/// Every `every` seconds, `at` seconds into the cycle, she does something
/// for `lasts` seconds.
struct Habit {
    every: f32,
    at: f32,
    lasts: f32,
}

impl Habit {
    /// A habit indulged just once, `at` seconds in, rather than on a cycle
    const fn once(at: f32, lasts: f32) -> Self {
        Self { every: f32::INFINITY, at, lasts }
    }

    /// Whether the habit is being indulged at `t`
    fn active(&self, t: f32) -> bool {
        self.elapsed(t).is_some()
    }

    /// Seconds since the habit started, if it is active at `t`
    fn elapsed(&self, t: f32) -> Option<f32> {
        let dt = t % self.every - self.at;
        (0.0..self.lasts).contains(&dt).then_some(dt)
    }
}

// Sleep
const DOZE_AFTER: f32 = 30.0;

const BREATH: f32 = 1.2;
const BREATH_IN: f32 = 0.45;
const SIGH_EVERY: u32 = 8;
const Z_LIFETIME: f32 = 2.4;

const EAR_TWITCH_R: Habit = Habit { every: 7.3, at: 0.0, lasts: 0.18 };
const EAR_TWITCH_L: Habit = Habit { every: 11.1, at: 0.0, lasts: 0.18 };

const TOE_TWITCH: Habit = Habit { every: 13.7, at: 5.0, lasts: 0.35 };

// Stretching
const SQUASH_FOR: f32 = 0.5;
const HALF_FOR: f32 = 0.7;
const BOW_FOR: f32 = 2.0;
const UNHALF_FOR: f32 = 0.5;
const LAND_FOR: f32 = 0.3;
const STRETCH_SEQ: f32 = SQUASH_FOR + HALF_FOR + BOW_FOR + UNHALF_FOR + LAND_FOR;

/// The stretch timeline, each keyframe and how long it holds
const STRETCH_KEYFRAMES: [(f32, Pose); 5] = [
    (SQUASH_FOR, Pose::Squash),
    (HALF_FOR, Pose::Half),
    (BOW_FOR, Pose::Bow),
    (UNHALF_FOR, Pose::Half),
    (LAND_FOR, Pose::Squash),
];

// Awake
const STRETCH: Habit = Habit { every: 53.0, at: 29.0, lasts: STRETCH_SEQ };
const STRETCH_IDLE_MAX: f32 = 20.0;

// Waking up
const STIR_UNTIL: f32 = 0.8;
const STARTLED_UNTIL: f32 = 2.3;
const ASSESS_UNTIL: f32 = 3.1;
const MEOW: Habit = Habit::once(4.0, 1.6);

// Facial behavior
const BLINK: Habit = Habit { every: 3.2, at: 0.0, lasts: 0.14 };
const DOUBLE_BLINK: Habit = Habit { every: BLINK.every * 3.0, at: 0.4, lasts: BLINK.lasts };
const SLOWBLINK: Habit = Habit { every: 17.0, at: 13.0, lasts: 0.7 };
const BLEP: Habit = Habit { every: 27.0, at: 21.0, lasts: 0.8 };
const FLICK: Habit = Habit { every: 2.6, at: 0.3, lasts: 0.35 };

const SLEEPY_FOR: f32 = 1.5; // right before sleeping

const EARS: &str = "/\\_/\\";
const EARS_L: &str = "|\\_/\\";
const EARS_R: &str = "/\\_/|";

const MEOW_BUBBLE: [&str; 3] = ["  ╭────────╮", "  │ *meow* │", "  ╰──v─────╯"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Eyes slowly open, still flat
    Stirring,
    /// Eyes wide, confused
    Startled,
    /// Eyes settle, wakes up to loaf
    Assessing,
    /// Casual stretch once in a while
    Stretching,
    /// Eyes heavy, about to fall asleep
    Sleepy,
    Awake,
    Asleep,
}

/// Debbie stretches once in a while. Each stretch is composed of three keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pose {
    Squash,
    Half,
    Bow,
}

/// Deborah Whiskersworth III, whom I fondly refer to as Debbie, is a resident
/// specimen of *felis catus*, perched on the bottom right corner of the frame.
///
/// She exhibits a full repertoire of her species, and is rather "eepy", dozing
/// off every few seconds and ocassionally performing full body stretches, when
/// not in her base loaf configuration.
pub struct Debbie {
    /// When Debbie last came out of sleep
    woke_at: Instant,
    /// When the user last provided any input. When idle for too long, Debbie
    /// goes to sleep
    last_activity: Instant,
}

impl Default for Debbie {
    fn default() -> Self {
        // Both clocks start at connection, meaning the wake process is displayed
        // at the first render
        Self { woke_at: Instant::now(), last_activity: Instant::now() }
    }
}

/// Debbie at a single instant, including her clocks, the state and any
/// stretch keyframe fixed from them. A frame's content is derived purely
/// from this.
struct Moment {
    t_wake: f32,
    t_idle: f32,
    state: State,
    pose: Option<Pose>,
}

impl Moment {
    fn at(t_wake: f32, t_idle: f32) -> Self {
        let stretch = STRETCH.elapsed(t_wake).filter(|_| t_idle < STRETCH_IDLE_MAX);

        let state = match (t_wake, t_idle) {
            (_, idle) if idle >= DOZE_AFTER => State::Asleep,
            (wake, _) if wake < STIR_UNTIL => State::Stirring,
            (wake, _) if wake < STARTLED_UNTIL => State::Startled,
            (wake, _) if wake < ASSESS_UNTIL => State::Assessing,
            _ if stretch.is_some() => State::Stretching,
            (_, idle) if idle >= DOZE_AFTER - SLEEPY_FOR => State::Sleepy,
            _ => State::Awake,
        };

        let pose = match state {
            State::Stretching => stretch.map(Self::stretch_pose),
            _ => None,
        };

        Self { t_wake, t_idle, state, pose }
    }

    /// Which keyframe the stretch sequence shows at `t` seconds
    fn stretch_pose(mut t: f32) -> Pose {
        for (length, pose) in STRETCH_KEYFRAMES {
            if t < length {
                return pose;
            }

            t -= length;
        }

        Pose::Squash
    }

    /// How long Debbie has been asleep, zero when awake
    fn asleep_for(&self) -> f32 {
        (self.t_idle - DOZE_AFTER).max(0.0)
    }

    fn meowing(&self) -> bool {
        self.state == State::Awake && MEOW.active(self.t_wake)
    }

    //
    // Body Parts
    //

    fn eyes(&self) -> &'static str {
        use State::*;

        // slow blink, periodic blink or double blink
        let blinking = SLOWBLINK.active(self.t_wake)
            || BLINK.active(self.t_wake)
            || DOUBLE_BLINK.active(self.t_wake);

        match self.state {
            Asleep | Sleepy | Stirring | Stretching => "-.-",
            Startled => "O.O",
            Assessing => "o.o",
            Awake if self.meowing() => "^.^",
            Awake if blinking => "-.-",
            Awake => "o.o",
        }
    }

    fn muzzle(&self) -> &'static str {
        use State::*;

        match (self.pose, self.state) {
            (Some(Pose::Bow), _) => "- o -",
            _ if self.meowing() => "> w <",
            (_, Awake) if BLEP.active(self.t_wake) => "> u <",
            (_, Asleep | Stirring | Stretching) => "- Y -",
            _ => "> Y <",
        }
    }

    fn ears(&self) -> &'static str {
        let asleep_for = self.asleep_for();
        match self.state {
            State::Asleep if EAR_TWITCH_L.active(asleep_for) => EARS_L,
            State::Asleep if EAR_TWITCH_R.active(asleep_for) => EARS_R,
            _ => EARS,
        }
    }

    fn tail(&self) -> &'static str {
        let flicking = self.state == State::Awake && FLICK.active(self.t_wake);
        match flicking {
            true => "~'",
            false => "~,",
        }
    }

    fn toe(&self) -> &'static str {
        let twitching = self.state == State::Asleep && TOE_TWITCH.active(self.asleep_for());
        match twitching {
            true => ">",
            false => ",",
        }
    }

    fn body(&self) -> Vec<String> {
        match self.pose {
            Some(Pose::Bow) => self.bow(),
            Some(Pose::Half) => self.half(),
            Some(Pose::Squash) => self.flat(false),
            None => match self.state {
                State::Asleep | State::Stirring | State::Startled | State::Assessing => {
                    self.flat(true)
                }
                _ => self.loaf(),
            },
        }
    }

    /// Full body art for when Debbie is awake
    fn loaf(&self) -> Vec<String> {
        vec![
            format!("    {}", self.ears()),
            format!("  =( {} )----.", self.eyes()),
            format!("    {}      `.", self.muzzle()),
            "   (             )".to_string(),
            format!("    `,__{}____,__,'{}", self.toe(), self.tail()),
        ]
    }

    /// Full body art for when Debbie is asleep. Flat, head on her paws.
    ///
    /// `wrapped` curls the tail around the front.
    fn flat(&self, wrapped: bool) -> Vec<String> {
        let asleep_for = self.asleep_for();
        let sighing = self.state == State::Asleep
            && (asleep_for % BREATH) / BREATH < BREATH_IN
            && (asleep_for / BREATH) as u32 % SIGH_EVERY == SIGH_EVERY - 1;

        let bottom = match wrapped {
            true => format!("  ~-`,__{}____,__,'", self.toe()),
            false => format!("    `,__{}____,__,'~,", self.toe()),
        };

        [
            format!("    {}", self.ears()),
            format!("  =( {} )----.", self.eyes()),
            format!("    {}      `.", self.muzzle()),
        ]
        .into_iter()
        .chain(sighing.then(|| "   ;             )".to_string()))
        .chain([bottom])
        .collect()
    }

    //
    // Stretch Phases
    //

    fn half(&self) -> Vec<String> {
        vec![
            "     /\\_/\\".to_string(),
            format!("   =( {} )_,   ,--.", self.eyes()),
            format!("     {}  `-(     )", self.muzzle()),
            format!(" __(_)________(_____,'{}", self.tail()),
        ]
    }

    fn bow(&self) -> Vec<String> {
        vec![
            "     /\\_/\\      ,--.".to_string(),
            format!("   =( {} )_,-`     \\", self.eyes()),
            format!("     {}    (      )", self.muzzle()),
            format!(" __(_)________(_____,'{}", self.tail()),
        ]
    }

    /// The z's start at the head, increasing in size as they go higher
    fn zs(&self) -> Vec<Z> {
        let asleep_for = self.asleep_for();
        let cycle = BREATH * 3.0;
        (0..3u16)
            .filter_map(|index| {
                let age = (asleep_for + cycle - BREATH * f32::from(index)) % cycle;
                (age < Z_LIFETIME).then(|| {
                    let progress = age / Z_LIFETIME;
                    let glyph = match progress < 0.5 {
                        true => 'z',
                        false => 'Z',
                    };

                    // Fade them out slowly
                    let style = match (0.15..=0.6).contains(&progress) {
                        true => Style::default().fg(Color::Magenta).bold(),
                        false => Style::default().fg(Color::DarkGray),
                    };

                    let row = (EEPY_ROWS - 1) - (progress * f32::from(EEPY_ROWS)) as u16;
                    Z { column: 6 + (progress * 2.0) as u16, row, glyph, style }
                })
            })
            .collect()
    }

    /// The band of reserved rows above Debbie for displaying the z's or a meow.
    ///
    /// `pad` shifts the band along with the body's right-alignment.
    fn band(&self, pad: usize) -> Vec<Line<'static>> {
        match (self.meowing(), self.state) {
            (true, _) => MEOW_BUBBLE
                .iter()
                .map(|row| {
                    let row = format!("{}{row}", " ".repeat(pad));
                    Line::styled(
                        format!("{row:<width$}", width = WIDTH),
                        Style::default().fg(Color::DarkGray),
                    )
                })
                .collect(),
            (_, State::Asleep) => {
                let zs = self.zs();
                (0..EEPY_ROWS).map(|row| Z::line(&zs, row, pad as u16)).collect()
            }
            _ => (0..EEPY_ROWS).map(|_| Line::raw(" ".repeat(WIDTH))).collect(),
        }
    }
}

/// One z rising above Debbie's head while she sleeps
struct Z {
    column: u16,
    /// Row within the band, counted from its top
    row: u16,
    glyph: char,
    style: Style,
}

impl Z {
    /// One row of the band above Debbie, with any z's placed into it.
    ///
    /// `offset` shifts the z's along with the body's right-alignment, so they
    /// keep rising from above the head.
    fn line(zs: &[Z], row: u16, offset: u16) -> Line<'static> {
        let mut spans = Vec::new();
        let mut cursor = 0u16;
        let mut row_zs = zs.iter().filter(|z| z.row == row).collect::<Vec<_>>();
        row_zs.sort_by_key(|z| z.column);

        for z in row_zs {
            let column = z.column + offset;
            if column < cursor {
                continue;
            }

            spans.push(Span::raw(" ".repeat(usize::from(column - cursor))));
            spans.push(Span::styled(z.glyph.to_string(), z.style));
            cursor = column + 1;
        }

        spans.push(Span::raw(" ".repeat(WIDTH - usize::from(cursor))));
        Line::from(spans)
    }
}

impl Debbie {
    // Exported constants as public for external consumers
    pub const Z_ROWS: u16 = EEPY_ROWS;
    pub const HEIGHT: u16 = HEIGHT;

    pub fn new() -> Self {
        Self::default()
    }

    fn now(&self) -> Moment {
        Moment::at(
            self.woke_at.elapsed().as_secs_f32(),
            self.last_activity.elapsed().as_secs_f32(),
        )
    }

    fn border_row(row: String) -> Line<'static> {
        let chars = row.chars().map(|c| if c == '_' { '─' } else { c }).collect::<Vec<_>>();
        let start = chars.iter().position(|c| *c != ' ').unwrap_or(0);
        let end = chars.iter().rposition(|c| *c != ' ').map_or(0, |i| i + 1);

        let border = Style::default().fg(Color::DarkGray);
        Line::from(vec![
            Span::styled("─".repeat(start), border),
            Span::styled(
                chars[start..end].iter().collect::<String>(),
                Style::default().fg(Color::Magenta).bold(),
            ),
            Span::styled("─".repeat(chars.len() - end), border),
        ])
    }
}

impl Component for Debbie {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick | Action::Render => {}
            _ => {
                // Any interaction should "wake" Debbie by resetting the wake time
                if self.last_activity.elapsed().as_secs_f32() >= DOZE_AFTER {
                    self.woke_at = Instant::now();
                }

                self.last_activity = Instant::now();
            }
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        if area.width < WIDTH as u16 + 2 || area.height < HEIGHT {
            return Ok(());
        }

        let now = self.now();
        let body = now.body();
        let body_pad =
            WIDTH - body.iter().map(|row| row.chars().count()).max().unwrap_or(WIDTH);

        let mut lines = Vec::new();
        if area.height >= HEIGHT + EEPY_ROWS {
            lines.extend(now.band(body_pad));
        }

        // Pad against the tallest frame, so even a shorter pose clears the rows a previous
        // taller frame wrote
        for _ in body.len()..HEIGHT as usize {
            lines.push(Line::raw(" ".repeat(WIDTH)));
        }

        let last = body.len() - 1;
        for (i, row) in body.into_iter().enumerate() {
            let row = format!("{}{row}", " ".repeat(body_pad));
            let row = format!("{row:<width$}", width = WIDTH);
            lines.push(if i == last {
                Self::border_row(row)
            } else {
                Line::styled(row, Style::default().fg(Color::Magenta).bold())
            });
        }

        let height = lines.len() as u16;
        let corner = Rect {
            x: area.right().saturating_sub(WIDTH as u16 + 1),
            y: area.bottom().saturating_sub(height),
            width: WIDTH as u16,
            height,
        };

        frame.render_widget(Paragraph::new(Text::from(lines)), corner);
        Ok(())
    }
}
