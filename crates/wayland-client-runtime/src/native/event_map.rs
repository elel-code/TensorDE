//! Map [`NativeShellEvent`] toward the public [`crate::Event`] model.
//!
//! Structural bridge for migrating Fika off SCTK `Runtime`. Events that need a
//! live `WlSeat` serial (press/grab) are omitted until the native seat proxy is
//! threaded through; surface lifecycle and text-bearing keys map cleanly.

use std::collections::HashMap;

use crate::event::{Event, SurfaceEvent};
use crate::geometry::{LogicalPosition, LogicalSize};
use crate::native::shell::{NativeShellEvent, NativeSurfaceId};
use crate::surface::SurfaceId;
use crate::{LayerSurfaceEvent, ToplevelState};

/// Bidirectional id map for native ↔ public surface identifiers.
#[derive(Clone, Debug, Default)]
pub struct SurfaceIdMap {
    native_to_public: HashMap<NativeSurfaceId, SurfaceId>,
    next_public: u64,
}

impl SurfaceIdMap {
    pub fn new() -> Self {
        Self {
            native_to_public: HashMap::new(),
            next_public: 1,
        }
    }

    /// Allocate or reuse a public [`SurfaceId`] for a native surface.
    pub fn intern(&mut self, native: NativeSurfaceId) -> SurfaceId {
        *self.native_to_public.entry(native).or_insert_with(|| {
            let id = SurfaceId(self.next_public);
            self.next_public = self.next_public.saturating_add(1);
            id
        })
    }

    pub fn get(&self, native: NativeSurfaceId) -> Option<SurfaceId> {
        self.native_to_public.get(&native).copied()
    }

    pub fn remove(&mut self, native: NativeSurfaceId) -> Option<SurfaceId> {
        self.native_to_public.remove(&native)
    }
}

/// Convert one native shell event into a public crate event when possible.
///
/// Returns `None` for events that still need seat proxies, full DnD types, or
/// other SCTK-era context not yet available on the native path.
pub fn map_native_event(
    event: NativeShellEvent,
    surfaces: &mut SurfaceIdMap,
) -> Option<Event> {
    match event {
        NativeShellEvent::ToplevelConfigure {
            surface,
            suggested_size,
        } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::Configure {
                surface,
                suggested_size,
                state: ToplevelState::empty(),
                serial: 0,
            }))
        }
        NativeShellEvent::ToplevelClose { surface } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::CloseRequested { surface }))
        }
        NativeShellEvent::ScaleFactorChanged { surface, factor } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::ScaleFactorChanged {
                surface,
                factor,
            }))
        }
        NativeShellEvent::Frame { surface, time } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::Frame { surface, time }))
        }
        NativeShellEvent::PopupConfigure {
            surface,
            x,
            y,
            width,
            height,
        } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::PopupConfigure {
                surface,
                position: LogicalPosition::new(x, y),
                size: LogicalSize::new(width.max(0) as u32, height.max(0) as u32),
                serial: 0,
                kind: crate::event::PopupConfigureKind::Initial,
            }))
        }
        NativeShellEvent::PopupDone { surface } => {
            let surface = surfaces.intern(surface);
            Some(Event::Surface(SurfaceEvent::PopupDone { surface }))
        }
        NativeShellEvent::LayerConfigure {
            surface,
            suggested_size,
            serial,
        } => Some(Event::LayerSurface(LayerSurfaceEvent::Configure {
            surface: surfaces.intern(surface),
            suggested_size,
            serial,
        })),
        NativeShellEvent::LayerClosed { surface } => {
            Some(Event::LayerSurface(LayerSurfaceEvent::Closed {
                surface: surfaces.intern(surface),
            }))
        }
        // Keyboard text is the critical Fika path; serial/seat filled later.
        // We expose raw mapping via [`map_native_key_text`] for consumers that
        // do not need the full [`KeyboardEvent`] shape yet.
        _ => None,
    }
}

/// Extract UTF-8 / keysym from a native key event without building a full
/// [`KeyboardEvent`] (avoids inventing a fake `InputSerial` / seat).
pub fn map_native_key_text(
    event: &NativeShellEvent,
) -> Option<(u32, u32, bool, Option<&str>)> {
    match event {
        NativeShellEvent::SeatKeyboardKey {
            key,
            pressed,
            keysym,
            text,
        } => Some((*key, *keysym, *pressed, text.as_deref())),
        _ => None,
    }
}

/// Convenience: whether the event is a press that produced printable text.
pub fn native_key_text_pressed(event: &NativeShellEvent) -> Option<&str> {
    match event {
        NativeShellEvent::SeatKeyboardKey {
            pressed: true,
            text: Some(text),
            ..
        } if !text.is_empty() => Some(text.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SuggestedSize;

    #[test]
    fn maps_toplevel_configure() {
        let mut map = SurfaceIdMap::new();
        let native = NativeSurfaceId(1);
        let event = NativeShellEvent::ToplevelConfigure {
            surface: native,
            suggested_size: SuggestedSize::new(Some(800), Some(600)),
        };
        let mapped = map_native_event(event, &mut map).expect("mapped");
        match mapped {
            Event::Surface(SurfaceEvent::Configure {
                surface,
                suggested_size,
                ..
            }) => {
                assert_eq!(surface, map.get(native).unwrap());
                assert_eq!(suggested_size.width, Some(800));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn extracts_key_text() {
        let event = NativeShellEvent::SeatKeyboardKey {
            key: 30,
            pressed: true,
            keysym: 0x61,
            text: Some("a".into()),
        };
        assert_eq!(native_key_text_pressed(&event), Some("a"));
        let (key, keysym, pressed, text) = map_native_key_text(&event).unwrap();
        assert_eq!((key, keysym, pressed, text), (30, 0x61, true, Some("a")));
        // Full Event mapping intentionally skips keys until seat serial exists.
        let mut map = SurfaceIdMap::new();
        assert!(map_native_event(event, &mut map).is_none());
    }
}
