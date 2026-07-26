//! `ext-image-copy-capture` / `ext-image-capture-source` (tier-2 staging).
//!
//! # Performance
//!
//! - Handlers only **queue** frames; pixel fill runs from the event idle turn
//!   with a hard budget of one frame per turn.
//! - Silhouette + optional SHM client blit (1:1); no GPU/KMS on hot path.
//! - Cost is O(buffer pixels) once per request, never on the page-flip path.
//! - Oversized buffers and DMA client buffers fail honestly.
//!
//! Content is a **scene silhouette** (clear + opaque view rects). That unblocks
//! portal clients and protocol tests without a full Vulkan readback pipeline.

use std::{
    collections::VecDeque,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use smithay::{
    output::{Output, WeakOutput},
    utils::{Buffer as BufferCoords, Size, Transform},
    wayland::{
        foreign_toplevel_list::{ForeignToplevelHandle, ForeignToplevelWeakHandle},
        image_capture_source::ImageCaptureSource,
        image_copy_capture::{
            BufferConstraints, CaptureFailureReason, CursorSession, Frame, Session, SessionRef,
        },
        seat::WaylandFocus,
        shm::{BufferAccessError, with_buffer_contents_mut},
    },
};
use tracing::{debug, trace, warn};
use wayland_server::protocol::{wl_buffer::WlBuffer, wl_shm};

use super::capture_shm;
use super::{ObjectKey, RuntimeState};
use crate::ecs::ViewId;
use crate::layout::Rect;

/// Max pending capture frames (drop oldest on overflow — capture is lossy).
const MAX_PENDING_CAPTURES: usize = 4;
/// Refuse SHM fills larger than this (width×height).
const MAX_CAPTURE_PIXELS: u32 = 3840 * 2160;
/// Bus timer id used only to ensure an idle turn notices pending captures.
const CAPTURE_TIMER_ID: u64 = 0xC0_FF_EE;

/// Side table for live capture sessions and deferred frame work.
#[derive(Default)]
pub(crate) struct CaptureSessions {
    pub(crate) sessions: Vec<Session>,
    pub(crate) cursor_sessions: Vec<CursorSession>,
    pending: VecDeque<PendingCapture>,
}

struct PendingCapture {
    frame: Frame,
    kind: CaptureKind,
    queued_at: Instant,
}

#[derive(Clone, Copy, Debug)]
enum CaptureKind {
    /// Full output; `origin` is the output's global logical top-left.
    Output {
        size: Size<i32, BufferCoords>,
        origin: (i32, i32),
    },
    Toplevel {
        size: Size<i32, BufferCoords>,
        geometry: Rect,
    },
}

impl RuntimeState {
    pub(crate) fn capture_constraints_for_source(
        &self,
        source: &ImageCaptureSource,
    ) -> Option<BufferConstraints> {
        if let Some(weak) = source.user_data().get::<WeakOutput>()
            && let Some(output) = weak.upgrade()
        {
            return constraints_for_output(&output);
        }
        if let Some(weak) = source.user_data().get::<ForeignToplevelWeakHandle>()
            && let Some(handle) = weak.upgrade()
            && let Some((size, _)) = self.toplevel_capture_geometry(&handle)
        {
            return Some(shm_constraints(size));
        }
        None
    }

    fn toplevel_capture_geometry(
        &self,
        handle: &ForeignToplevelHandle,
    ) -> Option<(Size<i32, BufferCoords>, Rect)> {
        let key = *handle.user_data().get::<ObjectKey>()?;
        for window in self.space.elements() {
            let Some(surface) = window.wl_surface() else {
                continue;
            };
            if ObjectKey::from_surface(&surface) != key {
                continue;
            }
            let geo = self.space.element_geometry(window)?;
            let size = Size::from((geo.size.w.max(1), geo.size.h.max(1)));
            let rect = Rect::new(
                geo.loc.x,
                geo.loc.y,
                u32::try_from(geo.size.w.max(1)).unwrap_or(1),
                u32::try_from(geo.size.h.max(1)).unwrap_or(1),
            );
            return Some((size, rect));
        }
        None
    }

    pub(crate) fn store_capture_session(&mut self, session: Session) {
        self.protocol_side.capture.sessions.push(session);
    }

    pub(crate) fn store_cursor_capture_session(&mut self, session: CursorSession) {
        self.protocol_side.capture.cursor_sessions.push(session);
    }

    pub(crate) fn drop_capture_session(&mut self, session: &SessionRef) {
        self.protocol_side
            .capture
            .sessions
            .retain(|stored| stored.as_ref() != *session);
    }

    pub(crate) fn handle_capture_frame(&mut self, session: &SessionRef, frame: Frame) {
        let Some(kind) = capture_kind_for_session(self, session) else {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        };
        if capture_pixel_count(kind) > MAX_CAPTURE_PIXELS {
            warn!(
                pixels = capture_pixel_count(kind),
                "rejecting oversized image-copy-capture frame"
            );
            frame.fail(CaptureFailureReason::BufferConstraints);
            return;
        }
        let pending = &mut self.protocol_side.capture.pending;
        while pending.len() >= MAX_PENDING_CAPTURES {
            if let Some(old) = pending.pop_front() {
                debug!("dropping oldest pending capture (queue full)");
                old.frame.fail(CaptureFailureReason::Unknown);
            }
        }
        pending.push_back(PendingCapture {
            frame,
            kind,
            queued_at: Instant::now(),
        });
        // Ensure the next idle turn runs without forcing a CRTC redraw.
        let _ = self.push_event(tensor_event::Event::Timer(tensor_event::TimerId(
            CAPTURE_TIMER_ID,
        )));
    }

    /// Drain at most one pending capture fill (event idle turn).
    pub(crate) fn process_pending_captures(&mut self) {
        if let Some(pending) = self.protocol_side.capture.pending.pop_front() {
            let wait = pending.queued_at.elapsed();
            if fill_capture_frame(self, pending.frame, pending.kind).is_ok() {
                trace!(
                    ?wait,
                    "image-copy-capture frame filled (software silhouette)"
                );
            }
        }
    }
}

fn capture_kind_for_session(state: &RuntimeState, session: &SessionRef) -> Option<CaptureKind> {
    let source = session.source();
    if let Some(weak) = source.user_data().get::<WeakOutput>()
        && let Some(output) = weak.upgrade()
    {
        let mode = output.current_mode()?;
        let origin = state
            .space
            .output_geometry(&output)
            .map(|geo| (geo.loc.x, geo.loc.y))
            .unwrap_or((0, 0));
        return Some(CaptureKind::Output {
            size: Size::from((mode.size.w, mode.size.h)),
            origin,
        });
    }
    if let Some(weak) = source.user_data().get::<ForeignToplevelWeakHandle>()
        && let Some(handle) = weak.upgrade()
        && let Some((size, geometry)) = state.toplevel_capture_geometry(&handle)
    {
        return Some(CaptureKind::Toplevel { size, geometry });
    }
    None
}

fn capture_pixel_count(kind: CaptureKind) -> u32 {
    let size = match kind {
        CaptureKind::Output { size, .. } | CaptureKind::Toplevel { size, .. } => size,
    };
    let w = u32::try_from(size.w.max(0)).unwrap_or(0);
    let h = u32::try_from(size.h.max(0)).unwrap_or(0);
    w.saturating_mul(h)
}

fn fill_capture_frame(
    state: &RuntimeState,
    frame: Frame,
    kind: CaptureKind,
) -> Result<(), CaptureFailureReason> {
    let buffer = frame.buffer();
    let rects = silhouette_rects(state, kind);
    let size = match kind {
        CaptureKind::Output { size, .. } | CaptureKind::Toplevel { size, .. } => size,
    };
    let blits = collect_shm_blits(state, kind);
    match write_capture_shm(&buffer, size, &rects, &blits) {
        Ok(()) => {
            let presented = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO);
            frame.success(Transform::Normal, None, presented);
            Ok(())
        }
        Err(reason) => {
            frame.fail(reason);
            Err(reason)
        }
    }
}

