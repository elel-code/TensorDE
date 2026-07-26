//! Pointer hit-testing and frame action state machine.

use std::time::{Duration, Instant};

use crate::toplevel_interaction::ResizeEdge;

use super::buttons::ButtonKind;
use super::geometry::{BORDER_SIZE, HEADER_SIZE, RESIZE_CORNER};

/// Where the pointer is over the decoration frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HitLocation {
    #[default]
    None,
    Head,
    Top,
    TopLeft,
    TopRight,
    Left,
    Right,
    Bottom,
    BottomLeft,
    BottomRight,
    Button(ButtonKind),
}

impl HitLocation {
    #[allow(dead_code)]
    pub fn resize_edge(self) -> Option<ResizeEdge> {
        Some(match self {
            Self::Top => ResizeEdge::Top,
            Self::Bottom => ResizeEdge::Bottom,
            Self::Left => ResizeEdge::Left,
            Self::Right => ResizeEdge::Right,
            Self::TopLeft => ResizeEdge::TopLeft,
            Self::TopRight => ResizeEdge::TopRight,
            Self::BottomLeft => ResizeEdge::BottomLeft,
            Self::BottomRight => ResizeEdge::BottomRight,
            _ => return None,
        })
    }
}

/// Cursor shape recommendation for a hit location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameCursor {
    Default,
    Pointer,
    NResize,
    SResize,
    EResize,
    WResize,
    NeResize,
    NwResize,
    SeResize,
    SwResize,
}

impl HitLocation {
    pub fn cursor(self, resizable: bool) -> FrameCursor {
        if !resizable {
            return match self {
                Self::Button(_) => FrameCursor::Pointer,
                _ => FrameCursor::Default,
            };
        }
        match self {
            Self::Top => FrameCursor::NResize,
            Self::Bottom => FrameCursor::SResize,
            Self::Left => FrameCursor::WResize,
            Self::Right => FrameCursor::EResize,
            Self::TopLeft => FrameCursor::NwResize,
            Self::TopRight => FrameCursor::NeResize,
            Self::BottomLeft => FrameCursor::SwResize,
            Self::BottomRight => FrameCursor::SeResize,
            Self::Button(_) => FrameCursor::Pointer,
            _ => FrameCursor::Default,
        }
    }
}

/// High-level action requested by a frame interaction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameAction {
    Move,
    Resize(ResizeEdge),
    Close,
    Maximize,
    UnMaximize,
    Minimize,
    /// Show compositor window menu at surface-local position (content coords).
    ShowMenu { x: i32, y: i32 },
}

const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// Linux `BTN_LEFT` / `BTN_RIGHT` (input-event-codes).
pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;

#[derive(Debug, Default)]
pub struct MouseState {
    pub location: HitLocation,
    /// Surface-local position on the decoration part currently focused.
    pub position: (f64, f64),
    last_title_click: Option<Instant>,
}

impl MouseState {
    pub fn moved(&mut self, location: HitLocation, x: f64, y: f64) {
        self.location = location;
        self.position = (x, y);
    }

    pub fn left(&mut self) {
        self.location = HitLocation::None;
    }

    /// Handle a button event. `pressed` is true on press.
    ///
    /// Buttons fire on release; move/resize/menu fire on press.
    pub fn click(
        &mut self,
        button: u32,
        pressed: bool,
        resizable: bool,
        maximized: bool,
        can_maximize: bool,
    ) -> Option<FrameAction> {
        if button == BTN_RIGHT {
            if pressed
                && matches!(
                    self.location,
                    HitLocation::Head | HitLocation::Button(_)
                )
            {
                return Some(FrameAction::ShowMenu {
                    x: self.position.0 as i32,
                    y: self.position.1 as i32 - HEADER_SIZE as i32,
                });
            }
            return None;
        }
        if button != BTN_LEFT {
            return None;
        }

        match self.location {
            HitLocation::Top if resizable && pressed => {
                Some(FrameAction::Resize(ResizeEdge::Top))
            }
            HitLocation::Bottom if resizable && pressed => {
                Some(FrameAction::Resize(ResizeEdge::Bottom))
            }
            HitLocation::Left if resizable && pressed => {
                Some(FrameAction::Resize(ResizeEdge::Left))
            }
            HitLocation::Right if resizable && pressed => {
                Some(FrameAction::Resize(ResizeEdge::Right))
            }
            HitLocation::TopLeft if resizable && pressed => {
                Some(FrameAction::Resize(ResizeEdge::TopLeft))
            }
            HitLocation::TopRight if resizable && pressed => {
                Some(FrameAction::Resize(ResizeEdge::TopRight))
            }
            HitLocation::BottomLeft if resizable && pressed => {
                Some(FrameAction::Resize(ResizeEdge::BottomLeft))
            }
            HitLocation::BottomRight if resizable && pressed => {
                Some(FrameAction::Resize(ResizeEdge::BottomRight))
            }
            HitLocation::Button(ButtonKind::Close) if !pressed => Some(FrameAction::Close),
            HitLocation::Button(ButtonKind::Maximize) if !pressed => {
                if maximized {
                    Some(FrameAction::UnMaximize)
                } else {
                    Some(FrameAction::Maximize)
                }
            }
            HitLocation::Button(ButtonKind::Minimize) if !pressed => Some(FrameAction::Minimize),
            HitLocation::Head if pressed => {
                if can_maximize {
                    let now = Instant::now();
                    if let Some(last) = self.last_title_click.replace(now) {
                        if now.duration_since(last) < DOUBLE_CLICK {
                            self.last_title_click = None;
                            return Some(if maximized {
                                FrameAction::UnMaximize
                            } else {
                                FrameAction::Maximize
                            });
                        }
                    }
                }
                Some(FrameAction::Move)
            }
            _ => None,
        }
    }
}

