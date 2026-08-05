use tensor_protocol::{ImageDescriptionId, RenderIntent};
use wayland_protocols::wp::color_management::v1::server::{
    wp_color_management_output_v1::{self, WpColorManagementOutputV1},
    wp_color_management_surface_feedback_v1::{self, WpColorManagementSurfaceFeedbackV1},
    wp_color_management_surface_v1::{self, WpColorManagementSurfaceV1},
    wp_color_manager_v1,
    wp_image_description_v1::{Cause, WpImageDescriptionV1},
};
use wayland_server::{
    Client, DataInit, DisplayHandle, Resource, Weak, backend::ClientId,
    protocol::wl_surface::WlSurface,
};

use crate::protocol::{
    dispatch::{DispatchDelegate, delegate_dispatch},
    globals::{color_management::ImageDescriptionData, output::WeakOutput},
    state::{RuntimeState, apply_surface_image_description},
};

#[derive(Debug)]
pub(in crate::protocol) struct ColorManagementSurfaceData {
    pub(super) surface: Weak<WlSurface>,
}

#[derive(Debug)]
pub(in crate::protocol) struct SurfaceFeedbackData {
    pub(super) surface: Weak<WlSurface>,
}

#[derive(Debug)]
pub(in crate::protocol) struct ColorManagementOutputData {
    pub(super) output: Option<WeakOutput>,
}

impl DispatchDelegate<WpColorManagementSurfaceV1, RuntimeState> for ColorManagementSurfaceData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        resource: &WpColorManagementSurfaceV1,
        request: wp_color_management_surface_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_color_management_surface_v1::Request::Destroy => self.detach(state, resource),
            wp_color_management_surface_v1::Request::SetImageDescription {
                image_description,
                render_intent,
            } => {
                let Some(surface) = self.surface(resource) else {
                    return;
                };
                let Some(intent) = decode_intent(render_intent) else {
                    resource.post_error(
                        wp_color_management_surface_v1::Error::RenderIntent,
                        "the rendering intent was not advertised",
                    );
                    return;
                };
                let Some(description) = image_description
                    .data::<ImageDescriptionData>()
                    .and_then(|data| data.description())
                else {
                    resource.post_error(
                        wp_color_management_surface_v1::Error::ImageDescription,
                        "the image description is not ready",
                    );
                    return;
                };
                state
                    .protocol_globals
                    .color_management
                    .set_pending(&surface, Some((description, intent)));
            }
            wp_color_management_surface_v1::Request::UnsetImageDescription => {
                let Some(surface) = self.surface(resource) else {
                    return;
                };
                state
                    .protocol_globals
                    .color_management
                    .set_pending(&surface, None);
            }
            _ => unreachable!(),
        }
    }

    fn destroyed(
        &self,
        state: &mut RuntimeState,
        _client: ClientId,
        resource: &WpColorManagementSurfaceV1,
    ) {
        self.detach(state, resource);
    }
}

impl ColorManagementSurfaceData {
    fn surface(&self, resource: &WpColorManagementSurfaceV1) -> Option<WlSurface> {
        match self.surface.upgrade() {
            Ok(surface) => Some(surface),
            Err(_) => {
                resource.post_error(
                    wp_color_management_surface_v1::Error::Inert,
                    "the associated wl_surface was destroyed",
                );
                None
            }
        }
    }

    fn detach(&self, state: &RuntimeState, resource: &WpColorManagementSurfaceV1) {
        let Ok(surface) = self.surface.upgrade() else {
            return;
        };
        state
            .protocol_globals
            .color_management
            .detach_surface(&surface, resource);
    }
}

impl DispatchDelegate<WpColorManagementSurfaceFeedbackV1, RuntimeState> for SurfaceFeedbackData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        feedback: &WpColorManagementSurfaceFeedbackV1,
        request: wp_color_management_surface_feedback_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_color_management_surface_feedback_v1::Request::Destroy => {}
            wp_color_management_surface_feedback_v1::Request::GetPreferred {
                image_description,
            }
            | wp_color_management_surface_feedback_v1::Request::GetPreferredParametric {
                image_description,
            } => {
                if self.surface.upgrade().is_err() {
                    feedback.post_error(
                        wp_color_management_surface_feedback_v1::Error::Inert,
                        "the associated wl_surface was destroyed",
                    );
                    return;
                }
                super::image::init_for_version(
                    data_init,
                    image_description,
                    state
                        .protocol_globals
                        .color_management
                        .default_description(),
                    true,
                    feedback.version(),
                );
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpColorManagementOutputV1, RuntimeState> for ColorManagementOutputData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        output: &WpColorManagementOutputV1,
        request: wp_color_management_output_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_color_management_output_v1::Request::Destroy => {}
            wp_color_management_output_v1::Request::GetImageDescription { image_description } => {
                if self.output.as_ref().and_then(WeakOutput::upgrade).is_some() {
                    super::image::init_for_version(
                        data_init,
                        image_description,
                        state
                            .protocol_globals
                            .color_management
                            .default_description(),
                        true,
                        output.version(),
                    );
                } else {
                    let image: WpImageDescriptionV1 =
                        data_init.init(image_description, ImageDescriptionData::failed());
                    image.failed(
                        Cause::NoOutput,
                        "the wl_output is no longer live".to_owned(),
                    );
                }
            }
            _ => unreachable!(),
        }
    }
}

pub(super) fn send_preferred_changed(
    feedback: &WpColorManagementSurfaceFeedbackV1,
    identity: ImageDescriptionId,
) {
    if feedback.version() >= 2 {
        feedback.preferred_changed2((identity.get() >> 32) as u32, identity.get() as u32);
    } else {
        feedback.preferred_changed(u32::try_from(identity.get()).unwrap_or(u32::MAX).max(1));
    }
}

pub(super) fn description_post_commit(
    state: &mut RuntimeState,
    _display: &DisplayHandle,
    surface: &WlSurface,
) {
    let Some(description) = state
        .protocol_globals
        .color_management
        .commit_surface(surface)
    else {
        return;
    };
    apply_surface_image_description(surface, description);
}

fn decode_intent(
    value: wayland_server::WEnum<wp_color_manager_v1::RenderIntent>,
) -> Option<RenderIntent> {
    match value {
        wayland_server::WEnum::Value(wp_color_manager_v1::RenderIntent::Perceptual) => {
            Some(RenderIntent::Perceptual)
        }
        wayland_server::WEnum::Value(_) | wayland_server::WEnum::Unknown(_) => None,
    }
}

delegate_dispatch!(
    RuntimeState,
    WpColorManagementSurfaceFeedbackV1,
    SurfaceFeedbackData
);
