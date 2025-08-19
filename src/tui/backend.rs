use std::io;
use std::ops::{Deref, DerefMut};

use ratatui::backend::{Backend, CrosstermBackend, WindowSize};
use ratatui::layout::Size;

use crate::ssh::TermWriter;

#[derive(Debug)]
pub struct SshBackend {
    inner: CrosstermBackend<TermWriter>,
    pub dims: (u16, u16),
    pub pixel: (u16, u16),
}

impl SshBackend {
    pub fn new(
        writer: TermWriter,
        init_width: u16,
        init_height: u16,
        init_pixel_width: u16,
        init_pixel_height: u16,
    ) -> Self {
        let inner = CrosstermBackend::new(writer);
        SshBackend {
            inner,
            dims: (init_width, init_height),
            pixel: (init_pixel_width, init_pixel_height),
        }
    }
}

impl Backend for SshBackend {
    fn size(&self) -> io::Result<Size> {
        Ok(Size { width: self.dims.0, height: self.dims.1 })
    }

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>, {
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<ratatui::prelude::Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<ratatui::prelude::Position>>(
        &mut self,
        position: P,
    ) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn window_size(&mut self) -> io::Result<ratatui::backend::WindowSize> {
        Ok(WindowSize {
            columns_rows: self.size()?,
            pixels: Size { width: self.dims.0, height: self.dims.1 },
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

impl Deref for SshBackend {
    type Target = CrosstermBackend<TermWriter>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for SshBackend {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
