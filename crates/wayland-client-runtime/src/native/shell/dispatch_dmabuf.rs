//! `zwp_linux_dmabuf_v1` dispatch (formats, feedback, params, buffer release).

use std::os::fd::AsFd;
use std::slice;

use wayland_client::protocol::wl_buffer;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
    zwp_linux_buffer_params_v1, zwp_linux_dmabuf_feedback_v1, zwp_linux_dmabuf_v1,
};

use super::types::{NativeShellEvent, NativeShellState};
use crate::dmabuf::{DmabufFeedback, DmabufFeedbackTranche, DmabufFormat, DmabufTrancheFlags};

impl Dispatch<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        proxy: &zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
        event: zwp_linux_dmabuf_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            // Formats are duplicated by modifier events since v3; ignore like Mesa/SCTK.
            zwp_linux_dmabuf_v1::Event::Format { .. } => {}
            zwp_linux_dmabuf_v1::Event::Modifier {
                format,
                modifier_hi,
                modifier_lo,
            }
                // v4+ uses feedback format tables; only collect legacy modifiers.
                if proxy.version() < 4 => {
                    let modifier = (u64::from(modifier_hi) << 32) | u64::from(modifier_lo);
                    state
                        .dmabuf_modifiers
                        .push(DmabufFormat::new(format, modifier));
                }
            _ => {}
        }
    }
}

impl Dispatch<zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        proxy: &zwp_linux_dmabuf_feedback_v1::ZwpLinuxDmabufFeedbackV1,
        event: zwp_linux_dmabuf_feedback_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let pid = proxy.id().protocol_id();
        match event {
            zwp_linux_dmabuf_feedback_v1::Event::FormatTable { fd, size } => {
                let size = size as usize;
                let entry = std::mem::size_of::<DmabufFormat>();
                if size == 0 || !size.is_multiple_of(entry) {
                    return;
                }
                let len = size / entry;
                // MAP_PRIVATE copy-on-write so the compositor can reuse the fd.
                // SAFETY: `fd` is a live compositor-owned format table; size is a
                // multiple of `DmabufFormat` and non-zero (checked above). We map
                // read-only PRIVATE and unmap before the fd is dropped.
                let map = match unsafe {
                    rustix::mm::mmap(
                        std::ptr::null_mut(),
                        size,
                        rustix::mm::ProtFlags::READ,
                        rustix::mm::MapFlags::PRIVATE,
                        fd.as_fd(),
                        0,
                    )
                } {
                    Ok(ptr) => ptr,
                    Err(_) => return,
                };
                // SAFETY: `map` is a successful mmap of `size` bytes holding `len`
                // densely packed `DmabufFormat` values (layout matches the protocol
                // table). We copy out then munmap the same range.
                let formats = unsafe {
                    let slice = slice::from_raw_parts(map as *const DmabufFormat, len);
                    let owned = slice.to_vec();
                    let _ = rustix::mm::munmap(map, size);
                    owned
                };
                let pending = state.dmabuf_feedback_pending.entry(pid).or_default();
                pending.formats = formats;
            }
            zwp_linux_dmabuf_feedback_v1::Event::MainDevice { device } => {
                let main_device = device_bytes_to_u64(&device);
                state
                    .dmabuf_feedback_pending
                    .entry(pid)
                    .or_default()
                    .main_device = main_device;
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheTargetDevice { device } => {
                let device = device_bytes_to_u64(&device);
                state
                    .dmabuf_tranche_pending
                    .entry(pid)
                    .or_default()
                    .device = device;
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheFlags { flags } => {
                let bits = match flags {
                    WEnum::Value(f) => f.bits(),
                    WEnum::Unknown(raw) => raw,
                };
                state
                    .dmabuf_tranche_pending
                    .entry(pid)
                    .or_default()
                    .flags = DmabufTrancheFlags::from_bits_truncate(bits);
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheFormats { indices } => {
                if !indices.len().is_multiple_of(2) {
                    return;
                }
                let formats = indices
                    .chunks_exact(2)
                    .map(|c| u16::from_ne_bytes([c[0], c[1]]))
                    .collect();
                state
                    .dmabuf_tranche_pending
                    .entry(pid)
                    .or_default()
                    .formats = formats;
            }
            zwp_linux_dmabuf_feedback_v1::Event::TrancheDone => {
                let tranche = state
                    .dmabuf_tranche_pending
                    .remove(&pid)
                    .unwrap_or_default();
                state
                    .dmabuf_feedback_pending
                    .entry(pid)
                    .or_default()
                    .tranches
                    .push(DmabufFeedbackTranche {
                        device: tranche.device,
                        flags: tranche.flags,
                        formats: tranche.formats,
                    });
            }
            zwp_linux_dmabuf_feedback_v1::Event::Done => {
                let build = state
                    .dmabuf_feedback_pending
                    .remove(&pid)
                    .unwrap_or_default();
                let surface = state.dmabuf_feedback_surfaces.get(&pid).copied();
                let feedback = DmabufFeedback {
                    main_device: build.main_device,
                    formats: build.formats,
                    tranches: build.tranches,
                };
                if surface.is_none() {
                    state.dmabuf_default_feedback = Some(feedback.clone());
                } else if let Some(sid) = surface {
                    state.dmabuf_surface_feedback.insert(sid, feedback.clone());
                }
                state.push(NativeShellEvent::DmabufFeedback {
                    surface,
                    feedback,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, ()> for NativeShellState {
    fn event(
        state: &mut Self,
        params: &zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let params_id = params.id().protocol_id();
        match event {
            zwp_linux_buffer_params_v1::Event::Created { buffer } => {
                let buffer_proto = buffer.id().protocol_id();
                let id = state.next_dmabuf_buffer_id;
                state.next_dmabuf_buffer_id = state.next_dmabuf_buffer_id.saturating_add(1);
                state.dmabuf_buffers.insert(
                    id,
                    super::types::DmabufBufferRecord {
                        buffer,
                        params_proto: Some(params_id),
                    },
                );
                state.dmabuf_buffer_by_proto.insert(buffer_proto, id);
                state.dmabuf_params.remove(&params_id);
                params.destroy();
                state.push(NativeShellEvent::DmabufBufferCreated {
                    id: crate::dmabuf::DmabufBufferId(id),
                });
            }
            zwp_linux_buffer_params_v1::Event::Failed => {
                state.dmabuf_params.remove(&params_id);
                params.destroy();
                state.push(NativeShellEvent::DmabufBufferFailed);
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(NativeShellState, zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1, [
        zwp_linux_buffer_params_v1::EVT_CREATED_OPCODE => (wl_buffer::WlBuffer, ())
    ]);
}

/// Decode `array` device id (`dev_t`) from the protocol wire format.
fn device_bytes_to_u64(device: &[u8]) -> u64 {
    match device.len() {
        8 => u64::from_ne_bytes(device.try_into().unwrap_or([0; 8])),
        4 => u64::from(u32::from_ne_bytes(device.try_into().unwrap_or([0; 4]))),
        _ => {
            let mut buf = [0u8; 8];
            let n = device.len().min(8);
            buf[..n].copy_from_slice(&device[..n]);
            u64::from_ne_bytes(buf)
        }
    }
}
