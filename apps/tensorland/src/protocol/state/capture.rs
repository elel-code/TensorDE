//! `ext-image-copy-capture` / `ext-image-capture-source` (tier-2 staging).
//!
//! # Performance
//!
//! - Handlers only **queue** frames. TTY output and single-output toplevel
//!   capture use a retained GPU tap and deferred timeline readback; headless
//!   fill remains idle work.
//! - SHM publication and format conversion stay off the page-flip path.
//! - Oversized buffers and DMA client buffers fail honestly.

mod cursor;
#[cfg(feature = "tty")]
mod gpu;
#[cfg(test)]
mod tests;
mod toplevel;

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use tensor_util::BufferSize;
use tracing::{debug, trace, warn};
use wayland_server::protocol::{wl_buffer::WlBuffer, wl_shm};

use super::capture_shm;
use super::{ObjectKey, RuntimeState};
use crate::ecs::ViewId;
use crate::layout::Rect;
use crate::protocol::globals::image_capture_source::ImageCaptureSource;
use crate::protocol::globals::image_copy_capture::{
    BufferConstraints, CaptureFailureReason, CursorSession, Frame, FrameRef, Session, SessionRef,
};
use crate::protocol::globals::output::Output;
use crate::protocol::globals::shm::{BufferAccessError, with_buffer_contents_mut};
#[cfg(feature = "tty")]
use crate::render::{OutputCaptureId, OutputCaptureRequest, OutputCaptureResult};
#[cfg(feature = "tty")]
use gpu::write_gpu_capture_shm;
use toplevel::ToplevelGpuTarget;

/// Max pending capture frames (drop oldest on overflow — capture is lossy).
const MAX_PENDING_CAPTURES: usize = 4;
/// Refuse SHM fills larger than this (width×height).
const MAX_CAPTURE_PIXELS: u32 = 3840 * 2160;
/// Bus timer id used only to ensure an idle turn notices pending captures.
const CAPTURE_TIMER_ID: u64 = 0xC0_FF_EE;

/// Side table for live capture sessions and deferred frame work.
#[derive(Default)]
pub(crate) struct CaptureSessions {
    sessions: Vec<Session>,
    cursor_sessions: Vec<CursorSession>,
    cursor_pending: VecDeque<cursor::PendingCursorCapture>,
    pending: VecDeque<PendingCapture>,
    #[cfg(feature = "tty")]
    in_flight: VecDeque<InFlightCapture>,
    #[cfg(feature = "tty")]
    next_id: u64,
}

struct PendingCapture {
    frame: Frame,
    kind: CaptureKind,
    queued_at: Instant,
}

#[cfg(feature = "tty")]
struct InFlightCapture {
    id: OutputCaptureId,
    frame: Frame,
    queued_at: Instant,
}

#[derive(Clone, Copy, Debug)]
enum CaptureKind {
    /// Full output; `origin` is the output's global logical top-left.
    Output {
        output: crate::protocol::globals::output::OutputInstanceId,
        size: BufferSize<i32>,
        origin: (i32, i32),
        draw_cursors: bool,
    },
    Toplevel {
        size: BufferSize<i32>,
        geometry: Rect,
        draw_cursors: bool,
        gpu_target: Option<ToplevelGpuTarget>,
    },
}

impl RuntimeState {
    pub(in crate::protocol) fn capture_constraints_for_source(
        &self,
        source: &ImageCaptureSource,
    ) -> Option<BufferConstraints> {
        if let Some(output) = source.output() {
            return constraints_for_output(&output);
        }
        if let Some(key) = source.toplevel_key()
            && let Some((logical_size, geometry)) = self.toplevel_capture_geometry(key)
        {
            #[cfg(feature = "tty")]
            let size = self
                .toplevel_gpu_capture_target(geometry)
                .map(|target| {
                    BufferSize::from((
                        i32::try_from(target.region.width).unwrap_or(i32::MAX),
                        i32::try_from(target.region.height).unwrap_or(i32::MAX),
                    ))
                })
                .unwrap_or(logical_size);
            #[cfg(not(feature = "tty"))]
            let size = {
                let _ = geometry;
                logical_size
            };
            return Some(shm_constraints(size));
        }
        None
    }

