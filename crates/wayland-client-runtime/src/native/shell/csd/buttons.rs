//! Titlebar window-control buttons and layout.

use super::geometry::HEADER_SIZE;
use super::input::HitLocation;
use super::paint::{Pixmap, fill_rect, stroke_line};
use super::theme::ColorMap;

const BUTTON_SIZE: f32 = 24.0;
const BUTTON_MARGIN: f32 = 8.0;
const BUTTON_SPACING: f32 = 10.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonKind {
    Close,
    Maximize,
    Minimize,
}

#[derive(Clone, Copy, Debug)]
struct Button {
    kind: ButtonKind,
    /// Left edge of the button circle/box in header-local coordinates.
    x: f32,
}

#[derive(Clone, Debug)]
pub struct Buttons {
    left: Vec<Button>,
    right: Vec<Button>,
    supports_maximize: bool,
    supports_minimize: bool,
}

impl Default for Buttons {
    fn default() -> Self {
        Self {
            left: Vec::new(),
            right: vec![
                Button {
                    kind: ButtonKind::Minimize,
                    x: 0.0,
                },
                Button {
                    kind: ButtonKind::Maximize,
                    x: 0.0,
                },
                Button {
                    kind: ButtonKind::Close,
                    x: 0.0,
                },
            ],
            supports_maximize: true,
            supports_minimize: true,
        }
    }
}

impl Buttons {
    pub fn set_capabilities(&mut self, maximize: bool, minimize: bool) {
        self.supports_maximize = maximize;
        self.supports_minimize = minimize;
        // Rebuild right-side defaults with capability filter.
        self.right.clear();
        if minimize {
            self.right.push(Button {
                kind: ButtonKind::Minimize,
                x: 0.0,
            });
        }
        if maximize {
            self.right.push(Button {
                kind: ButtonKind::Maximize,
                x: 0.0,
            });
        }
        self.right.push(Button {
            kind: ButtonKind::Close,
            x: 0.0,
        });
    }

    /// Place buttons for a header of the given logical width.
    pub fn arrange(&mut self, header_width: u32) {
        let mut lx = BUTTON_MARGIN;
        for b in &mut self.left {
            b.x = lx;
            lx += BUTTON_SIZE + BUTTON_SPACING;
        }
        let mut rx = header_width as f32 - BUTTON_MARGIN;
        // Right side: Close is rightmost; place right-to-left then reverse order
        // is already Minimize, Maximize, Close so walk reverse.
        for b in self.right.iter_mut().rev() {
            rx -= BUTTON_SIZE;
            b.x = rx;
            rx -= BUTTON_SPACING;
        }
    }

    pub fn find(&self, x: f64, y: f64) -> HitLocation {
        let y = y as f32;
        let x = x as f32;
        let cy = (HEADER_SIZE as f32 - BUTTON_SIZE) * 0.5;
        if y < cy || y > cy + BUTTON_SIZE {
            return HitLocation::Head;
        }
        for b in self.left.iter().chain(self.right.iter()) {
            if x >= b.x && x < b.x + BUTTON_SIZE {
                return HitLocation::Button(b.kind);
            }
        }
        HitLocation::Head
    }

    /// Horizontal range available for the title (between left and right buttons).
    pub fn title_range(&self, header_width: u32) -> (f32, f32) {
        let left_end = self
            .left
            .last()
            .map(|b| b.x + BUTTON_SIZE + BUTTON_SPACING)
            .unwrap_or(BUTTON_MARGIN);
        let right_start = self
            .right
            .first()
            .map(|b| b.x - BUTTON_SPACING)
            .unwrap_or(header_width as f32 - BUTTON_MARGIN);
        (left_end, right_start.max(left_end))
    }

    pub fn draw(
        &self,
        pixmap: &mut Pixmap,
        scale: f32,
        colors: &ColorMap,
        hover: HitLocation,
        maximized: bool,
    ) {
        let cy = (HEADER_SIZE as f32 - BUTTON_SIZE) * 0.5;
        for b in self.left.iter().chain(self.right.iter()) {
            let hovered = hover == HitLocation::Button(b.kind);
            let bg = if hovered {
                colors.button_hover
            } else {
                colors.button_idle
            };
            let x0 = (b.x * scale) as i32;
            let y0 = (cy * scale) as i32;
            let size = (BUTTON_SIZE * scale) as u32;
            // Rounded-ish button background (filled rect with inset).
            fill_rect(pixmap, x0, y0, size, size, bg);
            draw_icon(pixmap, b.kind, x0, y0, size, colors.button_icon, maximized);
        }
    }
}

fn draw_icon(
    pixmap: &mut Pixmap,
    kind: ButtonKind,
    x0: i32,
    y0: i32,
    size: u32,
    color: [u8; 4],
    maximized: bool,
) {
    let pad = (size as i32) / 4;
    let x1 = x0 + pad;
    let y1 = y0 + pad;
    let x2 = x0 + size as i32 - pad;
    let y2 = y0 + size as i32 - pad;
    match kind {
        ButtonKind::Close => {
            stroke_line(pixmap, x1, y1, x2, y2, color, 2);
            stroke_line(pixmap, x2, y1, x1, y2, color, 2);
        }
        ButtonKind::Minimize => {
            let mid_y = (y1 + y2) / 2 + (size as i32) / 8;
            stroke_line(pixmap, x1, mid_y, x2, mid_y, color, 2);
        }
        ButtonKind::Maximize => {
            if maximized {
                // Two overlapping squares (restore).
                let o = (size as i32) / 8;
                stroke_rect(pixmap, x1 + o, y1, x2, y2 - o, color, 2);
                stroke_rect(pixmap, x1, y1 + o, x2 - o, y2, color, 2);
            } else {
                stroke_rect(pixmap, x1, y1, x2, y2, color, 2);
            }
        }
    }
}

fn stroke_rect(
    pixmap: &mut Pixmap,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: [u8; 4],
    thickness: i32,
) {
    stroke_line(pixmap, x1, y1, x2, y1, color, thickness);
    stroke_line(pixmap, x2, y1, x2, y2, color, thickness);
    stroke_line(pixmap, x2, y2, x1, y2, color, thickness);
    stroke_line(pixmap, x1, y2, x1, y1, color, thickness);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_close_on_right() {
        let mut buttons = Buttons::default();
        buttons.arrange(400);
        // Close is rightmost.
        let close_x = buttons.right.last().unwrap().x as f64 + 4.0;
        let y = HEADER_SIZE as f64 / 2.0;
        assert_eq!(
            buttons.find(close_x, y),
            HitLocation::Button(ButtonKind::Close)
        );
        assert_eq!(buttons.find(200.0, y), HitLocation::Head);
    }
}
