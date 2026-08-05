//! Tensor-owned `zwp_linux_dmabuf_v1` wire adapter.
//!
//! Validation and feedback sequencing are adapted from Smithay's protocol
//! implementation. See `LICENSES/Smithay-MIT.txt`.

mod feedback;
mod params;

use std::{os::fd::OwnedFd, sync::Arc};

use tensor_host::{DrmFormat, Modifier};
use wayland_protocols::wp::linux_dmabuf::zv1::server::{
    zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1,
    zwp_linux_dmabuf_feedback_v1::{self, ZwpLinuxDmabufFeedbackV1},
    zwp_linux_dmabuf_v1::{self, ZwpLinuxDmabufV1},
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, New, Resource,
    backend::{ClientId, GlobalId},
    protocol::wl_buffer::{self, WlBuffer},
};

use crate::{
    ecs::SurfaceBufferId,
    protocol::{
        dispatch::{
            DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
        },
        state::RuntimeState,
    },
};

use feedback::DmabufFeedback;
pub(in crate::protocol) use params::DmabufParamsData;

const VERSION: u32 = 6;

pub(in crate::protocol) trait DmabufImportHandler: 'static {
    fn import_dmabuf(&mut self, buffer: &DmabufBuffer) -> Result<SurfaceBufferId, String>;

    fn register_dmabuf_buffer(
        &mut self,
        buffer: &WlBuffer,
        id: SurfaceBufferId,
        size: tensor_util::Size,
    ) -> bool;

    fn release_dmabuf_import(&mut self, id: SurfaceBufferId);

    fn dmabuf_buffer_destroyed(&mut self, buffer: &WlBuffer);
}

#[derive(Debug)]
pub(in crate::protocol) struct DmabufBuffer {
    descriptor: crate::render::Dmabuf<OwnedFd>,
    flags: u32,
}

impl DmabufBuffer {
    pub(in crate::protocol) fn new(descriptor: crate::render::Dmabuf<OwnedFd>, flags: u32) -> Self {
        Self { descriptor, flags }
    }

    pub(in crate::protocol) fn descriptor(&self) -> &crate::render::Dmabuf<OwnedFd> {
        &self.descriptor
    }

    pub(in crate::protocol) fn size(&self) -> tensor_util::Size {
        self.descriptor.size
    }

    pub(in crate::protocol) fn flags(&self) -> u32 {
        self.flags
    }
}

pub(in crate::protocol) fn dmabuf_buffer(buffer: &WlBuffer) -> Option<&DmabufBuffer> {
    buffer.data::<DmabufBuffer>()
}

pub(in crate::protocol) fn is_dmabuf_buffer(buffer: &WlBuffer) -> bool {
    dmabuf_buffer(buffer).is_some()
}

/// Owns the immutable default feedback and its advertised global.
pub(crate) struct DmabufProtocol {
    global: Option<GlobalId>,
    feedback: Option<Arc<DmabufFeedback>>,
}

impl DmabufProtocol {
    pub(crate) fn new() -> Self {
        Self {
            global: None,
            feedback: None,
        }
    }

    pub(crate) fn install(
        &mut self,
        display: &DisplayHandle,
        main_device: u64,
        formats: impl IntoIterator<Item = DrmFormat>,
    ) -> Result<bool, String> {
        if self.global.is_some() {
            return Err("linux-dmabuf global was installed more than once".to_owned());
        }
        let mut unique = Vec::new();
        for format in formats {
            if !unique.contains(&format) {
                unique.push(format);
            }
        }
        if unique.is_empty() {
            return Ok(false);
        }
        let formats: Arc<[DrmFormat]> = unique.into();
        let feedback = Arc::new(
            DmabufFeedback::new(main_device, &formats).map_err(|error| error.to_string())?,
        );
        let global = display.create_global::<RuntimeState, ZwpLinuxDmabufV1, _>(
            VERSION,
            DmabufGlobalData {
                formats,
                feedback: Arc::clone(&feedback),
            },
        );
        self.global = Some(global);
        self.feedback = Some(feedback);
        Ok(true)
    }

