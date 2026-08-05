use tensor_protocol::{ColorPrimaries, TransferFunction};
use wayland_protocols::wp::color_management::v1::server::{
    wp_color_management_output_v1::WpColorManagementOutputV1,
    wp_color_management_surface_feedback_v1::WpColorManagementSurfaceFeedbackV1,
    wp_color_management_surface_v1::WpColorManagementSurfaceV1,
    wp_color_manager_v1::{self, WpColorManagerV1},
    wp_image_description_creator_params_v1::WpImageDescriptionCreatorParamsV1,
    wp_image_description_v1::{Cause, WpImageDescriptionV1},
};
use wayland_server::{Client, DataInit, DisplayHandle, New, Resource};

use crate::protocol::{
    dispatch::{
        DispatchDelegate, GlobalDispatchDelegate, delegate_dispatch, delegate_global_dispatch,
    },
    globals::{
        color_management::{
            AttachResult, ColorManagementOutputData, ColorManagementSurfaceData,
            ImageDescriptionData, ImageDescriptionReferenceData, ParametricCreatorData,
            SurfaceFeedbackData,
        },
        compositor,
        output::Output,
    },
    state::RuntimeState,
};

#[derive(Debug)]
pub(in crate::protocol) struct ColorManagerGlobalData;

#[derive(Debug)]
pub(in crate::protocol) struct ColorManagerData;

impl GlobalDispatchDelegate<WpColorManagerV1, RuntimeState> for ColorManagerGlobalData {
    fn bind(
        &self,
        _state: &mut RuntimeState,
        _display: &DisplayHandle,
        _client: &Client,
        resource: New<WpColorManagerV1>,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        let manager = data_init.init(resource, ColorManagerData);
        manager.supported_intent(wp_color_manager_v1::RenderIntent::Perceptual);
        for feature in [
            wp_color_manager_v1::Feature::Parametric,
            wp_color_manager_v1::Feature::SetPrimaries,
            wp_color_manager_v1::Feature::SetTfPower,
            wp_color_manager_v1::Feature::SetLuminances,
            wp_color_manager_v1::Feature::SetMasteringDisplayPrimaries,
        ] {
            manager.supported_feature(feature);
        }
        for transfer in supported_transfer_functions(manager.version()) {
            manager.supported_tf_named(transfer);
        }
        for primaries in supported_primaries() {
            manager.supported_primaries_named(primaries);
        }
        manager.done();
    }
}

impl DispatchDelegate<WpColorManagerV1, RuntimeState> for ColorManagerData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        manager: &WpColorManagerV1,
        request: wp_color_manager_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_color_manager_v1::Request::Destroy => {}
            wp_color_manager_v1::Request::GetOutput { id, output } => {
                let output = Output::from_resource_including_inactive(&output)
                    .map(|output| output.downgrade());
                data_init.init(id, ColorManagementOutputData { output });
            }
            wp_color_manager_v1::Request::GetSurface { id, surface } => {
                let resource = data_init.init(
                    id,
                    ColorManagementSurfaceData {
                        surface: surface.downgrade(),
                    },
                );
                match state
                    .protocol_globals
                    .color_management
                    .attach_surface(&surface, &resource)
                {
                    AttachResult::AlreadyExists => manager.post_error(
                        wp_color_manager_v1::Error::SurfaceExists,
                        "the surface already has a color-management object",
                    ),
                    AttachResult::Attached { install_hooks } => {
                        if install_hooks {
                            compositor::add_post_commit_hook::<RuntimeState, _>(
                                &surface,
                                super::surface::description_post_commit,
                            );
                        }
                    }
                }
            }
            wp_color_manager_v1::Request::GetSurfaceFeedback { id, surface } => {
                let feedback: WpColorManagementSurfaceFeedbackV1 = data_init.init(
                    id,
                    SurfaceFeedbackData {
                        surface: surface.downgrade(),
                    },
                );
                super::surface::send_preferred_changed(
                    &feedback,
                    state
                        .protocol_globals
                        .color_management
                        .default_description()
                        .id,
                );
            }
            wp_color_manager_v1::Request::CreateParametricCreator { obj } => {
                let _: WpImageDescriptionCreatorParamsV1 =
                    data_init.init(obj, ParametricCreatorData::default());
            }
            wp_color_manager_v1::Request::CreateIccCreator { .. } => {
                unsupported(manager, "ICC image-description creators are not supported")
            }
            wp_color_manager_v1::Request::CreateWindowsScrgb { .. } => unsupported(
                manager,
                "Windows-scRGB image descriptions are not supported",
            ),
            wp_color_manager_v1::Request::CreateWindowsBt2100 { .. } => unsupported(
                manager,
                "Windows-BT.2100 image descriptions are not supported",
            ),
            wp_color_manager_v1::Request::GetImageDescription {
                image_description,
                reference,
            } => {
                let Some(reference) = reference.data::<ImageDescriptionReferenceData>() else {
                    let image: WpImageDescriptionV1 =
                        data_init.init(image_description, ImageDescriptionData::failed());
                    image.failed(
                        Cause::Unsupported,
                        "unknown image-description reference".to_owned(),
                    );
                    return;
                };
                super::image::init_for_version(
                    data_init,
                    image_description,
                    reference.description,
                    reference.information_allowed,
                    manager.version(),
                );
            }
            _ => unreachable!(),
        }
    }
}

