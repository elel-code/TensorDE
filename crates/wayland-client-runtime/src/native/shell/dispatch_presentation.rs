//! `wp_presentation` / `wp_presentation_feedback` dispatch.

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::presentation_time::client::{wp_presentation, wp_presentation_feedback};

use super::types::{NativeShellEvent, NativeShellState};

impl Dispatch<wp_presentation::WpPresentation, ()> for NativeShellState {
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

impl Dispatch<wp_presentation_feedback::WpPresentationFeedback, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        feedback: &wp_presentation_feedback::WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let obj = feedback.id().protocol_id();
        match event {
            wp_presentation_feedback::Event::SyncOutput { output } => {
                if let Some(entry) = state.presentation_feedbacks.get_mut(&obj) {
                    let out_id = output.id().protocol_id();
                    entry.sync_output = state.output_objects.get(&out_id).copied();
                }
            }
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                seq_hi,
                seq_lo,
                flags,
            } => {
                let Some(entry) = state.presentation_feedbacks.remove(&obj) else {
                    return;
                };
                state.presentation_pending.remove(&entry.surface);
                let tv_sec = (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo);
                let seq = (u64::from(seq_hi) << 32) | u64::from(seq_lo);
                let flags = match flags {
                    WEnum::Value(f) => f,
                    WEnum::Unknown(raw) => wp_presentation_feedback::Kind::from_bits_truncate(raw),
                };
                state.push(NativeShellEvent::Presented {
                    surface: entry.surface,
                    tv_sec,
                    tv_nsec,
                    refresh_ns: refresh,
                    seq,
                    flags_bits: flags.bits(),
                    sync_output: entry.sync_output,
                });
            }
            wp_presentation_feedback::Event::Discarded => {
                if let Some(entry) = state.presentation_feedbacks.remove(&obj) {
                    state.presentation_pending.remove(&entry.surface);
                    state.push(NativeShellEvent::PresentationDiscarded {
                        surface: entry.surface,
                    });
                }
            }
            _ => {}
        }
    }
}
