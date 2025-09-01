use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use color_eyre::eyre::eyre;
use color_eyre::Result;
use figlet_rs::FIGfont;
use ratatui::prelude::*;
use ratatui::widgets::*;
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::action::Action;
use crate::components::Card;
#[cfg(feature = "blog")]
use crate::components::Post;
use crate::config::Config;

#[allow(dead_code)]
pub(super) fn truncate(s: &str, max: usize) -> String {
    s.char_indices()
        .find(|(idx, ch)| idx + ch.len_utf8() > max)
        .map_or(s.to_string(), |(idx, _)| s[..idx].to_string() + "...")
}

#[derive(Default)]
pub struct Content {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    selected_tab: Arc<AtomicUsize>,
}

// TODO: Use layouts and make this ugly

impl Content {
    pub fn new(selected_tab: Arc<AtomicUsize>) -> Self {
        Self { selected_tab, ..Default::default() }
    }

    /// Generate the content for the "About" tab
    fn about_content(&self, area: Rect) -> Result<Vec<Line<'static>>> {
        let greetings_header =
            FIGfont::from_content(include_str!("../../assets/drpepper.flf"))
                .map_err(|err| eyre!(err))?
                .convert("hiya!")
                .ok_or(eyre!("Failed to create figlet header for about page"))?
                .to_string();

        let lines: Vec<String> =
            greetings_header.trim_end_matches('\n').split('\n').map(String::from).collect();

        let mut content = lines
            .iter()
            .enumerate()
            .map(|(pos, line)| {
                if pos == lines.len() - 3 {
                    return Line::from(vec![
                        Span::from(" "),
                        Span::from(line.clone()),
                        Span::from("  I'm Erica ("),
                        Span::styled(
                            "she/they",
                            Style::default().add_modifier(Modifier::ITALIC),
                        ),
                        Span::from("), and I make scalable systems or something. IDFK."),
                    ]);
                } else if pos == lines.len() - 2 {
                    return Line::from(vec![
                        Span::from(" "),
                        Span::from(line.clone()),
                        Span::from("      "),
                        //  hi@devcomp.xyz   @CompeyDev   @devcomp.xyz   @DevComp_
                        Span::styled(" ", Style::default().fg(Color::Cyan)),
                        Span::from("hi@devcomp.xyz"),
                        Span::from("  "),
                        Span::styled(" ", Style::default().fg(Color::LightMagenta)),
                        Span::from("@CompeyDev"),
                        Span::from("  "),
                        Span::styled(" ", Style::default().fg(Color::Blue)),
                        Span::from("@devcomp.xyz"),
                        Span::from("  "),
                        Span::styled(" ", Style::default().fg(Color::LightBlue)),
                        Span::from("@DevComp_"),
                    ])
                    .add_modifier(Modifier::BOLD);
                }

                Line::raw(format!(" {line}"))
                    .style(Style::default().add_modifier(Modifier::BOLD))
            })
            .collect::<Vec<Line<'static>>>();

        content.extend(vec![
            Line::default(),
            Line::from(vec![
                Span::from(" "),
                Span::from("I specialize in systems programming, primarily in "),
                Span::styled(
                    "Rust 🦀",
                    Style::default()
                        .fg(Color::LightRed)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                ),
                Span::from(" and "),
                Span::styled(
                    "Luau 🦭",
                    Style::default()
                        .fg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC),
                ),
                Span::from("."),
            ]),
            Line::from(""),
            Line::from(
                " I am an avid believer of open-source software, and contribute to a few \
                 projects such as:",
            ),
        ]);

        let projects = vec![
            (
                Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD),
                "lune-org/lune: A standalone Luau runtime",
            ),
            (
                Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
                "DiscordLuau/discord-luau: A Luau library for creating Discord bots, powered \
                 by Lune",
            ),
            (
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                "pesde-pkg/pesde: A multi-runtime package manager for the Luau programming language",
            ),
        ];

        for (style, project) in projects {
            let parts: Vec<&str> = project.splitn(2, ':').collect();
            let (left, right) =
                if parts.len() == 2 { (parts[0], parts[1]) } else { (project, "") };

            let formatted_left = Span::styled(left, style);

            let bullet = " • ";
            let indent = "   ";

            let first_line = if project.len() > area.width as usize - bullet.len() {
                let split_point = project
                    .char_indices()
                    .take_while(|(i, _)| *i < area.width as usize - bullet.len())
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(project.len());
                let (first, rest) = project.split_at(split_point);
                content.push(Line::from(vec![
                    Span::from(bullet),
                    formatted_left,
                    Span::from(":"),
                    Span::styled(
                        first.trim_start_matches(format!("{left}:").as_str()).to_string(),
                        Style::default().fg(Color::White),
                    ),
                ]));
                rest.to_string()
            } else {
                content.push(Line::from(vec![
                    Span::from(bullet),
                    formatted_left,
                    Span::from(":"),
                    Span::styled(right.to_string(), Style::default().fg(Color::White)),
                ]));
                String::new()
            };

            let mut remaining_text = first_line;
            while !remaining_text.is_empty() {
                if remaining_text.len() > area.width as usize - indent.len() {
                    let split_point = remaining_text
                        .char_indices()
                        .take_while(|(i, _)| *i < area.width as usize - indent.len())
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(remaining_text.len());
                    let (first, rest) = remaining_text.split_at(split_point);
                    content.push(Line::from(vec![
                        Span::from(indent),
                        Span::styled(first.to_string(), Style::default().fg(Color::White)),
                    ]));
                    remaining_text = rest.to_string();
                } else {
                    content.push(Line::from(vec![
                        Span::from(indent),
                        Span::styled(
                            remaining_text.clone(),
                            Style::default().fg(Color::White),
                        ),
                    ]));
                    remaining_text.clear();
                }
            }
        }

        content.extend(vec![
            Line::from(""),
            Line::from(
                " I am also a fan of the 8 bit aesthetic and think seals are super adorable \
                 :3",
            ),
        ]);

        Ok(content)
    }

    /// Generate the content for the "Projects" tab
    #[rustfmt::skip]
    fn projects_content(&self) -> Vec<Card<'static>> {
        vec![
            Card { title: " 0x5eal/luau-unzip", description: "Unzip implementation in pure Luau" },
            Card {
                title: " CompeyDev/discord-status-action",
                description: "GitHub action to update your discord status in a file using the Lanyard API",
            },
            Card { title: " CompeyDev/bad-apple-efi", description: "An EFI application to play the silly video" },
            Card { title: " CompeyDev/lei", description: "🌸 A collection of Go bindings to Luau" },
            Card { title: " 0x5eal/wg-lua", description: "A Lua implementation of the wireguard keygen algorithm" },
            Card { title: " 0x5eal/semver-luau", description: "Strongly typed semver parser for Luau" },
            Card { title: " CompeyDev/elytra-lock-fabric", description: "Client-side fabric mod to lock elytra usage using a keybind" },
            Card { 
                title: " CompeyDev/touch-grass-reminder",
                description: "Client-side quilt mod which warns players when they have been excessively playing Minecraft"
            },
            Card {
                title: " CompeyDev/stinky-mod",
                description: "Server-side fabric mod featuring (mostly) customizable randomized join, leave, death, and MOTD messages",
            },
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
}

