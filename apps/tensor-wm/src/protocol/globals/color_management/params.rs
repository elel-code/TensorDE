use std::sync::Mutex;

use tensor_protocol::{
    Chromaticities, Chromaticity, ColorLuminances, ColorPrimaries, ImageDescription,
    MasteringMetadata, TransferFunction,
};
use wayland_protocols::wp::color_management::v1::server::{
    wp_image_description_creator_params_v1::{self, WpImageDescriptionCreatorParamsV1},
    wp_image_description_v1::{Cause, WpImageDescriptionV1},
};
use wayland_server::{Client, DataInit, DisplayHandle, Resource};

use crate::protocol::{
    dispatch::{DispatchDelegate, delegate_dispatch},
    globals::color_management::{ImageDescriptionData, manager},
    state::RuntimeState,
};

#[derive(Debug, Default)]
pub(in crate::protocol) struct ParametricCreatorData {
    builder: Mutex<ParametricBuilder>,
}

#[derive(Clone, Copy, Debug, Default)]
struct ParametricBuilder {
    transfer: Option<TransferFunction>,
    primaries: Option<ColorPrimaries>,
    luminances: Option<ColorLuminances>,
    mastering_primaries: Option<Chromaticities>,
    mastering_luminance: Option<(u32, u32)>,
    max_cll: Option<u32>,
    max_fall: Option<u32>,
}

