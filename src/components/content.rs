use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};

use color_eyre::eyre::eyre;
use color_eyre::Result;
use figlet_rs::FIGfont;
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};

use super::{Card, CardGrid, Component};
use crate::action::Action;
#[cfg(feature = "blog")]
use crate::components::Post;
use crate::components::Tabs;

static BANNER: LazyLock<Vec<String>> = LazyLock::new(|| {
    FIGfont::from_content(include_str!("../../assets/drpepper.flf"))
        .expect("embedded figlet font is malformed")
        .convert("hiya!")
        .expect("figlet conversion produced no output")
        .to_string()
        .trim_end_matches('\n')
        .split('\n')
        .map(String::from)
        .collect()
});

#[allow(dead_code)]
pub(super) fn truncate(s: &str, max: usize) -> String {
    s.char_indices()
        .find(|(idx, ch)| idx + ch.len_utf8() > max)
        .map_or(s.to_string(), |(idx, _)| s[..idx].to_string() + "...")
}

pub struct Content {
    selected_tab: Arc<AtomicUsize>,
    projects: CardGrid<'static>,
    about_scroll: u16,
}

impl Content {
    pub fn new(selected_tab: Arc<AtomicUsize>) -> Self {
        let projects = CardGrid::new(Self::projects(), Arc::clone(&selected_tab));
        Self { selected_tab, projects, about_scroll: 0 }
    }

    fn on_about(&self) -> bool {
        self.selected_tab.load(Ordering::Relaxed) == Tabs::ABOUT
    }

    fn heading(label: &str, width: u16) -> Line<'static> {
        const RULE_END: usize = 62;
        let used = label.chars().count() + 3;
        let rule_len = RULE_END.min(width as usize).saturating_sub(used);

