use tensor_protocol::{Chromaticities, ColorPrimaries, ImageDescription, TransferFunction};
use wayland_protocols::wp::color_management::v1::server::{
    wp_color_manager_v1,
    wp_image_description_info_v1::{self, WpImageDescriptionInfoV1},
    wp_image_description_reference_v1::{self, WpImageDescriptionReferenceV1},
    wp_image_description_v1::{self, WpImageDescriptionV1},
};
use wayland_server::{Client, DataInit, DisplayHandle, New, Resource};

use crate::protocol::{
    dispatch::{DispatchDelegate, delegate_dispatch},
    state::RuntimeState,
};

#[derive(Clone, Copy, Debug)]
enum ImageDescriptionState {
    Ready(ImageDescription),
    Failed,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::protocol) struct ImageDescriptionData {
    state: ImageDescriptionState,
    information_allowed: bool,
}

impl ImageDescriptionData {
    pub(super) const fn ready(description: ImageDescription, information_allowed: bool) -> Self {
        Self {
            state: ImageDescriptionState::Ready(description),
            information_allowed,
        }
    }

    pub(super) const fn failed() -> Self {
        Self {
            state: ImageDescriptionState::Failed,
            information_allowed: false,
        }
    }

    pub(super) const fn description(self) -> Option<ImageDescription> {
        match self.state {
            ImageDescriptionState::Ready(description) => Some(description),
            ImageDescriptionState::Failed => None,
        }
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct ImageDescriptionInfoData;

#[derive(Clone, Copy, Debug)]
pub(in crate::protocol) struct ImageDescriptionReferenceData {
    pub(super) description: ImageDescription,
    pub(super) information_allowed: bool,
}

pub(super) fn init_for_version(
    data_init: &mut DataInit<'_, RuntimeState>,
    new: New<WpImageDescriptionV1>,
    description: ImageDescription,
    information_allowed: bool,
    creating_version: u32,
) -> WpImageDescriptionV1 {
    if minimum_version(description) > creating_version {
        let image = data_init.init(new, ImageDescriptionData::failed());
        image.failed(
            wp_image_description_v1::Cause::LowVersion,
            "the creating interface cannot represent this image description".to_owned(),
        );
        return image;
    }
    let image = data_init.init(
        new,
        ImageDescriptionData::ready(description, information_allowed),
    );
    send_ready(&image, description);
    image
}

fn minimum_version(description: ImageDescription) -> u32 {
    match description.transfer_function {
        TransferFunction::CompoundPower24 => 2,
        _ => 1,
    }
}

fn send_ready(image: &WpImageDescriptionV1, description: ImageDescription) {
    let identity = description.id.get();
    if image.version() >= 2 {
        image.ready2((identity >> 32) as u32, identity as u32);
    } else {
        let legacy = u32::try_from(identity).unwrap_or(u32::MAX).max(1);
        image.ready(legacy);
    }
}

impl DispatchDelegate<WpImageDescriptionV1, RuntimeState> for ImageDescriptionData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        image: &WpImageDescriptionV1,
        request: wp_image_description_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_image_description_v1::Request::Destroy => {}
            wp_image_description_v1::Request::GetInformation { information } => {
                let Some(description) = self.description() else {
                    image.post_error(
                        wp_image_description_v1::Error::NotReady,
                        "failed image descriptions can only be destroyed",
                    );
                    return;
                };
                if !self.information_allowed {
                    image.post_error(
                        wp_image_description_v1::Error::NoInformation,
                        "this image description does not expose information",
                    );
                    return;
                }
                let info: WpImageDescriptionInfoV1 =
                    data_init.init(information, ImageDescriptionInfoData);
                send_information(&info, description);
                state
                    .protocol_globals
                    .color_management
                    .defer_information_done(info);
            }
            _ => unreachable!(),
        }
    }
}

impl DispatchDelegate<WpImageDescriptionInfoV1, RuntimeState> for ImageDescriptionInfoData {
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &WpImageDescriptionInfoV1,
        _request: wp_image_description_info_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
    }
}

