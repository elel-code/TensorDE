//! Tensor-owned `ext-image-capture-source-v1` wire and opaque source handles.

use std::sync::Arc;

use wayland_protocols::ext::image_capture_source::v1::server::{
    ext_foreign_toplevel_image_capture_source_manager_v1::{
        self, ExtForeignToplevelImageCaptureSourceManagerV1,
    },
    ext_image_capture_source_v1::{self, ExtImageCaptureSourceV1},
    ext_output_image_capture_source_manager_v1::{self, ExtOutputImageCaptureSourceManagerV1},
};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, New, Resource, backend::GlobalId};

use crate::protocol::globals::output::{Output, WeakOutput};
use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    state::RuntimeState,
};

use super::foreign_toplevel::{ForeignToplevelWeakHandle, weak_handle_from_resource};

const VERSION: u32 = 1;

pub(crate) struct ImageCaptureSourceProtocol {
    _output_global: GlobalId,
    _toplevel_global: GlobalId,
}

impl ImageCaptureSourceProtocol {
    pub(crate) fn new(
        display: &DisplayHandle,
        filter: impl Fn(&Client) -> bool + Send + Sync + 'static,
    ) -> Self {
        let filter: Arc<dyn Fn(&Client) -> bool + Send + Sync> = Arc::new(filter);
        let output_global = display
            .create_global::<RuntimeState, ExtOutputImageCaptureSourceManagerV1, _>(
                VERSION,
                CaptureSourceGlobalData {
                    filter: Arc::clone(&filter),
                },
            );
        let toplevel_global = display
            .create_global::<RuntimeState, ExtForeignToplevelImageCaptureSourceManagerV1, _>(
                VERSION,
                CaptureSourceGlobalData { filter },
            );
        Self {
            _output_global: output_global,
            _toplevel_global: toplevel_global,
        }
    }
}

pub(in crate::protocol) struct CaptureSourceGlobalData {
    filter: Arc<dyn Fn(&Client) -> bool + Send + Sync>,
}

#[derive(Debug)]
pub(in crate::protocol) struct CaptureSourceManagerData;

#[derive(Clone, Debug)]
enum SourceKind {
    Invalid,
    Output(WeakOutput),
    Toplevel(ForeignToplevelWeakHandle),
}

#[derive(Clone, Debug)]
pub(in crate::protocol) struct ImageCaptureSource {
    kind: Arc<SourceKind>,
}

impl ImageCaptureSource {
    fn new(kind: SourceKind) -> Self {
        Self {
            kind: Arc::new(kind),
        }
    }

    pub(super) fn invalid() -> Self {
        Self::new(SourceKind::Invalid)
    }

    pub(in crate::protocol) fn from_resource(resource: &ExtImageCaptureSourceV1) -> Option<Self> {
        resource
            .data::<ImageCaptureSourceData>()
            .map(|data| data.source.clone())
    }

    pub(in crate::protocol) fn output(&self) -> Option<Output> {
        match self.kind.as_ref() {
            SourceKind::Output(output) => output.upgrade(),
            _ => None,
        }
    }

    pub(in crate::protocol) fn toplevel_key(&self) -> Option<crate::protocol::state::ObjectKey> {
        match self.kind.as_ref() {
            SourceKind::Toplevel(toplevel) => toplevel.live_key(),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct ImageCaptureSourceData {
    source: ImageCaptureSource,
}

impl<D, I> GlobalDispatchDelegate<I, D> for CaptureSourceGlobalData
where
    I: wayland_server::Resource + 'static,
    D: Dispatch<I, CaptureSourceManagerData> + 'static,
{
    fn bind(
        &self,
        _state: &mut D,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<I>,
        data_init: &mut DataInit<'_, D>,
    ) {
        data_init.init(resource, CaptureSourceManagerData);
    }

    fn can_view(&self, client: &Client) -> bool {
        (self.filter)(client)
    }
}

impl<D> DispatchDelegate<ExtImageCaptureSourceV1, D> for ImageCaptureSourceData
where
    D: 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _source: &ExtImageCaptureSourceV1,
        request: ext_image_capture_source_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_image_capture_source_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> DispatchDelegate<ExtOutputImageCaptureSourceManagerV1, D> for CaptureSourceManagerData
where
    D: Dispatch<ExtImageCaptureSourceV1, ImageCaptureSourceData> + 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _manager: &ExtOutputImageCaptureSourceManagerV1,
        request: ext_output_image_capture_source_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_output_image_capture_source_manager_v1::Request::CreateSource {
                source,
                output,
            } => {
                let kind = Output::from_resource(&output)
                    .map(|output| SourceKind::Output(output.downgrade()))
                    .unwrap_or(SourceKind::Invalid);
                data_init.init(
                    source,
                    ImageCaptureSourceData {
                        source: ImageCaptureSource::new(kind),
                    },
                );
            }
            ext_output_image_capture_source_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl<D> DispatchDelegate<ExtForeignToplevelImageCaptureSourceManagerV1, D>
    for CaptureSourceManagerData
where
    D: Dispatch<ExtImageCaptureSourceV1, ImageCaptureSourceData> + 'static,
{
    fn request(
        &self,
        _state: &mut D,
        _client: &Client,
        _manager: &ExtForeignToplevelImageCaptureSourceManagerV1,
        request: ext_foreign_toplevel_image_capture_source_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, D>,
    ) {
        match request {
            ext_foreign_toplevel_image_capture_source_manager_v1::Request::CreateSource {
                source,
                toplevel_handle,
            } => {
                let kind = weak_handle_from_resource(&toplevel_handle)
                    .map(SourceKind::Toplevel)
                    .unwrap_or(SourceKind::Invalid);
                data_init.init(
                    source,
                    ImageCaptureSourceData {
                        source: ImageCaptureSource::new(kind),
                    },
                );
            }
            ext_foreign_toplevel_image_capture_source_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

delegate_global_dispatch!(
    RuntimeState,
    ExtOutputImageCaptureSourceManagerV1,
    CaptureSourceGlobalData
);
delegate_global_dispatch!(
    RuntimeState,
    ExtForeignToplevelImageCaptureSourceManagerV1,
    CaptureSourceGlobalData
);
delegate_dispatch!(
    RuntimeState,
    ExtOutputImageCaptureSourceManagerV1,
    CaptureSourceManagerData
);
delegate_dispatch!(
    RuntimeState,
    ExtForeignToplevelImageCaptureSourceManagerV1,
    CaptureSourceManagerData
);
delegate_dispatch!(
    RuntimeState,
    ExtImageCaptureSourceV1,
    ImageCaptureSourceData
);
