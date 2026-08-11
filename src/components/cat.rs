use color_eyre::Result;
use indoc::indoc;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::Component;
use crate::action::Action;

const CAT_DIMS: (u16, u16) = (16, 4);
const CAT_ASCII_ART: &str = indoc! {r#"
      |\__/,|   (`\
      |_ _  |.--.) )
      ( T   )     /
     (((^_(((/(((_>
"#};

#[derive(Default)]
pub struct Cat;

impl Cat {
    pub fn new() -> Self {
        Self
    }
}

impl Component for Cat {
    fn update(&mut self, action: Action) -> Result<Option<Action>> {
        match action {
            Action::Tick => {}
            Action::Render => {}
            _ => {}
        }

        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let (width, height) = CAT_DIMS;
        if area.width < width + 2 || area.height < height {
            return Ok(());
        }

        let corner = Rect {
            x: area.right().saturating_sub(width + 1),
            y: area.bottom().saturating_sub(height),
            width,
            height,
        };

        frame.render_widget(
            Paragraph::new(CAT_ASCII_ART).style(Style::default().fg(Color::Magenta).bold()),
            corner,
        );

        Ok(())
    }
}
