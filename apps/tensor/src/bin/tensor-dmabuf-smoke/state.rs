use std::{
    collections::{BTreeSet, HashMap},
    os::fd::AsFd,
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::GlobalListContents,
    protocol::{wl_buffer, wl_callback, wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::{
    wp::{
        linux_dmabuf::zv1::client::{
            zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1, zwp_linux_dmabuf_v1,
        },
        presentation_time::client::{wp_presentation, wp_presentation_feedback},
    },
    xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base},
};

use super::{
    SmokeError,
    buffer_pool::BufferPool,
    feedback::{FeedbackState, ReadyFeedback},
};

pub(super) struct SmokeState {
    surface: wl_surface::WlSurface,
    _xdg_surface: xdg_surface::XdgSurface,
    _toplevel: xdg_toplevel::XdgToplevel,
    presentation: wp_presentation::WpPresentation,
    feedback: FeedbackState,
    params: Vec<Option<zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1>>,
    buffers: Vec<Option<wl_buffer::WlBuffer>>,
    buffer_slots: HashMap<wayland_client::backend::ObjectId, usize>,
    configured: bool,
    expected_frames: usize,
    sent_frames: usize,
    frame_callbacks: BTreeSet<usize>,
    presentations: BTreeSet<usize>,
    released: BTreeSet<usize>,
    presentation_clock_id: Option<u32>,
    failure: Option<String>,
}

impl SmokeState {
    pub(super) fn new(
        surface: wl_surface::WlSurface,
        xdg_surface: xdg_surface::XdgSurface,
        toplevel: xdg_toplevel::XdgToplevel,
        presentation: wp_presentation::WpPresentation,
        expected_frames: usize,
    ) -> Self {
        Self {
            surface,
            _xdg_surface: xdg_surface,
            _toplevel: toplevel,
            presentation,
            feedback: FeedbackState::default(),
            params: (0..expected_frames).map(|_| None).collect(),
            buffers: (0..expected_frames).map(|_| None).collect(),
            buffer_slots: HashMap::new(),
            configured: false,
            expected_frames,
            sent_frames: 0,
            frame_callbacks: BTreeSet::new(),
            presentations: BTreeSet::new(),
            released: BTreeSet::new(),
            presentation_clock_id: None,
            failure: None,
        }
    }

    pub(super) fn request_buffers(
        &mut self,
        dmabuf: &zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
        queue_handle: &QueueHandle<Self>,
        pool: &BufferPool,
    ) -> Result<(), SmokeError> {
        for (slot, buffer) in pool.buffers.iter().enumerate() {
            let fd = buffer.fd_for_plane(0).map_err(|source| SmokeError::Gbm {
                context: "export dma-buf plane",
                source: std::io::Error::other(source.to_string()),
            })?;
            let params = dmabuf.create_params(queue_handle, slot);
            let modifier_hi = (pool.format.modifier >> 32) as u32;
            let modifier_lo = pool.format.modifier as u32;
            params.add(
                fd.as_fd(),
                0,
                buffer.offset(0),
                buffer.stride_for_plane(0),
                modifier_hi,
                modifier_lo,
            );
            params.create(
                i32::try_from(buffer.width()).map_err(|_| SmokeError::DimensionsTooLarge)?,
                i32::try_from(buffer.height()).map_err(|_| SmokeError::DimensionsTooLarge)?,
                pool.format.fourcc,
                zwp_linux_buffer_params_v1::Flags::empty(),
            );
            self.params[slot] = Some(params);
        }
        Ok(())
    }

    pub(super) fn commit_initial_surface(&self) {
        self.surface.commit();
    }

    pub(super) fn take_feedback(&mut self) -> Result<ReadyFeedback, SmokeError> {
        self.feedback.take_ready()
    }

    pub(super) fn check_failure(&mut self) -> Result<(), SmokeError> {
        if let Some(error) = self.failure.take() {
            return Err(SmokeError::Health(error));
        }
        if let Some(error) = self.feedback.take_error() {
            return Err(SmokeError::InvalidFeedback(error));
        }
        Ok(())
    }

    pub(super) fn is_healthy(&self) -> bool {
        health_satisfied(
            self.created_buffer_count(),
            self.expected_frames,
            self.sent_frames,
            self.frame_callbacks.len(),
            self.presentations.len(),
            self.released.len(),
            self.presentation_clock_id.is_some(),
        )
    }

    pub(super) fn success_report(&self) -> String {
        format!(
            "created={} frame_callbacks={} presented={} released={} clock_id={}",
            self.created_buffer_count(),
            self.frame_callbacks.len(),
            self.presentations.len(),
            self.released.len(),
            self.presentation_clock_id.unwrap_or_default(),
        )
    }

    pub(super) fn progress(&self) -> String {
        format!(
            "created={}/{} configured={} submitted={}/{} frame_callbacks={}/{} presented={}/{} released={}/{} clock_id={}",
            self.created_buffer_count(),
            self.expected_frames,
            self.configured,
            self.sent_frames,
            self.expected_frames,
            self.frame_callbacks.len(),
            self.expected_frames,
            self.presentations.len(),
            self.expected_frames,
            self.released.len(),
            self.expected_frames.saturating_sub(1),
            self.presentation_clock_id.is_some(),
        )
    }

    fn maybe_submit(&mut self, queue_handle: &QueueHandle<Self>) {
        if self.failure.is_some()
            || !self.configured
            || self.sent_frames == self.expected_frames
            || self.buffers.iter().any(Option::is_none)
        {
            return;
        }
        let frame = self.sent_frames;
        let Some(buffer) = self.buffers[frame].as_ref() else {
            return;
        };
        self.presentation
            .feedback(&self.surface, queue_handle, frame);
        self.surface.frame(queue_handle, frame);
        self.surface.attach(Some(buffer), 0, 0);
        self.surface.damage(0, 0, i32::MAX, i32::MAX);
        self.surface.commit();
        self.sent_frames += 1;
        println!("tensor-dmabuf-smoke: submitted buffer slot={frame}");
    }

    fn note_created(
        &mut self,
        slot: usize,
        buffer: wl_buffer::WlBuffer,
        queue_handle: &QueueHandle<Self>,
    ) {
        if self.buffers.get(slot).is_none() {
            self.fail(format!("compositor created unexpected dma-buf slot {slot}"));
            return;
        }
        self.buffer_slots.insert(buffer.id(), slot);
        self.buffers[slot] = Some(buffer);
        self.params[slot] = None;
        println!("tensor-dmabuf-smoke: dma-buf import accepted slot={slot}");
        self.maybe_submit(queue_handle);
    }

    fn note_release(&mut self, buffer: &wl_buffer::WlBuffer) {
        let Some(slot) = self.buffer_slots.get(&buffer.id()).copied() else {
            self.fail("compositor released an unknown dma-buf".to_owned());
            return;
        };
        self.released.insert(slot);
        println!("tensor-dmabuf-smoke: compositor released buffer slot={slot}");
    }

    fn created_buffer_count(&self) -> usize {
        self.buffers.iter().flatten().count()
    }

    fn fail(&mut self, message: String) {
        self.failure.get_or_insert(message);
    }
}

fn health_satisfied(
    created: usize,
    expected: usize,
    submitted: usize,
    frame_callbacks: usize,
    presentations: usize,
    released: usize,
    has_clock_id: bool,
) -> bool {
    created == expected
        && submitted == expected
        && frame_callbacks == expected
        && presentations == expected
        && released >= expected.saturating_sub(1)
        && has_clock_id
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for SmokeState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(SmokeState: ignore wl_compositor::WlCompositor);
delegate_noop!(SmokeState: ignore wl_surface::WlSurface);
delegate_noop!(SmokeState: ignore zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1);

impl Dispatch<zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1, ()> for SmokeState {
    fn event(
        state: &mut Self,
        _: &zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
        event: zwp_linux_dmabuf_feedback_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_linux_dmabuf_feedback_v1::Event::Done => state.feedback.mark_done(),
            zwp_linux_dmabuf_feedback_v1::Event::FormatTable { fd, size } => {
                state.feedback.record_format_table(fd, size)
            }
            zwp_linux_dmabuf_feedback_v1::Event::MainDevice { device } => {
                state.feedback.record_main_device(&device)
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheFormats { indices } => {
                state.feedback.record_indices(&indices)
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheDone => state.feedback.finish_tranche(),
            zwp_linux_dmabuf_feedback_v1::Event::TrancheTargetDevice { .. }
            | zwp_linux_dmabuf_feedback_v1::Event::TrancheFlags { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, usize> for SmokeState {
    fn event(
        state: &mut Self,
        params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        slot: &usize,
        _: &Connection,
        queue_handle: &QueueHandle<Self>,
    ) {
        match event {
            zwp_linux_buffer_params_v1::Event::Created { buffer } => {
                params.destroy();
                state.note_created(*slot, buffer, queue_handle);
            }
            zwp_linux_buffer_params_v1::Event::Failed => {
                params.destroy();
                state.fail(format!(
                    "compositor rejected dma-buf import for slot {slot}"
                ));
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(
        SmokeState,
        zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        [zwp_linux_buffer_params_v1::EVT_CREATED_OPCODE => (wl_buffer::WlBuffer, ())]
    );
}

impl Dispatch<wl_buffer::WlBuffer, ()> for SmokeState {
    fn event(
        state: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_buffer::Event::Release) {
            state.note_release(buffer);
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for SmokeState {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for SmokeState {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        queue_handle: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured = true;
            println!("tensor-dmabuf-smoke: xdg surface configured");
            state.maybe_submit(queue_handle);
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for SmokeState {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(event, xdg_toplevel::Event::Close) {
            state.fail("compositor closed the smoke toplevel".to_owned());
        }
    }
}

impl Dispatch<wl_callback::WlCallback, usize> for SmokeState {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        frame: &usize,
        _: &Connection,
        queue_handle: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_callback::Event::Done { .. }) {
            state.frame_callbacks.insert(*frame);
            println!("tensor-dmabuf-smoke: frame callback slot={frame}");
            state.maybe_submit(queue_handle);
        }
    }
}

impl Dispatch<wp_presentation::WpPresentation, ()> for SmokeState {
    fn event(
        state: &mut Self,
        _: &wp_presentation::WpPresentation,
        event: wp_presentation::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            state.presentation_clock_id = Some(clk_id);
        }
    }
}

impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, usize> for SmokeState {
    fn event(
        state: &mut Self,
        _: &wp_presentation_feedback::WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        frame: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wp_presentation_feedback::Event::Presented { .. } => {
                state.presentations.insert(*frame);
                println!("tensor-dmabuf-smoke: presentation feedback slot={frame}");
            }
            wp_presentation_feedback::Event::Discarded => {
                state.fail(format!(
                    "presentation feedback was discarded for slot {frame}"
                ));
            }
            wp_presentation_feedback::Event::SyncOutput { .. } => {}
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_requires_kms_presentation_and_old_buffer_release() {
        assert!(!health_satisfied(2, 2, 2, 2, 2, 0, true));
        assert!(!health_satisfied(2, 2, 2, 2, 1, 1, true));
        assert!(!health_satisfied(2, 2, 2, 2, 2, 1, false));
        assert!(health_satisfied(2, 2, 2, 2, 2, 1, true));
    }
}
