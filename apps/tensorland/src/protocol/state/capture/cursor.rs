use std::{sync::Arc, time::Duration};

use tensor_util::{BufferSize, OutputScale, Point};
use wayland_server::protocol::{wl_shm, wl_surface::WlSurface};

use super::{
    BufferAccessError, BufferConstraints, CAPTURE_TIMER_ID, CaptureFailureReason, Frame,
    MAX_PENDING_CAPTURES, RuntimeState, capture_shm, with_buffer_contents_mut,
};
use crate::protocol::globals::image_copy_capture::{
    CursorSession, CursorSessionRef, CursorSessionUpdate,
};

#[derive(Clone)]
pub(in crate::protocol) enum CursorCapturePixels {
    Rgba(Arc<[u8]>),
    Surface(WlSurface),
}

#[derive(Clone)]
pub(in crate::protocol) struct CursorCaptureImage {
    pub(in crate::protocol) size: tensor_util::Size,
    pub(in crate::protocol) hotspot: Point,
    pub(in crate::protocol) sample_transform: tensor_protocol::SurfaceSampleTransform,
    pub(in crate::protocol) pixels: CursorCapturePixels,
}

pub(super) struct PendingCursorCapture {
    pub(super) frame: Frame,
    session: CursorSessionRef,
}

struct CursorSourceSpace {
    origin: (f64, f64),
    size: BufferSize<i32>,
    scale: OutputScale,
}

struct CursorSnapshot {
    update: CursorSessionUpdate,
    image: Option<CursorCaptureImage>,
}

impl RuntimeState {
    pub(in crate::protocol) fn store_cursor_capture_session(&mut self, session: CursorSession) {
        if let Some(update) = self.cursor_capture_update_for_session(session.as_ref()) {
            session.as_ref().apply_update(update);
            self.protocol_side.capture.cursor_sessions.push(session);
        }
    }

    pub(in crate::protocol) fn drop_cursor_capture_session(&mut self, session: &CursorSessionRef) {
        self.protocol_side.capture.sessions.retain(|stored| {
            stored
                .as_ref()
                .cursor()
                .is_none_or(|cursor| cursor != *session)
        });
        self.protocol_side
            .capture
            .cursor_pending
            .retain(|pending| pending.session != *session);
        self.protocol_side
            .capture
            .cursor_sessions
            .retain(|stored| stored.as_ref() != session);
    }

    pub(in crate::protocol) fn cursor_capture_update_for_session(
        &self,
        session: &CursorSessionRef,
    ) -> Option<CursorSessionUpdate> {
        self.cursor_snapshot(session)
            .map(|snapshot| snapshot.update)
    }

    pub(crate) fn refresh_cursor_capture_sessions(&mut self) {
        for session in &self.protocol_side.capture.cursor_sessions {
            if let Some(update) = self.cursor_capture_update_for_session(session.as_ref()) {
                session.as_ref().apply_update(update);
            } else {
                session.as_ref().stop();
            }
        }
        if !self.protocol_side.capture.cursor_pending.is_empty() {
            let _ = self.push_event(tensor_event::Event::Timer(tensor_event::TimerId(
                CAPTURE_TIMER_ID,
            )));
        }
    }

    pub(super) fn queue_cursor_capture_frame(&mut self, session: CursorSessionRef, frame: Frame) {
        let pending = &mut self.protocol_side.capture.cursor_pending;
        while pending.len() >= MAX_PENDING_CAPTURES {
            if let Some(old) = pending.pop_front() {
                old.frame.fail(CaptureFailureReason::Unknown);
            }
        }
        pending.push_back(PendingCursorCapture { frame, session });
        let _ = self.push_event(tensor_event::Event::Timer(tensor_event::TimerId(
            CAPTURE_TIMER_ID,
        )));
    }

