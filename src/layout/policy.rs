use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::geometry::Rect;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum LayoutKind {
    #[default]
    #[serde(rename = "scrolling-1d")]
    Scrolling1D,
    #[serde(rename = "spatial-2d")]
    Spatial2D,
    #[serde(rename = "classic")]
    Classic,
}

impl LayoutKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scrolling1D => "scrolling-1d",
            Self::Spatial2D => "spatial-2d",
            Self::Classic => "classic",
        }
    }
}

impl FromStr for LayoutKind {
    type Err = ParseLayoutError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "scrolling-1d" => Ok(Self::Scrolling1D),
            "spatial-2d" => Ok(Self::Spatial2D),
            "classic" => Ok(Self::Classic),
            _ => Err(ParseLayoutError(value.to_owned())),
        }
    }
}

#[derive(Debug, Error)]
#[error("unknown layout '{0}'; expected scrolling-1d, spatial-2d, or classic")]
pub struct ParseLayoutError(String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutEngine {
    kind: LayoutKind,
}

impl LayoutEngine {
    pub const fn new(kind: LayoutKind) -> Self {
        Self { kind }
    }

    pub const fn kind(self) -> LayoutKind {
        self.kind
    }

    pub fn arrange(self, area: Rect, view_count: usize) -> Vec<Rect> {
        match self.kind {
            LayoutKind::Scrolling1D => arrange_columns(area, view_count),
            LayoutKind::Spatial2D => arrange_grid(area, view_count),
            LayoutKind::Classic => arrange_master_stack(area, view_count),
        }
    }
}

fn arrange_columns(area: Rect, count: usize) -> Vec<Rect> {
    tensor_util::split_evenly(area.width, count)
        .into_iter()
        .scan(area.x, |x, width| {
            let rect = Rect::new(*x, area.y, width, area.height);
            *x = add_offset(*x, width);
            Some(rect)
        })
        .collect()
}

fn arrange_grid(area: Rect, count: usize) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }

    let columns = integer_ceil_sqrt(count);
    let rows = count.div_ceil(columns);
    let widths = tensor_util::split_evenly(area.width, columns);
    let heights = tensor_util::split_evenly(area.height, rows);

    let mut result = Vec::with_capacity(count);
    let mut y = area.y;
    for height in heights {
        let mut x = area.x;
        for &width in &widths {
            if result.len() == count {
                return result;
            }
            result.push(Rect::new(x, y, width, height));
            x = add_offset(x, width);
        }
        y = add_offset(y, height);
    }
    result
}

fn arrange_master_stack(area: Rect, count: usize) -> Vec<Rect> {
    match count {
        0 => Vec::new(),
        1 => vec![area],
        _ => {
            let master_width = area.width.saturating_mul(55) / 100;
            let stack_width = area.width - master_width;
            let stack_x = add_offset(area.x, master_width);
            let mut result = Vec::with_capacity(count);
            result.push(Rect::new(area.x, area.y, master_width, area.height));

            let mut y = area.y;
            for height in tensor_util::split_evenly(area.height, count - 1) {
                result.push(Rect::new(stack_x, y, stack_width, height));
                y = add_offset(y, height);
            }
            result
        }
    }
}

fn integer_ceil_sqrt(value: usize) -> usize {
    let mut root: usize = 1;
    while root.saturating_mul(root) < value {
        root += 1;
    }
    root
}

fn add_offset(origin: i32, amount: u32) -> i32 {
    origin.saturating_add(i32::try_from(amount).unwrap_or(i32::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: Rect = Rect::new(0, 0, 1920, 1080);

    #[test]
    fn empty_layout_has_no_rectangles() {
        for kind in [
            LayoutKind::Scrolling1D,
            LayoutKind::Spatial2D,
            LayoutKind::Classic,
        ] {
            assert!(LayoutEngine::new(kind).arrange(OUTPUT, 0).is_empty());
        }
    }

    #[test]
    fn serialized_names_match_configuration_names() {
        assert_eq!(
            serde_json::to_string(&LayoutKind::Scrolling1D).unwrap(),
            "\"scrolling-1d\""
        );
        assert_eq!(
            serde_json::to_string(&LayoutKind::Spatial2D).unwrap(),
            "\"spatial-2d\""
        );
    }

    #[test]
    fn scrolling_layout_builds_full_height_columns() {
        let rects = LayoutEngine::new(LayoutKind::Scrolling1D).arrange(OUTPUT, 3);

        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0], Rect::new(0, 0, 640, 1080));
        assert_eq!(rects[2], Rect::new(1280, 0, 640, 1080));
    }

    #[test]
    fn spatial_layout_builds_a_compact_grid() {
        let rects = LayoutEngine::new(LayoutKind::Spatial2D).arrange(OUTPUT, 4);

        assert_eq!(rects.len(), 4);
        assert_eq!(rects[0], Rect::new(0, 0, 960, 540));
        assert_eq!(rects[3], Rect::new(960, 540, 960, 540));
    }

    #[test]
    fn classic_layout_builds_master_and_stack() {
        let rects = LayoutEngine::new(LayoutKind::Classic).arrange(OUTPUT, 3);

        assert_eq!(rects[0], Rect::new(0, 0, 1056, 1080));
        assert_eq!(rects[1], Rect::new(1056, 0, 864, 540));
        assert_eq!(rects[2], Rect::new(1056, 540, 864, 540));
    }

    #[test]
    fn uneven_splits_cover_the_whole_axis() {
        let rects = LayoutEngine::new(LayoutKind::Scrolling1D).arrange(Rect::new(10, 20, 7, 5), 3);

        assert_eq!(rects.iter().map(|rect| rect.width).sum::<u32>(), 7);
        assert_eq!(
            rects.last().map(|rect| rect.x + rect.width as i32),
            Some(17)
        );
    }
}