/// Surfaces eligible for a real SHM pixel blit (idle path).
struct ShmBlit {
    surface: wayland_server::protocol::wl_surface::WlSurface,
    rect: Rect,
}

fn collect_shm_blits(state: &RuntimeState, kind: CaptureKind) -> Vec<ShmBlit> {
    let mut blits = Vec::new();
    match kind {
        CaptureKind::Output { origin, .. } => {
            let active = state.active_workspace();
            for window in state.space.elements() {
                let Some(geo) = state.space.element_geometry(window) else {
                    continue;
                };
                let Some(surface) = window.wl_surface() else {
                    continue;
                };
                if let Some(view_id) = state.view_for_surface(&surface)
                    && state
                        .world
                        .view_workspace(view_id)
                        .is_some_and(|ws| ws != active)
                {
                    continue;
                }
                let rect = Rect::new(
                    geo.loc.x.saturating_sub(origin.0),
                    geo.loc.y.saturating_sub(origin.1),
                    u32::try_from(geo.size.w.max(0)).unwrap_or(0),
                    u32::try_from(geo.size.h.max(0)).unwrap_or(0),
                );
                if rect.width > 0 && rect.height > 0 {
                    blits.push(ShmBlit {
                        surface: surface.into_owned(),
                        rect,
                    });
                }
            }
        }
        CaptureKind::Toplevel { size, geometry } => {
            // Root surface of the toplevel at full local geometry.
            for window in state.space.elements() {
                let Some(geo) = state.space.element_geometry(window) else {
                    continue;
                };
                if geo.loc.x != geometry.x || geo.loc.y != geometry.y {
                    continue;
                }
                let Some(surface) = window.wl_surface() else {
                    continue;
                };
                blits.push(ShmBlit {
                    surface: surface.into_owned(),
                    rect: Rect::new(0, 0, size.w.max(0) as u32, size.h.max(0) as u32),
                });
                break;
            }
        }
    }
    blits
}