fn unsupported(manager: &WpColorManagerV1, message: &'static str) {
    manager.post_error(wp_color_manager_v1::Error::UnsupportedFeature, message);
}

pub(super) fn supported_transfer_functions(
    version: u32,
) -> impl Iterator<Item = wp_color_manager_v1::TransferFunction> {
    [
        wp_color_manager_v1::TransferFunction::Bt1886,
        wp_color_manager_v1::TransferFunction::Gamma22,
        wp_color_manager_v1::TransferFunction::Gamma28,
        wp_color_manager_v1::TransferFunction::ExtLinear,
        wp_color_manager_v1::TransferFunction::St2084Pq,
        wp_color_manager_v1::TransferFunction::Hlg,
        wp_color_manager_v1::TransferFunction::CompoundPower24,
    ]
    .into_iter()
    .filter(move |transfer| {
        version >= 2 || *transfer != wp_color_manager_v1::TransferFunction::CompoundPower24
    })
}

pub(super) fn supported_primaries() -> impl Iterator<Item = wp_color_manager_v1::Primaries> {
    [
        wp_color_manager_v1::Primaries::Srgb,
        wp_color_manager_v1::Primaries::Bt2020,
    ]
    .into_iter()
}

pub(super) fn decode_transfer(
    value: wayland_server::WEnum<wp_color_manager_v1::TransferFunction>,
) -> Option<TransferFunction> {
    use wp_color_manager_v1::TransferFunction as Wire;
    match value {
        wayland_server::WEnum::Value(Wire::Bt1886) => Some(TransferFunction::Bt1886),
        wayland_server::WEnum::Value(Wire::Gamma22) => Some(TransferFunction::Gamma22),
        wayland_server::WEnum::Value(Wire::Gamma28) => Some(TransferFunction::Gamma28),
        wayland_server::WEnum::Value(Wire::ExtLinear) => Some(TransferFunction::ExtendedLinear),
        wayland_server::WEnum::Value(Wire::St2084Pq) => Some(TransferFunction::St2084Pq),
        wayland_server::WEnum::Value(Wire::Hlg) => Some(TransferFunction::Hlg),
        wayland_server::WEnum::Value(Wire::CompoundPower24) => {
            Some(TransferFunction::CompoundPower24)
        }
        wayland_server::WEnum::Value(_) | wayland_server::WEnum::Unknown(_) => None,
    }
}

pub(super) fn decode_primaries(
    value: wayland_server::WEnum<wp_color_manager_v1::Primaries>,
) -> Option<ColorPrimaries> {
    match value {
        wayland_server::WEnum::Value(wp_color_manager_v1::Primaries::Srgb) => {
            Some(ColorPrimaries::Srgb)
        }
        wayland_server::WEnum::Value(wp_color_manager_v1::Primaries::Bt2020) => {
            Some(ColorPrimaries::Bt2020)
        }
        wayland_server::WEnum::Value(_) | wayland_server::WEnum::Unknown(_) => None,
    }
}

delegate_global_dispatch!(RuntimeState, WpColorManagerV1, ColorManagerGlobalData);
delegate_dispatch!(RuntimeState, WpColorManagerV1, ColorManagerData);
delegate_dispatch!(
    RuntimeState,
    WpColorManagementOutputV1,
    ColorManagementOutputData
);
delegate_dispatch!(
    RuntimeState,
    WpColorManagementSurfaceV1,
    ColorManagementSurfaceData
);