impl DispatchDelegate<WpImageDescriptionCreatorParamsV1, RuntimeState> for ParametricCreatorData {
    fn request(
        &self,
        state: &mut RuntimeState,
        _client: &Client,
        creator: &WpImageDescriptionCreatorParamsV1,
        request: wp_image_description_creator_params_v1::Request,
        _display: &DisplayHandle,
        data_init: &mut DataInit<'_, RuntimeState>,
    ) {
        use wp_image_description_creator_params_v1::{Error, Request};
        match request {
            Request::Create { image_description } => {
                let builder = *self.builder.lock().unwrap();
                let Some((primaries, transfer)) = builder.primaries.zip(builder.transfer) else {
                    creator.post_error(
                        Error::IncompleteSet,
                        "transfer function and primaries must both be set",
                    );
                    return;
                };
                if builder
                    .max_cll
                    .zip(builder.max_fall)
                    .is_some_and(|(cll, fall)| fall > cll)
                {
                    creator.post_error(Error::InvalidLuminance, "max_fall exceeds max_cll");
                    return;
                }

                let id = state
                    .protocol_globals
                    .color_management
                    .allocate_description_id();
                let description = builder.build(id, primaries, transfer);
                match description.and_then(ImageDescription::validate) {
                    Ok(description)
                        if creator.version() == 1
                            && !v1_content_light_levels_valid(description) =>
                    {
                        creator.post_error(
                            Error::InvalidLuminance,
                            "version 1 content-light levels are outside the target range",
                        );
                    }
                    Ok(description) if builder.target_is_supported(description) => {
                        super::image::init_for_version(
                            data_init,
                            image_description,
                            description,
                            false,
                            creator.version(),
                        );
                    }
                    Ok(_) | Err(_) => {
                        let image: WpImageDescriptionV1 =
                            data_init.init(image_description, ImageDescriptionData::failed());
                        image.failed(
                            Cause::Unsupported,
                            "unsupported parametric color volume".to_owned(),
                        );
                    }
                }
            }
            Request::SetTfNamed { tf } => {
                let mut builder = self.builder.lock().unwrap();
                if builder.transfer.is_some() {
                    creator.post_error(Error::AlreadySet, "transfer function was already set");
                } else if let Some(transfer) = manager::decode_transfer(tf)
                    && (creator.version() >= 2 || transfer != TransferFunction::CompoundPower24)
                {
                    builder.transfer = Some(transfer);
                } else {
                    creator.post_error(Error::InvalidTf, "transfer function was not advertised");
                }
            }
            Request::SetTfPower { eexp } => {
                let mut builder = self.builder.lock().unwrap();
                if builder.transfer.is_some() {
                    creator.post_error(Error::AlreadySet, "transfer function was already set");
                } else if !(10_000..=100_000).contains(&eexp) {
                    creator.post_error(Error::InvalidTf, "power exponent is outside 1.0..=10.0");
                } else {
                    builder.transfer = Some(TransferFunction::Power(eexp));
                }
            }
            Request::SetPrimariesNamed { primaries } => {
                let mut builder = self.builder.lock().unwrap();
                if builder.primaries.is_some() {
                    creator.post_error(Error::AlreadySet, "primaries were already set");
                } else if let Some(primaries) = manager::decode_primaries(primaries) {
                    builder.primaries = Some(primaries);
                } else {
                    creator.post_error(
                        Error::InvalidPrimariesNamed,
                        "named primaries were not advertised",
                    );
                }
            }
            Request::SetPrimaries {
                r_x,
                r_y,
                g_x,
                g_y,
                b_x,
                b_y,
                w_x,
                w_y,
            } => {
                let mut builder = self.builder.lock().unwrap();
                if builder.primaries.is_some() {
                    creator.post_error(Error::AlreadySet, "primaries were already set");
                } else {
                    builder.primaries = Some(ColorPrimaries::Custom(chromaticities(
                        r_x, r_y, g_x, g_y, b_x, b_y, w_x, w_y,
                    )));
                }
            }
            Request::SetLuminances {
                min_lum,
                max_lum,
                reference_lum,
            } => {
                let mut builder = self.builder.lock().unwrap();
                if builder.luminances.is_some() {
                    creator.post_error(Error::AlreadySet, "luminances were already set");
                    return;
                }
                let pq = builder.transfer == Some(TransferFunction::St2084Pq);
                let luminances =
                    ColorLuminances::new(min_lum, if pq { 10_000 } else { max_lum }, reference_lum);
                if !luminances.valid() {
                    creator.post_error(
                        Error::InvalidLuminance,
                        "maximum and reference luminance must exceed minimum luminance",
                    );
                } else {
                    builder.luminances = Some(luminances);
                }
            }
            Request::SetMasteringDisplayPrimaries {
                r_x,
                r_y,
                g_x,
                g_y,
                b_x,
                b_y,
                w_x,
                w_y,
            } => {
                let mut builder = self.builder.lock().unwrap();
                if builder.mastering_primaries.is_some() {
                    creator.post_error(
                        Error::AlreadySet,
                        "mastering display primaries were already set",
                    );
                } else {
                    builder.mastering_primaries =
                        Some(chromaticities(r_x, r_y, g_x, g_y, b_x, b_y, w_x, w_y));
                }
            }
            Request::SetMasteringLuminance { min_lum, max_lum } => {
                let mut builder = self.builder.lock().unwrap();
                if builder.mastering_luminance.is_some() {
                    creator.post_error(Error::AlreadySet, "mastering luminance was already set");
                } else if u64::from(min_lum) >= u64::from(max_lum) * 10_000 {
                    creator.post_error(
                        Error::InvalidLuminance,
                        "mastering maximum luminance must exceed its minimum",
                    );
                } else {
                    builder.mastering_luminance = Some((min_lum, max_lum));
                }
            }
            Request::SetMaxCll { max_cll } => {
                let mut builder = self.builder.lock().unwrap();
                if builder.max_cll.is_some() {
                    creator.post_error(Error::AlreadySet, "max_cll was already set");
                } else {
                    builder.max_cll = Some(max_cll);
                }
            }
            Request::SetMaxFall { max_fall } => {
                let mut builder = self.builder.lock().unwrap();
                if builder.max_fall.is_some() {
                    creator.post_error(Error::AlreadySet, "max_fall was already set");
                } else {
                    builder.max_fall = Some(max_fall);
                }
            }
            _ => unreachable!(),
        }
    }
}