    pub(super) fn process_pending_cursor_capture(&mut self) -> bool {
        let Some(pending) = self.protocol_side.capture.cursor_pending.pop_front() else {
            return false;
        };
        if !pending.frame.is_alive() {
            return true;
        }
        if pending.frame.session_stopped() {
            pending.frame.fail(CaptureFailureReason::Stopped);
            return true;
        }
        let Some(snapshot) = self.cursor_snapshot(&pending.session) else {
            pending.frame.fail(CaptureFailureReason::Stopped);
            return true;
        };
        pending.session.apply_update(snapshot.update);
        let Some(image) = snapshot.image else {
            self.protocol_side
                .capture
                .cursor_pending
                .push_front(pending);
            return true;
        };
        match write_cursor_shm(&pending.frame, &image) {
            Ok(()) => {
                let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
                pending
                    .frame
                    .success(Duration::new(now.tv_sec as u64, now.tv_nsec as u32));
            }
            Err(reason) => pending.frame.fail(reason),
        }
        true
    }

    #[cfg(feature = "tty")]
    fn cursor_snapshot(&self, session: &CursorSessionRef) -> Option<CursorSnapshot> {
        if !self.protocol_globals.seat.owns_pointer(&session.pointer()) {
            return None;
        }
        let source = cursor_source_space(self, session)?;
        let image = self
            .cursor
            .pointer_capture_image(source.scale, |surface, scale| {
                crate::protocol::state::surfaces::cursor_surface_raster(
                    &self.surface_buffers,
                    surface,
                    scale,
                )
            });
        if let Some(image) = image.as_ref()
            && let CursorCapturePixels::Surface(surface) = &image.pixels
            && (image.sample_transform != tensor_protocol::SurfaceSampleTransform::IDENTITY
                || !crate::protocol::state::surfaces::surface_buffer(surface).is_some_and(
                    |buffer| {
                        crate::protocol::globals::shm::shm_buffer(&buffer).is_some_and(|shm| {
                            let metadata = shm.metadata();
                            metadata.width == i32::try_from(image.size.width).unwrap_or(i32::MAX)
                                && metadata.height
                                    == i32::try_from(image.size.height).unwrap_or(i32::MAX)
                        })
                    },
                ))
        {
            return None;
        }
        if let Some(image) = image {
            let location = self
                .input_seat
                .pointer_location()
                .map(|location| (location.x, location.y));
            let update = update_for_image(source, &image, location);
            let image = update.position.is_some().then_some(image);
            return Some(CursorSnapshot { update, image });
        }
        session.constraints().map(|constraints| CursorSnapshot {
            update: CursorSessionUpdate {
                constraints,
                position: None,
                hotspot: session.hotspot(),
            },
            image: None,
        })
    }

    #[cfg(not(feature = "tty"))]
    fn cursor_snapshot(&self, _session: &CursorSessionRef) -> Option<CursorSnapshot> {
        None
    }
}

#[cfg(feature = "tty")]
fn cursor_source_space(
    state: &RuntimeState,
    session: &CursorSessionRef,
) -> Option<CursorSourceSpace> {
    let source = session.source();
    if let Some(output) = source.output() {
        let mode = output.current_mode()?;
        let geometry = state.space.output_geometry(&output)?;
        return Some(CursorSourceSpace {
            origin: (f64::from(geometry.loc.x), f64::from(geometry.loc.y)),
            size: BufferSize::from((mode.width, mode.height)),
            scale: output.current_scale(),
        });
    }
    let key = source.toplevel_key()?;
    let (logical_size, geometry) = state.toplevel_capture_geometry(key)?;
    if let Some(target) = state.toplevel_gpu_capture_target(geometry) {
        let scale = state.outputs.values().find_map(|managed| {
            (managed.output.instance_id() == target.output).then(|| managed.output.current_scale())
        })?;
        return Some(CursorSourceSpace {
            origin: (f64::from(geometry.x), f64::from(geometry.y)),
            size: BufferSize::from((
                i32::try_from(target.region.width).ok()?,
                i32::try_from(target.region.height).ok()?,
            )),
            scale,
        });
    }
    (state.renderer.is_none()).then_some(CursorSourceSpace {
        origin: (f64::from(geometry.x), f64::from(geometry.y)),
        size: logical_size,
        scale: OutputScale::ONE,
    })
}