    pub(crate) fn advertised(&self) -> bool {
        self.global.is_some() && self.feedback.is_some()
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct DmabufGlobalData {
    formats: Arc<[DrmFormat]>,
    feedback: Arc<DmabufFeedback>,
}

#[derive(Debug)]
pub(in crate::protocol) struct DmabufInstanceData {
    formats: Arc<[DrmFormat]>,
    feedback: Arc<DmabufFeedback>,
}

#[derive(Debug)]
pub(in crate::protocol) struct DmabufFeedbackData;

impl<D> GlobalDispatchDelegate<ZwpLinuxDmabufV1, D> for DmabufGlobalData
where
    D: Dispatch<ZwpLinuxDmabufV1, DmabufInstanceData>,
    D: 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpLinuxDmabufV1>,
        data_init: &mut DataInit<'_, D>,
    ) {
        let dmabuf = data_init.init(
            resource,
            DmabufInstanceData {
                formats: Arc::clone(&self.formats),
                feedback: Arc::clone(&self.feedback),
            },
        );
        if dmabuf.version() >= 4 {
            return;
        }
        for (index, format) in self.formats.iter().copied().enumerate() {
            if dmabuf.version() < 3 {
                let first_code = !self.formats[..index]
                    .iter()
                    .any(|known| known.code == format.code);
                if first_code
                    && (format.modifier == Modifier::INVALID || format.modifier == Modifier::LINEAR)
                {
                    dmabuf.format(format.code.raw());
                }
                continue;
            }
            let modifier = format.modifier.raw();
            dmabuf.modifier(format.code.raw(), (modifier >> 32) as u32, modifier as u32);
        }
    }
}

impl<D> DispatchDelegate<ZwpLinuxDmabufV1, D> for DmabufInstanceData
where
    D: Dispatch<ZwpLinuxBufferParamsV1, DmabufParamsData>,
    D: Dispatch<ZwpLinuxDmabufFeedbackV1, DmabufFeedbackData>,
    D: DmabufImportHandler,
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &ZwpLinuxDmabufV1,
        request: zwp_linux_dmabuf_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwp_linux_dmabuf_v1::Request::Destroy => {}
            zwp_linux_dmabuf_v1::Request::CreateParams { params_id } => {
                data_init.init(params_id, DmabufParamsData::new(Arc::clone(&self.formats)));
            }
            zwp_linux_dmabuf_v1::Request::GetDefaultFeedback { id }
            | zwp_linux_dmabuf_v1::Request::GetSurfaceFeedback { id, .. } => {
                let feedback = data_init.init(id, DmabufFeedbackData);
                self.feedback.send(&feedback);
            }
            _ => unreachable!(),
        }
    }
}

impl<D> DispatchDelegate<ZwpLinuxDmabufFeedbackV1, D> for DmabufFeedbackData
where
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &ZwpLinuxDmabufFeedbackV1,
        request: zwp_linux_dmabuf_feedback_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            zwp_linux_dmabuf_feedback_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> DispatchDelegate<WlBuffer, D> for DmabufBuffer
where
    D: DmabufImportHandler,
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _resource: &WlBuffer,
        request: wl_buffer::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            wl_buffer::Request::Destroy => {}
            _ => unreachable!(),
        }
    }

    fn destroyed(&self, state: &mut D, _client: ClientId, resource: &WlBuffer) {
        state.dmabuf_buffer_destroyed(resource);
    }
}

delegate_global_dispatch!(RuntimeState, ZwpLinuxDmabufV1, DmabufGlobalData);
delegate_dispatch!(RuntimeState, ZwpLinuxDmabufV1, DmabufInstanceData);
delegate_dispatch!(RuntimeState, ZwpLinuxDmabufFeedbackV1, DmabufFeedbackData);
delegate_dispatch!(RuntimeState, ZwpLinuxBufferParamsV1, DmabufParamsData);
delegate_dispatch!(RuntimeState, WlBuffer, DmabufBuffer);

#[cfg(test)]
mod tests {
    use tensor_host::{Fourcc, Modifier};
    use wayland_server::Display;

    use super::*;

    #[test]
    fn feedback_global_is_created_only_for_a_nonempty_import_contract() {
        let display = Display::<RuntimeState>::new().unwrap();
        let mut protocol = DmabufProtocol::new();
        assert!(
            !protocol
                .install(&display.handle(), 0, std::iter::empty())
                .unwrap()
        );
        assert!(!protocol.advertised());

        let format = DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9));
        assert!(protocol.install(&display.handle(), 0, [format]).unwrap());
        assert!(protocol.advertised());
    }
}