impl DispatchDelegate<WpImageDescriptionReferenceV1, RuntimeState>
    for ImageDescriptionReferenceData
{
    fn request(
        &self,
        _state: &mut RuntimeState,
        _client: &Client,
        _resource: &WpImageDescriptionReferenceV1,
        request: wp_image_description_reference_v1::Request,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        match request {
            wp_image_description_reference_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

fn send_information(info: &WpImageDescriptionInfoV1, description: ImageDescription) {
    let primaries = description
        .primaries
        .chromaticities()
        .expect("ready parametric descriptions retain explicit chromaticities");
    send_primaries(info, primaries);
    if let Some(named) = wire_primaries(description.primaries) {
        info.primaries_named(named);
    }
    match description.transfer_function {
        TransferFunction::Power(exponent) => info.tf_power(exponent),
        transfer => info.tf_named(
            wire_transfer(transfer)
                .expect("ready parametric descriptions retain an advertised transfer function"),
        ),
    }
    let luminances = description.luminances;
    info.luminances(
        luminances.min_luminance_x10k,
        luminances.max_luminance,
        luminances.reference_white,
    );
    let (target_primaries, target_min, target_max, max_cll, max_fall) =
        description.mastering.map_or(
            (
                primaries,
                luminances.min_luminance_x10k,
                luminances.max_luminance,
                None,
                None,
            ),
            |mastering| {
                (
                    mastering.primaries,
                    mastering.min_luminance_x10k,
                    mastering.max_luminance,
                    mastering.max_content_light_level,
                    mastering.max_frame_average_light_level,
                )
            },
        );
    send_target_primaries(info, target_primaries);
    info.target_luminance(target_min, target_max);
    if let Some(max_cll) = max_cll {
        info.target_max_cll(max_cll);
    }
    if let Some(max_fall) = max_fall {
        info.target_max_fall(max_fall);
    }
}

fn send_primaries(info: &WpImageDescriptionInfoV1, value: Chromaticities) {
    info.primaries(
        value.red.x,
        value.red.y,
        value.green.x,
        value.green.y,
        value.blue.x,
        value.blue.y,
        value.white.x,
        value.white.y,
    );
}

fn send_target_primaries(info: &WpImageDescriptionInfoV1, value: Chromaticities) {
    info.target_primaries(
        value.red.x,
        value.red.y,
        value.green.x,
        value.green.y,
        value.blue.x,
        value.blue.y,
        value.white.x,
        value.white.y,
    );
}

fn wire_primaries(value: ColorPrimaries) -> Option<wp_color_manager_v1::Primaries> {
    match value {
        ColorPrimaries::Srgb => Some(wp_color_manager_v1::Primaries::Srgb),
        ColorPrimaries::Bt2020 => Some(wp_color_manager_v1::Primaries::Bt2020),
        _ => None,
    }
}

fn wire_transfer(value: TransferFunction) -> Option<wp_color_manager_v1::TransferFunction> {
    use wp_color_manager_v1::TransferFunction as Wire;
    match value {
        TransferFunction::Bt1886 => Some(Wire::Bt1886),
        TransferFunction::Gamma22 => Some(Wire::Gamma22),
        TransferFunction::Gamma28 => Some(Wire::Gamma28),
        TransferFunction::ExtendedLinear => Some(Wire::ExtLinear),
        TransferFunction::St2084Pq => Some(Wire::St2084Pq),
        TransferFunction::Hlg => Some(Wire::Hlg),
        TransferFunction::CompoundPower24 => Some(Wire::CompoundPower24),
        _ => None,
    }
}

delegate_dispatch!(RuntimeState, WpImageDescriptionV1, ImageDescriptionData);
delegate_dispatch!(
    RuntimeState,
    WpImageDescriptionInfoV1,
    ImageDescriptionInfoData
);
delegate_dispatch!(
    RuntimeState,
    WpImageDescriptionReferenceV1,
    ImageDescriptionReferenceData
);