impl Component for Content {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> Result<()> {
        self.config = config;
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
        let selected_tab = self.selected_tab.load(Ordering::Relaxed);

        // Create the border lines
        let mut border_top = Line::default();
        border_top.spans.push(Span::styled("╭", Style::default().fg(Color::DarkGray)));

        let devcomp_width = 13;
        border_top.spans.push(Span::styled(
            "─".repeat(devcomp_width),
            Style::default().fg(Color::DarkGray),
        ));

        let tabs = ["about", "projects", "blog"];
        let mut current_pos = 1 + devcomp_width;

        for (i, &tab) in tabs.iter().enumerate() {
            let (char, style) = if i == self.selected_tab.load(Ordering::Relaxed) {
                ("━", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
            } else {
                ("─", Style::default().fg(Color::DarkGray))
            };

            let default_style = Style::default().fg(Color::DarkGray);

            border_top.spans.push(Span::styled("┴", default_style));
            border_top.spans.push(Span::styled("─", default_style));
            border_top.spans.push(Span::styled(char.repeat(tab.len()), style));
            border_top.spans.push(Span::styled("─", default_style));
            border_top.spans.push(Span::styled("┴", default_style));

            current_pos += tab.len() + 4;
        }

        border_top.spans.push(Span::styled(
            "─".repeat(area.width as usize - current_pos - 1),
            Style::default().fg(Color::DarkGray),
        ));

        border_top.spans.push(Span::styled("╮", Style::default().fg(Color::DarkGray)));

        let border_bottom = Line::from(Span::styled(
            "╰".to_owned() + &"─".repeat(area.width as usize - 2) + "╯",
            Style::default().fg(Color::DarkGray),
        ));

        let border_left = Span::styled("│", Style::default().fg(Color::DarkGray));
        let border_right = Span::styled("│", Style::default().fg(Color::DarkGray));

        // Render the content
        let content_area = Rect {
            x: area.x + 3,
            y: area.y + 2,
            width: area.width - 6,
            height: area.height - 6,
        };

        if selected_tab == 0 {
            let widget = Paragraph::new(self.about_content(area)?)
                .block(Block::default().borders(Borders::NONE))
                .wrap(Wrap { trim: false });
            frame.render_widget(widget, content_area);
        } else if selected_tab == 1 {
            self.projects_content().draw(frame, content_area)?;
        } // FIXME: Blog tab handled in `App::render`

        // Render the borders
        frame.render_widget(
            Paragraph::new(border_top),
            Rect { x: area.x, y: area.y, width: area.width, height: 1 },
        );

        frame.render_widget(
            Paragraph::new(border_bottom),
            Rect { x: area.x, y: area.y + area.height - 1, width: area.width, height: 1 },
        );

        for i in 1..area.height - 1 {
            frame.render_widget(
                Paragraph::new(Line::from(border_left.clone())),
                Rect { x: area.x, y: area.y + i, width: 1, height: 1 },
            );

            frame.render_widget(
                Paragraph::new(Line::from(border_right.clone())),
                Rect { x: area.x + area.width - 1, y: area.y + i, width: 1, height: 1 },
            );
        }

        Ok(())
    }
}