    fn toplevel_capture_geometry(&self, key: ObjectKey) -> Option<(BufferSize<i32>, Rect)> {
        for window in self.space.elements() {
            let Some(surface) = window.wl_surface() else {
                continue;
            };
            if ObjectKey::from_surface(&surface) != key {
                continue;
            }
            let geo = self.space.element_geometry(window)?;
            let size = BufferSize::from((geo.size.w.max(1), geo.size.h.max(1)));
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

    pub(in crate::protocol) fn store_capture_session(&mut self, session: Session) {
        self.protocol_side.capture.sessions.push(session);
    }

    pub(in crate::protocol) fn drop_capture_session(&mut self, session: &SessionRef) {
        self.protocol_side
            .capture
            .sessions
            .retain(|stored| stored.as_ref() != session);
    }

    pub(in crate::protocol) fn abort_capture_frame(&mut self, frame: &FrameRef) {
        self.protocol_side
            .capture
            .cursor_pending
            .retain(|pending| pending.frame.as_ref() != frame);
        self.protocol_side
            .capture
            .pending
            .retain(|pending| pending.frame.as_ref() != frame);
        #[cfg(feature = "tty")]
        self.protocol_side
            .capture
            .in_flight
            .retain(|capture| capture.frame.as_ref() != frame);
    }

    pub(in crate::protocol) fn handle_capture_frame(&mut self, session: &SessionRef, frame: Frame) {
        if let Some(cursor) = session.cursor() {
            self.queue_cursor_capture_frame(cursor, frame);
            return;
        }
        let Some(kind) = capture_kind_for_session(self, session) else {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        };
        #[cfg(feature = "tty")]
        if matches!(
            kind,
            CaptureKind::Toplevel {
                gpu_target: None,
                ..
            }
        ) && self.renderer.is_some()
        {
            frame.fail(CaptureFailureReason::Unknown);
            return;
        }
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
        #[cfg(feature = "tty")]
        if let Some(output) = capture_output(kind)
            && let Some(output_id) = self
                .outputs
                .iter()
                .find_map(|(id, managed)| (managed.output.instance_id() == output).then_some(*id))
        {
            self.queue_redraw(output_id);
        }
        // Ensure the next idle turn runs without forcing a CRTC redraw.
        let _ = self.push_event(tensor_event::Event::Timer(tensor_event::TimerId(
            CAPTURE_TIMER_ID,
        )));
    }

    /// Drain at most one pending capture fill (event idle turn).
    pub(crate) fn process_pending_captures(&mut self) {
        #[cfg(feature = "tty")]
        self.process_completed_gpu_captures();
        if self.process_pending_cursor_capture() {
            return;
        }
        let software_index = self
            .protocol_side
            .capture
            .pending
            .iter()
            .position(|pending| {
                matches!(
                    pending.kind,
                    CaptureKind::Toplevel {
                        gpu_target: None,
                        ..
                    }
                ) || {
                    #[cfg(feature = "tty")]
                    {
                        self.renderer.is_none()
                    }
                    #[cfg(not(feature = "tty"))]
                    {
                        true
                    }
                }
            });
        if let Some(pending) =
            software_index.and_then(|index| self.protocol_side.capture.pending.remove(index))
        {
            if !pending.frame.is_alive() {
                return;
            }
            if pending.frame.session_stopped() {
                pending.frame.fail(CaptureFailureReason::Stopped);
                return;
            }
            let wait = pending.queued_at.elapsed();
            if fill_capture_frame(self, pending.frame, pending.kind).is_ok() {
                trace!(
                    ?wait,
                    "image-copy-capture frame filled (software silhouette)"
                );
            }
        }
    }

    #[cfg(feature = "tty")]
    pub(super) fn has_pending_output_capture(&self, output: &Output) -> bool {
        self.protocol_side.capture.pending.iter().any(|pending| {
            capture_output(pending.kind).is_some_and(|instance| instance == output.instance_id())
        })
    }

    #[cfg(feature = "tty")]
    pub(super) fn take_output_capture(
        &mut self,
        output: &Output,
        region: Rect,
    ) -> Option<OutputCaptureRequest> {
        let index = self
            .protocol_side
            .capture
            .pending
            .iter()
            .position(|pending| {
                capture_output(pending.kind)
                    .is_some_and(|instance| instance == output.instance_id())
            })?;
        let pending = self.protocol_side.capture.pending.remove(index)?;
        let (region, draw_cursors) = match pending.kind {
            CaptureKind::Output { draw_cursors, .. } => (region, draw_cursors),
            CaptureKind::Toplevel {
                draw_cursors,
                gpu_target: Some(target),
                ..
            } => (target.region, draw_cursors),
            CaptureKind::Toplevel {
                gpu_target: None, ..
            } => return None,
        };
        self.protocol_side.capture.next_id = self
            .protocol_side
            .capture
            .next_id
            .checked_add(1)
            .expect("capture request identity space exhausted");
        let id = OutputCaptureId::new(self.protocol_side.capture.next_id);
        self.protocol_side
            .capture
            .in_flight
            .push_back(InFlightCapture {
                id,
                frame: pending.frame,
                queued_at: pending.queued_at,
            });
        Some(OutputCaptureRequest {
            id,
            region,
            draw_cursors,
        })
    }

    #[cfg(feature = "tty")]
    fn process_completed_gpu_captures(&mut self) {
        let results = match self.renderer.as_mut() {
            Some(renderer) => match renderer.drain_completed_captures() {
                Ok(results) => results,
                Err(error) => {
                    warn!(%error, "failed to query output capture readback completion");
                    return;
                }
            },
            None => return,
        };
        for result in results {
            let id = result.id();
            let Some(index) = self
                .protocol_side
                .capture
                .in_flight
                .iter()
                .position(|capture| capture.id == id)
            else {
                continue;
            };
            let Some(capture) = self.protocol_side.capture.in_flight.remove(index) else {
                continue;
            };
            if !capture.frame.is_alive() {
                continue;
            }
            if capture.frame.session_stopped() {
                capture.frame.fail(CaptureFailureReason::Stopped);
                continue;
            }
            match result {
                OutputCaptureResult::Ready(pixels) => {
                    match write_gpu_capture_shm(&capture.frame.buffer(), &pixels) {
                        Ok(()) => {
                            let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
                            capture
                                .frame
                                .success(Duration::new(now.tv_sec as u64, now.tv_nsec as u32));
                            trace!(wait = ?capture.queued_at.elapsed(), "GPU output capture published");
                        }
                        Err(reason) => capture.frame.fail(reason),
                    }
                }
                OutputCaptureResult::Failed { reason, .. } => {
                    warn!(%reason, "GPU output capture failed");
                    capture.frame.fail(CaptureFailureReason::Unknown);
                }
            }
        }
    }
}

fn capture_kind_for_session(state: &RuntimeState, session: &SessionRef) -> Option<CaptureKind> {
    let source = session.source();
    if let Some(output) = source.output() {
        let mode = output.current_mode()?;
        let origin = state
            .space
            .output_geometry(&output)
            .map(|geo| (geo.loc.x, geo.loc.y))
            .unwrap_or((0, 0));
        return Some(CaptureKind::Output {
            output: output.instance_id(),
            size: BufferSize::from((mode.width, mode.height)),
            origin,
            draw_cursors: session.draw_cursors(),
        });
    }
    if let Some(key) = source.toplevel_key()
        && let Some((logical_size, geometry)) = state.toplevel_capture_geometry(key)
    {
        #[cfg(feature = "tty")]
        let gpu_target = state.toplevel_gpu_capture_target(geometry);
        #[cfg(not(feature = "tty"))]
        let gpu_target = None;
        let size = gpu_target
            .map(|target: ToplevelGpuTarget| {
                BufferSize::from((
                    i32::try_from(target.region.width).unwrap_or(i32::MAX),
                    i32::try_from(target.region.height).unwrap_or(i32::MAX),
                ))
            })
            .unwrap_or(logical_size);
        return Some(CaptureKind::Toplevel {
            size,
            geometry,
            draw_cursors: session.draw_cursors(),
            gpu_target,
        });
    }
    None
}

#[cfg(feature = "tty")]
fn capture_output(kind: CaptureKind) -> Option<crate::protocol::globals::output::OutputInstanceId> {
    match kind {
        CaptureKind::Output { output, .. } => Some(output),
        CaptureKind::Toplevel {
            gpu_target: Some(target),
            ..
        } => Some(target.output),
        CaptureKind::Toplevel {
            gpu_target: None, ..
        } => None,
    }
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
            let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
            let presented = Duration::new(now.tv_sec as u64, now.tv_nsec as u32);
            frame.success(presented);
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
        CaptureKind::Toplevel { size, geometry, .. } => {
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
/// Order is back-to-front: backdrop, then windows (WindowSpace stacking order),
/// then a thin focus ring on the active view when capturing an output.
fn silhouette_rects(state: &RuntimeState, kind: CaptureKind) -> Vec<(Rect, u32)> {
    let mut rects = Vec::new();
    match kind {
        CaptureKind::Output { size, origin, .. } => {
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
        CaptureKind::Toplevel { size, geometry, .. } => {
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
    size: BufferSize<i32>,
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
    Some(shm_constraints(BufferSize::from((mode.width, mode.height))))
}

fn shm_constraints(size: BufferSize<i32>) -> BufferConstraints {
    BufferConstraints {
        size,
        shm: [wl_shm::Format::Xrgb8888, wl_shm::Format::Argb8888],
        shm_count: 2,
    }
}