impl ParametricBuilder {
    fn build(
        self,
        id: tensor_protocol::ImageDescriptionId,
        primaries: ColorPrimaries,
        transfer: TransferFunction,
    ) -> Result<ImageDescription, tensor_protocol::ImageDescriptionError> {
        let mut luminances = self
            .luminances
            .unwrap_or_else(|| default_luminances(transfer));
        if transfer == TransferFunction::St2084Pq {
            // The wire max_lum argument is ignored for PQ. The value-only
            // model uses whole cd/m², so retain the normative 10k peak.
            luminances.max_luminance = 10_000;
        }
        let source_primaries = primaries.chromaticities();
        let mastering_needed = self.mastering_primaries.is_some()
            || self.mastering_luminance.is_some()
            || self.max_cll.is_some()
            || self.max_fall.is_some();
        let mastering = if mastering_needed {
            Some(MasteringMetadata {
                primaries: self
                    .mastering_primaries
                    .or(source_primaries)
                    .ok_or(tensor_protocol::ImageDescriptionError::InvalidPrimaries)?,
                min_luminance_x10k: self
                    .mastering_luminance
                    .map_or(luminances.min_luminance_x10k, |value| value.0),
                max_luminance: self
                    .mastering_luminance
                    .map_or(luminances.max_luminance, |value| value.1),
                max_content_light_level: self.max_cll,
                max_frame_average_light_level: self.max_fall,
            })
        } else {
            None
        };
        Ok(ImageDescription {
            id,
            primaries,
            transfer_function: transfer,
            luminances,
            mastering,
        })
    }

    fn target_is_supported(self, description: ImageDescription) -> bool {
        let Some(mastering) = description.mastering else {
            return true;
        };
        let Some(source) = description.primaries.chromaticities() else {
            return false;
        };
        mastering.primaries.is_physical()
            && [
                mastering.primaries.red,
                mastering.primaries.green,
                mastering.primaries.blue,
                mastering.primaries.white,
            ]
            .into_iter()
            .all(|point| triangle_contains(source, point))
            && mastering.min_luminance_x10k >= description.luminances.min_luminance_x10k
            && mastering.max_luminance <= description.luminances.max_luminance
    }
}

fn default_luminances(transfer: TransferFunction) -> ColorLuminances {
    match transfer {
        TransferFunction::Bt1886 => ColorLuminances::new(100, 100, 100),
        TransferFunction::St2084Pq => ColorLuminances::PQ,
        TransferFunction::Hlg => ColorLuminances::HLG,
        _ => ColorLuminances::SDR,
    }
}

fn v1_content_light_levels_valid(description: ImageDescription) -> bool {
    let Some(mastering) = description.mastering else {
        return true;
    };
    [
        mastering.max_content_light_level,
        mastering.max_frame_average_light_level,
    ]
    .into_iter()
    .flatten()
    .all(|level| {
        u64::from(level) * 10_000 > u64::from(mastering.min_luminance_x10k)
            && level <= mastering.max_luminance
    })
}

#[allow(clippy::too_many_arguments)]
fn chromaticities(
    r_x: i32,
    r_y: i32,
    g_x: i32,
    g_y: i32,
    b_x: i32,
    b_y: i32,
    w_x: i32,
    w_y: i32,
) -> Chromaticities {
    Chromaticities {
        red: Chromaticity::new(r_x, r_y),
        green: Chromaticity::new(g_x, g_y),
        blue: Chromaticity::new(b_x, b_y),
        white: Chromaticity::new(w_x, w_y),
    }
}

fn triangle_contains(triangle: Chromaticities, point: Chromaticity) -> bool {
    let cross = |a: Chromaticity, b: Chromaticity, p: Chromaticity| {
        i128::from(b.x - a.x) * i128::from(p.y - a.y)
            - i128::from(b.y - a.y) * i128::from(p.x - a.x)
    };
    let values = [
        cross(triangle.red, triangle.green, point),
        cross(triangle.green, triangle.blue, point),
        cross(triangle.blue, triangle.red, point),
    ];
    values.iter().all(|value| *value >= 0) || values.iter().all(|value| *value <= 0)
}

delegate_dispatch!(
    RuntimeState,
    WpImageDescriptionCreatorParamsV1,
    ParametricCreatorData
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_gamut_must_be_contained_without_extended_volume() {
        assert!(triangle_contains(
            Chromaticities::BT2020,
            Chromaticities::SRGB.red
        ));
        assert!(!triangle_contains(
            Chromaticities::SRGB,
            Chromaticities::BT2020.red
        ));
    }
}