/// Refine a coarse edge location using local coordinates (corner detection).
pub fn refine_edge(
    coarse: HitLocation,
    x: f64,
    y: f64,
    part_width: u32,
    part_height: u32,
) -> HitLocation {
    let corner = f64::from(RESIZE_CORNER);
    let w = f64::from(part_width);
    let h = f64::from(part_height);
    match coarse {
        HitLocation::Top | HitLocation::TopLeft | HitLocation::TopRight => {
            if x <= corner {
                HitLocation::TopLeft
            } else if x >= w - corner {
                HitLocation::TopRight
            } else {
                HitLocation::Top
            }
        }
        HitLocation::Bottom | HitLocation::BottomLeft | HitLocation::BottomRight => {
            if x <= corner {
                HitLocation::BottomLeft
            } else if x >= w - corner {
                HitLocation::BottomRight
            } else {
                HitLocation::Bottom
            }
        }
        HitLocation::Left => {
            if y <= corner {
                HitLocation::TopLeft
            } else if y >= h - corner {
                HitLocation::BottomLeft
            } else {
                HitLocation::Left
            }
        }
        HitLocation::Right => {
            if y <= corner {
                HitLocation::TopRight
            } else if y >= h - corner {
                HitLocation::BottomRight
            } else {
                HitLocation::Right
            }
        }
        other => other,
    }
}

/// Which decoration subsurface was hit (shared with the frame).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FramePartKind {
    Top,
    Left,
    Right,
    Bottom,
    Header,
}

/// Coarse hit location for a decoration part kind.
pub fn coarse_for_part(part: FramePartKind) -> HitLocation {
    match part {
        FramePartKind::Top => HitLocation::Top,
        FramePartKind::Left => HitLocation::Left,
        FramePartKind::Right => HitLocation::Right,
        FramePartKind::Bottom => HitLocation::Bottom,
        FramePartKind::Header => HitLocation::Head,
    }
}

/// Whether a content-local point falls in the outer resize ring (when borders
/// are drawn as part of the content surface — not used with subsurfaces).
#[cfg(test)]
pub fn content_edge_hit(
    x: f64,
    y: f64,
    content_w: u32,
    content_h: u32,
    border: u32,
) -> Option<HitLocation> {
    let b = f64::from(border);
    let w = f64::from(content_w);
    let h = f64::from(content_h);
    let left = x < b;
    let right = x >= w - b;
    let top = y < b;
    let bottom = y >= h - b;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(HitLocation::TopLeft),
        (_, true, true, _) => Some(HitLocation::TopRight),
        (true, _, _, true) => Some(HitLocation::BottomLeft),
        (_, true, _, true) => Some(HitLocation::BottomRight),
        (true, _, _, _) => Some(HitLocation::Left),
        (_, true, _, _) => Some(HitLocation::Right),
        (_, _, true, _) => Some(HitLocation::Top),
        (_, _, _, true) => Some(HitLocation::Bottom),
        _ => None,
    }
}

/// Map a header-local position through buttons.
pub fn header_hit(buttons: &super::buttons::Buttons, x: f64, y: f64) -> HitLocation {
    let _ = BORDER_SIZE;
    buttons.find(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_click_maximizes() {
        let mut mouse = MouseState::default();
        mouse.moved(HitLocation::Head, 100.0, 10.0);
        assert_eq!(
            mouse.click(BTN_LEFT, true, true, false, true),
            Some(FrameAction::Move)
        );
        // Simulate second click within window.
        mouse.last_title_click = Some(Instant::now());
        assert_eq!(
            mouse.click(BTN_LEFT, true, true, false, true),
            Some(FrameAction::Maximize)
        );
    }

    #[test]
    fn close_fires_on_release() {
        let mut mouse = MouseState::default();
        mouse.moved(HitLocation::Button(ButtonKind::Close), 10.0, 10.0);
        assert_eq!(mouse.click(BTN_LEFT, true, true, false, true), None);
        assert_eq!(
            mouse.click(BTN_LEFT, false, true, false, true),
            Some(FrameAction::Close)
        );
    }

    #[test]
    fn refine_corners() {
        assert_eq!(
            refine_edge(HitLocation::Top, 2.0, 0.0, 400, 11),
            HitLocation::TopLeft
        );
        assert_eq!(
            refine_edge(HitLocation::Top, 390.0, 0.0, 400, 11),
            HitLocation::TopRight
        );
    }

    #[test]
    fn content_edge_hit_detects_ring_and_corners() {
        let border = 5;
        assert_eq!(
            content_edge_hit(2.0, 50.0, 100, 100, border),
            Some(HitLocation::Left)
        );
        assert_eq!(
            content_edge_hit(2.0, 2.0, 100, 100, border),
            Some(HitLocation::TopLeft)
        );
        assert_eq!(content_edge_hit(50.0, 50.0, 100, 100, border), None);
    }
}
