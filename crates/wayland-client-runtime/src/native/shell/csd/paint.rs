//! Software rasterizer for CSD parts (ARGB8888, no external paint crate).

use super::theme::Argb;

/// Tightly packed ARGB8888 (little-endian B,G,R,A in memory) pixmap.
#[derive(Clone, Debug)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Pixmap {
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        Self {
            width: width.max(1),
            height: height.max(1),
            pixels: vec![0u8; n.max(4)],
        }
    }

    pub fn clear(&mut self, color: Argb) {
        let [a, r, g, b] = color;
        let bgra = [b, g, r, a];
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&bgra);
        }
    }

    fn put(&mut self, x: i32, y: i32, color: Argb) {
        if x < 0 || y < 0 {
            return;
        }
        let x = x as u32;
        let y = y as u32;
        if x >= self.width || y >= self.height {
            return;
        }
        let i = ((y * self.width + x) * 4) as usize;
        let [a, r, g, b] = color;
        self.pixels[i] = b;
        self.pixels[i + 1] = g;
        self.pixels[i + 2] = r;
        self.pixels[i + 3] = a;
    }
}

pub fn fill_rect(pixmap: &mut Pixmap, x: i32, y: i32, w: u32, h: u32, color: Argb) {
    let x1 = x.max(0);
    let y1 = y.max(0);
    let x2 = (x + w as i32).min(pixmap.width as i32);
    let y2 = (y + h as i32).min(pixmap.height as i32);
    if x2 <= x1 || y2 <= y1 {
        return;
    }
    let [a, r, g, b] = color;
    let bgra = [b, g, r, a];
    for row in y1..y2 {
        let start = ((row as u32 * pixmap.width + x1 as u32) * 4) as usize;
        let end = ((row as u32 * pixmap.width + x2 as u32) * 4) as usize;
        for chunk in pixmap.pixels[start..end].chunks_exact_mut(4) {
            chunk.copy_from_slice(&bgra);
        }
    }
}