/// Buffer-local rectangles and ARGB8888 colors for the software silhouette.
///
/// Order is back-to-front: backdrop, then windows (stacking order from Space),
/// then a thin focus ring on the active view when capturing an output.
fn silhouette_rects(state: &RuntimeState, kind: CaptureKind) -> Vec<(Rect, u32)> {
    let mut rects = Vec::new();
    match kind {
        CaptureKind::Output { size, origin } => {
            let viewport = Rect::new(0, 0, size.w.max(0) as u32, size.h.max(0) as u32);
            // Subtle vertical gradient feel via two bands (still O(rects)).
            rects.push((viewport, 0xFF_1A_1B_22));
            let band_h = (viewport.height / 3).max(1);
            rects.push((Rect::new(0, 0, viewport.width, band_h), 0xFF_22_24_2E));
            let active = state.active_workspace();
            let mut focus_rect = None;
            for window in state.space.elements() {
                let Some(geo) = state.space.element_geometry(window) else {
                    continue;
                };
                let Some(surface) = window.wl_surface() else {
                    continue;
                };
                let view_id = state.view_for_surface(&surface);
                // Only paint views on the active desktop (space may still hold
                // unmapped inactive windows after a switch).
                if let Some(view_id) = view_id
                    && state
                        .world
                        .view_workspace(view_id)
                        .is_some_and(|ws| ws != active)
                {
                    continue;
                }
                let color = view_id.map(view_color).unwrap_or(0xFF_60_60_70);
                let local = Rect::new(
                    geo.loc.x.saturating_sub(origin.0),
                    geo.loc.y.saturating_sub(origin.1),
                    u32::try_from(geo.size.w.max(0)).unwrap_or(0),
                    u32::try_from(geo.size.h.max(0)).unwrap_or(0),
                );
                if local.width == 0 || local.height == 0 {
                    continue;
                }
                // Body + lighter title strip.
                rects.push((local, color));
                let bar = local.height.clamp(1, 28);
                let light = lighten_argb(color, 28);
                rects.push((Rect::new(local.x, local.y, local.width, bar), light));
                if view_id.is_some_and(|v| state.world.is_focused(v)) {
                    focus_rect = Some(local);
                }
            }
            if let Some(r) = focus_rect {
                // 2px ring via outer/inner rects (cheap, no scanline outline).
                let ring = 0xFF_7A_A2_F7;
                rects.push((
                    Rect::new(
                        r.x.saturating_sub(2),
                        r.y.saturating_sub(2),
                        r.width.saturating_add(4),
                        2,
                    ),
                    ring,
                ));
                rects.push((
                    Rect::new(
                        r.x.saturating_sub(2),
                        r.y.saturating_add(r.height as i32),
                        r.width.saturating_add(4),
                        2,
                    ),
                    ring,
                ));
                rects.push((Rect::new(r.x.saturating_sub(2), r.y, 2, r.height), ring));
                rects.push((
                    Rect::new(r.x.saturating_add(r.width as i32), r.y, 2, r.height),
                    ring,
                ));
            }
        }
        CaptureKind::Toplevel { size, geometry } => {
            let local = Rect::new(0, 0, size.w.max(0) as u32, size.h.max(0) as u32);
            let body = view_color_from_geometry(geometry);
            rects.push((local, body));
            let bar_h = (size.h.max(0) as u32).clamp(1, 32);
            rects.push((Rect::new(0, 0, local.width, bar_h), lighten_argb(body, 40)));
            // Content placeholder block.
            if local.width > 16 && local.height > bar_h + 16 {
                rects.push((
                    Rect::new(
                        8,
                        bar_h as i32 + 8,
                        local.width.saturating_sub(16),
                        local.height.saturating_sub(bar_h + 16),
                    ),
                    0xFF_28_28_30,
                ));
            }
        }
    }
    rects
}