        Line::from(vec![
            Span::styled(
                format!("  {label} "),
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ),
            Span::styled("─".repeat(rule_len), Style::default().fg(Color::DarkGray)),
        ])
    }

    fn intro() -> Vec<Line<'static>> {
        vec![
            Line::from(vec![
                Span::from("i'm "),
                Span::styled(
                    "erica",
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                ),
                Span::from(" ("),
                Span::styled("she/they", Style::default().add_modifier(Modifier::ITALIC)),
                Span::from("). i build systems software,"),
            ]),
            Line::from("mostly in rust and luau, and you're reading this"),
            Line::from("over ssh, which covers most of what i'd claim in"),
            Line::from("a paragraph here anyway."),
        ]
    }

    #[rustfmt::skip]
    fn socials() -> Line<'static> {
        let sep = Span::styled("  ", Style::default().fg(Color::DarkGray));
        Line::from(vec![
            Span::styled(" ", Style::default().fg(Color::Cyan)),
            Span::from("hi@devcomp.xyz"), sep.clone(),
            Span::styled(" ", Style::default().fg(Color::LightMagenta)),
            Span::from("@CompeyDev"), sep.clone(),
            Span::styled(" ", Style::default().fg(Color::Blue)),
            Span::from("@devcomp.xyz"), sep,
            Span::styled(" ", Style::default().fg(Color::LightBlue)),
            Span::from("@DevComp_"),
        ])
        .style(Style::default().add_modifier(Modifier::BOLD))
    }

    #[rustfmt::skip]
    fn contributions() -> Vec<(&'static str, Style, &'static str)> {
        vec![
            ("lune-org/lune",            Style::new().fg(Color::LightMagenta), "a standalone luau runtime"),
            ("pesde-pkg/pesde",          Style::new().fg(Color::Yellow),       "a multi-runtime package manager for luau"),
            ("DiscordLuau/discord-luau", Style::new().fg(Color::Blue),         "a luau library for discord bots"),
        ]
    }

    fn about_body(&self, width: u16) -> Vec<Line<'static>> {
        let body = Style::default().fg(Color::White);
        let dim = Style::default().fg(Color::DarkGray);

        let mut lines = vec![Self::socials(), Line::default()];

        lines.push(Self::heading("now", width));
        lines.push(Line::styled("  (what i'm working on this month)", dim));
        lines.push(Line::default());
        // TODO: fetch something new every month, maybe using atproto or github

        lines.push(Self::heading("i maintain / contribute to", width));
        let name_col = Self::contributions()
            .iter()
            .map(|(name, ..)| name.chars().count())
            .max()
            .unwrap_or(0);
        for (name, style, blurb) in Self::contributions() {
            let pad = " ".repeat(name_col.saturating_sub(name.chars().count()));
            lines.push(Line::from(vec![
                Span::styled("   • ", dim),
                Span::styled(name, style.add_modifier(Modifier::BOLD)),
                Span::styled(format!("{pad}   {blurb}"), body),
            ]));
        }

        lines.push(Line::styled("    my own things are on the next tab →", dim));
        lines.push(Line::default());

        lines.push(Self::heading("this thing", width));
        lines.push(Line::from(vec![
            Span::styled("  ", dim),
            Span::styled(env!("PKG_LOC"), Style::default().fg(Color::LightRed)),
            Span::styled(" lines of rust. russh for transport, ratatui for drawing,", body),
        ]));
        lines.push(Line::styled(
            "  atproto for the blog. no http except the landing page.",
            body,
        ));
        lines
            .push(Line::styled(format!("  agpl-3.0 · {}", env!("CARGO_PKG_REPOSITORY")), dim));
        lines.push(Line::default());

        lines.push(Line::styled("  (availability)", dim));
        lines.extend(vec![
            Line::from(
                "  currently a student, and not looking for work. feel free to reach out",
            ),
            Line::from("  via any one of my above socials if you'd like to have a chat!"),
        ]);
        lines.push(Line::default());

        lines.push(Line::styled(
            "  huge fan of 8 bit aesthetics, seals and the color purple <3",
            Style::default().fg(Color::LightMagenta),
        ));

        lines
    }

    /// The project cards.
    #[rustfmt::skip]
    fn projects() -> Vec<Card<'static>> {
        // TODO: we could have an array of repos and fetch details lazily at runtime
        vec![
            Card { title: " 0x5eal/luau-unzip", description: "Unzip implementation in pure Luau" },
            Card { title: " CompeyDev/discord-status-action", description: "GitHub action to update your discord status in a file using the Lanyard API" },
            Card { title: " CompeyDev/bad-apple-efi", description: "An EFI application to play the silly video" },
            Card { title: " CompeyDev/lei", description: "🌸 A collection of Go bindings to Luau" },
            Card { title: " 0x5eal/wg-lua", description: "A Lua implementation of the wireguard keygen algorithm" },
            Card { title: " 0x5eal/semver-luau", description: "Strongly typed semver parser for Luau" },
            Card { title: " CompeyDev/elytra-lock-fabric", description: "Client-side fabric mod to lock elytra usage using a keybind" },
            Card { title: " CompeyDev/touch-grass-reminder", description: "Client-side quilt mod which warns players when they have been excessively playing Minecraft" },
            Card { title: " CompeyDev/stinky-mod", description: "Server-side fabric mod featuring (mostly) customizable randomized join, leave, death, and MOTD messages" },
            Card { title: " CompeyDev/lune-luau-template", description: "A simple template for initializing Luau projects with Lune" },
            Card { title: " CompeyDev/frktest-pesde", description: "A basic test framework for Lune (now with pesde support!)" },
            Card { title: " CompeyDev/cull-less-leaves", description: "1.21 release fork | Cull leaves while looking hot!" },
            Card { title: " CompeyDev/setup-rokit", description: "GitHub action to install and run rokit; a toolchain manager" },
            Card { title: " CompeyDev/fxtwitter-docker", description: "Dockerified fork of fxtwitter | Fix broken Twitter/X embeds!" },
        ]
    }

    /// Generate the content for the "Blog" tab
    #[cfg(feature = "blog")]
    pub async fn blog_content(&self) -> Result<Vec<Post>> {
        Ok(crate::atproto::blog::get_all_posts()
            .await?
            .iter()
            .map(|post| Arc::new(post.clone()))
            .collect())
    }

    fn draw_about(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let banner_width = BANNER
            .iter()
            .map(|line| line.chars().count())
            .max()
            .ok_or(eyre!("Figlet banner produced no lines"))?
            as u16;
        let banner_height = BANNER.len() as u16;

        let [head, _, body] = Layout::vertical([
            Constraint::Length(banner_height),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(area);

        let [banner_area, intro_area] =
            Layout::horizontal([Constraint::Length(banner_width + 2), Constraint::Min(0)])
                .areas(head);

        let banner = BANNER.iter().map(|line| Line::from(line.clone()));
        frame.render_widget(
            Paragraph::new(banner.collect::<Vec<_>>())
                .style(Style::default().add_modifier(Modifier::BOLD)),
            banner_area,
        );

        let intro = Self::intro();
        let intro_y = intro_area.y + banner_height.saturating_sub(intro.len() as u16);
        frame.render_widget(
            Paragraph::new(intro).wrap(Wrap { trim: false }),
            Rect { y: intro_y, height: intro_area.bottom() - intro_y, ..intro_area },
        );

        // Scrolling if its larger the frame size
        let text_width = body.width.saturating_sub(1);
        let lines = self.about_body(text_width);
        let overflow = (lines.len() as u16).saturating_sub(body.height);
        self.about_scroll = self.about_scroll.min(overflow);

        let [text_area, bar_area] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(body);

        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((self.about_scroll, 0)),
            text_area,
        );

        if overflow > 0 {
            let mut state =
                ScrollbarState::new(overflow as usize).position(self.about_scroll as usize);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .track_symbol(Some("│"))
                    .thumb_symbol("┃")
                    .style(Style::default().fg(Color::DarkGray))
                    .thumb_style(Style::default().fg(Color::Magenta)),
                bar_area,
                &mut state,
            );
        }

        Ok(())
    }
}

impl Component for Content {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            Action::SelectNext if self.on_about() => {
                self.about_scroll = self.about_scroll.saturating_add(1)
            }
            Action::SelectPrev if self.on_about() => {
                self.about_scroll = self.about_scroll.saturating_sub(1)
            }
            _ => {}
        }

        self.projects.update(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        match self.selected_tab.load(Ordering::Relaxed) {
            Tabs::ABOUT => self.draw_about(frame, area)?,
            Tabs::PROJECTS => self.projects.draw(frame, area)?,
            // The blog tab is drawn by `App`, which owns the split-pane layout
            _ => {}
        }

        Ok(())
    }
}