fn update_for_image(
    source: CursorSourceSpace,
    image: &CursorCaptureImage,
    location: Option<(f64, f64)>,
) -> CursorSessionUpdate {
    let position = location.and_then(|(x, y)| {
        let x = source
            .scale
            .physical_coordinate_round(x - source.origin.0)?;
        let y = source
            .scale
            .physical_coordinate_round(y - source.origin.1)?;
        let left = x.saturating_sub(image.hotspot.x);
        let top = y.saturating_sub(image.hotspot.y);
        let right = left.saturating_add_unsigned(image.size.width);
        let bottom = top.saturating_add_unsigned(image.size.height);
        (right > 0 && bottom > 0 && left < source.size.w && top < source.size.h)
            .then_some(Point::new(x, y))
    });
    CursorSessionUpdate {
        constraints: cursor_constraints(image.size),
        position,
        hotspot: image.hotspot,
    }
}

fn cursor_constraints(size: tensor_util::Size) -> BufferConstraints {
    BufferConstraints {
        size: BufferSize::from((
            i32::try_from(size.width).unwrap_or(i32::MAX),
            i32::try_from(size.height).unwrap_or(i32::MAX),
        )),
        shm: [wl_shm::Format::Argb8888, wl_shm::Format::Argb8888],
        shm_count: 1,
    }
}

fn write_cursor_shm(frame: &Frame, image: &CursorCaptureImage) -> Result<(), CaptureFailureReason> {
    let buffer = frame.buffer();
    with_buffer_contents_mut(&buffer, |ptr, len, data| {
        let width =
            i32::try_from(image.size.width).map_err(|_| CaptureFailureReason::BufferConstraints)?;
        let height = i32::try_from(image.size.height)
            .map_err(|_| CaptureFailureReason::BufferConstraints)?;
        if data.width < width
            || data.height < height
            || data.format != wl_shm::Format::Argb8888
            || data.stride < width.saturating_mul(4)
        {
            return Err(CaptureFailureReason::BufferConstraints);
        }
        let need = data
            .stride
            .checked_mul(height)
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(CaptureFailureReason::BufferConstraints)?;
        if need > len {
            return Err(CaptureFailureReason::BufferConstraints);
        }
        #[allow(unsafe_code)]
        let dest = unsafe { std::slice::from_raw_parts_mut(ptr, need) };
        dest.fill(0);
        match &image.pixels {
            CursorCapturePixels::Rgba(pixels) => {
                let row_bytes = usize::try_from(width).unwrap_or(0).saturating_mul(4);
                if pixels.len() != row_bytes.saturating_mul(usize::try_from(height).unwrap_or(0)) {
                    return Err(CaptureFailureReason::Unknown);
                }
                for y in 0..usize::try_from(height).unwrap_or(0) {
                    let source = &pixels[y * row_bytes..(y + 1) * row_bytes];
                    let dest = &mut dest[y * data.stride as usize..][..row_bytes];
                    for (source, dest) in source.chunks_exact(4).zip(dest.chunks_exact_mut(4)) {
                        dest.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
                    }
                }
                Ok(())
            }
            CursorCapturePixels::Surface(surface) => capture_shm::blit_surface_shm_into(
                surface,
                dest,
                data.stride,
                width,
                height,
                crate::layout::Rect::new(0, 0, image.size.width, image.size.height),
                crate::layout::Rect::new(0, 0, image.size.width, image.size.height),
            )
            .then_some(())
            .ok_or(CaptureFailureReason::Unknown),
        }
    })
    .map_err(|error| match error {
        BufferAccessError::NotManaged => CaptureFailureReason::BufferConstraints,
        _ => CaptureFailureReason::Unknown,
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_constraints_require_alpha_shm() {
        let constraints = cursor_constraints(tensor_util::Size::new(32, 24));
        assert_eq!(constraints.size, BufferSize::from((32, 24)));
        assert!(constraints.accepts(32, 24, wl_shm::Format::Argb8888));
        assert!(!constraints.accepts(32, 24, wl_shm::Format::Xrgb8888));
    }
}