pub fn stroke_line(
    pixmap: &mut Pixmap,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Argb,
    thickness: i32,
) {
    // Bresenham with square brush.
    let mut x = x0;
    let mut y = y0;
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let t = thickness.max(1);
    loop {
        for oy in -t / 2..=t / 2 {
            for ox in -t / 2..=t / 2 {
                pixmap.put(x + ox, y + oy, color);
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Minimal 5×7 ASCII bitmap font (subset used for titles).
mod font5x7 {
    // Each glyph: 5 columns × 7 rows, bit 0 = top row of column packed...
    // Stored as 7 rows, 5 bits LSB = left.
    pub fn glyph(c: char) -> Option<[u8; 7]> {
        let c = if c.is_ascii() { c } else { '?' };
        let idx = c as u8;
        if !(0x20..=0x7e).contains(&idx) {
            return Some(UNKNOWN);
        }
        Some(TABLE[(idx - 0x20) as usize])
    }

    const UNKNOWN: [u8; 7] = [0x1f, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1f];

    // Compact table for printable ASCII — generated patterns.
    const TABLE: [[u8; 7]; 95] = {
        let mut t = [[0u8; 7]; 95];
        // Space
        t[0] = [0, 0, 0, 0, 0, 0, 0];
        // ! 
        t[1] = [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04];
        // digits 0-9 at '0' = 0x30 → index 16
        t[16] = [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e]; // 0
        t[17] = [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e]; // 1
        t[18] = [0x0e, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1f]; // 2
        t[19] = [0x0e, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0e]; // 3
        t[20] = [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02]; // 4
        t[21] = [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e]; // 5
        t[22] = [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e]; // 6
        t[23] = [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08]; // 7
        t[24] = [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e]; // 8
        t[25] = [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c]; // 9
        // A-Z
        t[33] = [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11]; // A
        t[34] = [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e]; // B
        t[35] = [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e]; // C
        t[36] = [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e]; // D
        t[37] = [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f]; // E
        t[38] = [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10]; // F
        t[39] = [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f]; // G
        t[40] = [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11]; // H
        t[41] = [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e]; // I
        t[42] = [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0e]; // J
        t[43] = [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11]; // K
        t[44] = [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f]; // L
        t[45] = [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11]; // M
        t[46] = [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11]; // N
        t[47] = [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e]; // O
        t[48] = [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10]; // P
        t[49] = [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d]; // Q
        t[50] = [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11]; // R
        t[51] = [0x0e, 0x11, 0x10, 0x0e, 0x01, 0x11, 0x0e]; // S
        t[52] = [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04]; // T
        t[53] = [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e]; // U
        t[54] = [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04]; // V
        t[55] = [0x11, 0x11, 0x11, 0x15, 0x15, 0x1b, 0x11]; // W
        t[56] = [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11]; // X
        t[57] = [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04]; // Y
        t[58] = [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f]; // Z
        // a-z (lowercase — simplified copies of uppercase for readability)
        t[65] = [0x00, 0x00, 0x0e, 0x01, 0x0f, 0x11, 0x0f]; // a
        t[66] = [0x10, 0x10, 0x1e, 0x11, 0x11, 0x11, 0x1e]; // b
        t[67] = [0x00, 0x00, 0x0e, 0x11, 0x10, 0x11, 0x0e]; // c
        t[68] = [0x01, 0x01, 0x0f, 0x11, 0x11, 0x11, 0x0f]; // d
        t[69] = [0x00, 0x00, 0x0e, 0x11, 0x1f, 0x10, 0x0e]; // e
        t[70] = [0x06, 0x09, 0x08, 0x1c, 0x08, 0x08, 0x08]; // f
        t[71] = [0x00, 0x0f, 0x11, 0x11, 0x0f, 0x01, 0x0e]; // g
        t[72] = [0x10, 0x10, 0x1e, 0x11, 0x11, 0x11, 0x11]; // h
        t[73] = [0x04, 0x00, 0x0c, 0x04, 0x04, 0x04, 0x0e]; // i
        t[74] = [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0c]; // j
        t[75] = [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12]; // k
        t[76] = [0x0c, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e]; // l
        t[77] = [0x00, 0x00, 0x1a, 0x15, 0x15, 0x15, 0x15]; // m
        t[78] = [0x00, 0x00, 0x1e, 0x11, 0x11, 0x11, 0x11]; // n
        t[79] = [0x00, 0x00, 0x0e, 0x11, 0x11, 0x11, 0x0e]; // o
        t[80] = [0x00, 0x00, 0x1e, 0x11, 0x1e, 0x10, 0x10]; // p
        t[81] = [0x00, 0x00, 0x0f, 0x11, 0x0f, 0x01, 0x01]; // q
        t[82] = [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10]; // r
        t[83] = [0x00, 0x00, 0x0f, 0x10, 0x0e, 0x01, 0x1e]; // s
        t[84] = [0x08, 0x08, 0x1c, 0x08, 0x08, 0x09, 0x06]; // t
        t[85] = [0x00, 0x00, 0x11, 0x11, 0x11, 0x11, 0x0f]; // u
        t[86] = [0x00, 0x00, 0x11, 0x11, 0x11, 0x0a, 0x04]; // v
        t[87] = [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0a]; // w
        t[88] = [0x00, 0x00, 0x11, 0x0a, 0x04, 0x0a, 0x11]; // x
        t[89] = [0x00, 0x00, 0x11, 0x11, 0x0f, 0x01, 0x0e]; // y
        t[90] = [0x00, 0x00, 0x1f, 0x02, 0x04, 0x08, 0x1f]; // z
        // - _ . :
        t[13] = [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00]; // -
        t[63] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1f]; // _
        t[14] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c]; // .
        t[26] = [0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x00]; // :
        // /
        t[15] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x00, 0x00];
        t
    };
}

/// Draw `text` centered in `[x0, x1)` at vertical center of the pixmap.
pub fn draw_title(
    pixmap: &mut Pixmap,
    text: &str,
    x0: f32,
    x1: f32,
    scale: f32,
    color: Argb,
) {
    if text.is_empty() || x1 <= x0 {
        return;
    }
    let cell_w = 6.0 * scale; // 5 px glyph + 1 px gap
    let cell_h = 7.0 * scale;
    let chars: Vec<char> = text.chars().take(256).collect();
    let total_w = chars.len() as f32 * cell_w;
    let mut x = ((x0 + x1 - total_w) * 0.5).max(x0);
    let y = (pixmap.height as f32 - cell_h) * 0.5;
    let max_x = x1;
    for ch in chars {
        if x + cell_w > max_x {
            break;
        }
        if let Some(rows) = font5x7::glyph(ch) {
            for (row, bits) in rows.iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        let px = (x + col as f32 * scale) as i32;
                        let py = (y + row as f32 * scale) as i32;
                        // Scale block.
                        for sy in 0..scale.ceil() as i32 {
                            for sx in 0..scale.ceil() as i32 {
                                pixmap.put(px + sx, py + sy, color);
                            }
                        }
                    }
                }
            }
        }
        x += cell_w;
    }
}