fn lighten_argb(color: u32, amount: u8) -> u32 {
    let a = color >> 24;
    let r = ((color >> 16) & 0xFF).min(0xFF - u32::from(amount)) + u32::from(amount);
    let g = ((color >> 8) & 0xFF).min(0xFF - u32::from(amount)) + u32::from(amount);
    let b = (color & 0xFF).min(0xFF - u32::from(amount)) + u32::from(amount);
    (a << 24) | (r << 16) | (g << 8) | b
}

fn view_color(view: ViewId) -> u32 {
    let id = view.get();
    let r = 0x40 + ((id.wrapping_mul(37)) as u8 % 0x80);
    let g = 0x40 + ((id.wrapping_mul(17)) as u8 % 0x80);
    let b = 0x50 + ((id.wrapping_mul(53)) as u8 % 0x70);
    0xFF00_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

fn view_color_from_geometry(geometry: Rect) -> u32 {
    let id = (geometry.x as u64)
        .wrapping_mul(31)
        .wrapping_add(geometry.y as u64)
        .wrapping_mul(17)
        .wrapping_add(u64::from(geometry.width));
    view_color(ViewId::new(id.max(1)))
}

fn write_capture_shm(
    buffer: &WlBuffer,
    size: Size<i32, BufferCoords>,
    rects: &[(Rect, u32)],
    blits: &[ShmBlit],
) -> Result<(), CaptureFailureReason> {
    let width = size.w;
    let height = size.h;
    if width <= 0 || height <= 0 {
        return Err(CaptureFailureReason::BufferConstraints);
    }
    with_buffer_contents_mut(buffer, |ptr, len, data| {
        if data.width < width || data.height < height {
            return Err(CaptureFailureReason::BufferConstraints);
        }
        match data.format {
            wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {}
            _ => return Err(CaptureFailureReason::BufferConstraints),
        }
        let stride = data.stride;
        if stride < width.saturating_mul(4) {
            return Err(CaptureFailureReason::BufferConstraints);
        }
        let need = stride
            .checked_mul(height)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(CaptureFailureReason::BufferConstraints)?;
        if need > len {
            return Err(CaptureFailureReason::BufferConstraints);
        }
        #[allow(unsafe_code)]
        let pixels = unsafe { std::slice::from_raw_parts_mut(ptr, need) };
        let clip = Rect::new(0, 0, width as u32, height as u32);
        fill_rect(pixels, stride, width, height, &clip, 0xFF_18_18_1C, clip);
        for (rect, color) in rects {
            fill_rect(pixels, stride, width, height, rect, *color, clip);
        }
        for blit in blits {
            let _ = capture_shm::blit_surface_shm_into(
                &blit.surface,
                pixels,
                stride,
                width,
                height,
                blit.rect,
                clip,
            );
        }
        Ok(())
    })
    .map_err(|err| match err {
        BufferAccessError::NotManaged => CaptureFailureReason::BufferConstraints,
        _ => CaptureFailureReason::Unknown,
    })?
}

fn fill_rect(
    pixels: &mut [u8],
    stride: i32,
    buf_w: i32,
    buf_h: i32,
    rect: &Rect,
    color: u32,
    clip: Rect,
) {
    let Some(rect) = rect.intersection(clip) else {
        return;
    };
    let x0 = rect.x.clamp(0, buf_w);
    let y0 = rect.y.clamp(0, buf_h);
    let x1 = rect.right().clamp(0, buf_w);
    let y1 = rect.bottom().clamp(0, buf_h);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let bytes = color.to_le_bytes();
    for y in y0..y1 {
        let row = (y as usize).saturating_mul(stride as usize);
        for x in x0..x1 {
            let i = row + (x as usize) * 4;
            if i + 4 <= pixels.len() {
                pixels[i..i + 4].copy_from_slice(&bytes);
            }
        }
    }
}

fn constraints_for_output(output: &Output) -> Option<BufferConstraints> {
    let mode = output.current_mode()?;
    Some(shm_constraints(Size::from((mode.size.w, mode.size.h))))
}

fn shm_constraints(size: Size<i32, BufferCoords>) -> BufferConstraints {
    BufferConstraints {
        size,
        shm: vec![wl_shm::Format::Xrgb8888, wl_shm::Format::Argb8888],
        #[cfg(feature = "tty")]
        dma: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shm_constraints_advertise_standard_formats() {
        let constraints = shm_constraints(Size::from((1920, 1080)));
        assert_eq!(constraints.size.w, 1920);
        assert!(constraints.shm.contains(&wl_shm::Format::Xrgb8888));
    }

    #[test]
    fn view_color_is_opaque() {
        assert_eq!(view_color(ViewId::new(42)) >> 24, 0xFF);
    }

    #[test]
    fn capture_pixel_budget_rejects_absurd_sizes() {
        let kind = CaptureKind::Output {
            size: Size::from((16_000, 16_000)),
            origin: (0, 0),
        };
        assert!(capture_pixel_count(kind) > MAX_CAPTURE_PIXELS);
    }

    #[test]
    fn fill_rect_writes_expected_pixel() {
        let mut buf = vec![0u8; 8 * 4]; // 2×2, stride 8 bytes? use stride 8 for 2px
        // 2×2 XRGB, stride = 8
        let stride = 8;
        fill_rect(
            &mut buf,
            stride,
            2,
            2,
            &Rect::new(1, 0, 1, 1),
            0xFF_12_34_56,
            Rect::new(0, 0, 2, 2),
        );
        assert_eq!(&buf[4..8], &0xFF_12_34_56u32.to_le_bytes());
    }
}
