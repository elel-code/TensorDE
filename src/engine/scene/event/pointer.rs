//! Backend-independent pointer state and ordered pointer events.

use super::SceneEventSequence;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScenePointerSource {
    #[default]
    None,
    WaylandSurface,
    Replay,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScenePointerEventKind {
    Enter {
        serial: u32,
    },
    Leave {
        serial: u32,
    },
    Motion,
    Button {
        button: u32,
        pressed: bool,
        serial: u32,
    },
    Scroll {
        horizontal: f64,
        vertical: f64,
    },
}

impl ScenePointerEventKind {
    pub fn is_coalescible(self) -> bool {
        matches!(self, Self::Motion)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScenePointerEvent {
    pub source: ScenePointerSource,
    pub surface_id: u64,
    pub time_millis: u32,
    pub position: [f64; 2],
    pub surface_size: [u32; 2],
    pub kind: ScenePointerEventKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenePointerState {
    pub sequence: SceneEventSequence,
    pub source: ScenePointerSource,
    pub surface_id: u64,
    pub time_millis: u32,
    pub position: [f64; 2],
    pub surface_size: [u32; 2],
    pub inside: bool,
    pub pressed_buttons: Vec<u32>,
}

impl Default for ScenePointerState {
    fn default() -> Self {
        Self {
            sequence: SceneEventSequence::default(),
            source: ScenePointerSource::None,
            surface_id: 0,
            time_millis: 0,
            position: [0.0; 2],
            surface_size: [0; 2],
            inside: false,
            pressed_buttons: Vec::new(),
        }
    }
}

impl ScenePointerState {
    pub fn normalized_position_top_left(&self) -> Option<[f32; 2]> {
        let [width, height] = self.surface_size;
        if width == 0 || height == 0 {
            return None;
        }
        Some([
            (self.position[0] / f64::from(width)).clamp(0.0, 1.0) as f32,
            (self.position[1] / f64::from(height)).clamp(0.0, 1.0) as f32,
        ])
    }
}