/// Paint a border strip (top/left/right/bottom) with a 1px visible edge.
///
/// `edge_side` is the side that faces the content (for the visible edge).
pub fn paint_border_strip(
    width: u32,
    height: u32,
    scale: f32,
    fill: Argb,
    edge: Argb,
    edge_side: EdgeSide,
) -> Pixmap {
    let bw = ((width as f32) * scale).round().max(1.0) as u32;
    let bh = ((height as f32) * scale).round().max(1.0) as u32;
    let mut pm = Pixmap::new(bw, bh);
    pm.clear(fill);
    let edge_px = scale.round().max(1.0) as i32;
    match edge_side {
        EdgeSide::Bottom => fill_rect(&mut pm, 0, bh as i32 - edge_px, bw, edge_px as u32, edge),
        EdgeSide::Top => fill_rect(&mut pm, 0, 0, bw, edge_px as u32, edge),
        EdgeSide::Left => fill_rect(&mut pm, bw as i32 - edge_px, 0, edge_px as u32, bh, edge),
        EdgeSide::Right => fill_rect(&mut pm, 0, 0, edge_px as u32, bh, edge),
    }
    pm
}

#[derive(Clone, Copy)]
pub enum EdgeSide {
    Top,
    Bottom,
    Left,
    Right,
}

/// Paint the header bar: background, title, buttons.
pub fn paint_header(
    width: u32,
    scale: f32,
    colors: &super::theme::ColorMap,
    title: &str,
    buttons: &super::buttons::Buttons,
    hover: super::input::HitLocation,
    maximized: bool,
) -> Pixmap {
    let bw = ((width as f32) * scale).round().max(1.0) as u32;
    let bh = ((super::geometry::HEADER_SIZE as f32) * scale)
        .round()
        .max(1.0) as u32;
    let mut pm = Pixmap::new(bw, bh);
    pm.clear(colors.headerbar);
    // Bottom separator line.
    let edge_px = scale.round().max(1.0) as u32;
    fill_rect(
        &mut pm,
        0,
        bh as i32 - edge_px as i32,
        bw,
        edge_px,
        colors.border,
    );

    buttons.draw(&mut pm, scale, colors, hover, maximized);

    let (tx0, tx1) = buttons.title_range(width);
    draw_title(&mut pm, title, tx0 * scale, tx1 * scale, scale, colors.title);
    pm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixmap_clear_writes_argb() {
        let mut pm = Pixmap::new(2, 2);
        pm.clear([0xff, 0x11, 0x22, 0x33]);
        assert_eq!(&pm.pixels[0..4], &[0x33, 0x22, 0x11, 0xff]);
    }

    #[test]
    fn title_draws_without_panic() {
        let mut pm = Pixmap::new(200, 36);
        pm.clear([0xff, 0x20, 0x20, 0x20]);
        draw_title(&mut pm, "Hello Fika", 10.0, 190.0, 1.0, [0xff, 0xff, 0xff, 0xff]);
        // White title glyphs set B/G/R to 0xff; background keeps 0x20.
        assert!(
            pm.pixels.chunks_exact(4).any(|px| px[0] == 0xff && px[1] == 0xff && px[2] == 0xff),
            "expected at least one white title pixel"
        );
    }
}
