//! Tensorland color policy lowered into protocol-neutral renderer plans.

use tensor_protocol::{
    ChromaLocation, Chromaticities, ColorAlphaMode as ProtocolAlphaMode, ColorPrimaries,
    ColorRange as ProtocolRange, MatrixCoefficients, RenderIntent, SurfaceColorState,
    TransferFunction,
};
use thiserror::Error;
use vulkan_renderer::{
    ChromaOffset, ColorAlphaMode, ColorChromaticity, ColorPlanError,
    ColorPrimaries as RendererPrimaries, ColorTransferFunction, ColorTransformPlan, ColorVolume,
    PixelColorRange, PixelEncoding, SourceColorDescriptor, TargetColorDescriptor, TextureFormat,
    YcbcrMatrix,
};

pub(crate) fn sdr_output_target(format: TextureFormat) -> TargetColorDescriptor {
    TargetColorDescriptor {
        volume: ColorVolume::SDR_SRGB,
        format,
        hdr_metadata_supported: false,
    }
}

pub(crate) fn plan_surface_color(
    state: SurfaceColorState,
    target: TargetColorDescriptor,
) -> Result<ColorTransformPlan, SurfaceColorError> {
    let volume = match state.image_description {
        None => ColorVolume::SDR_SRGB,
        Some((description, intent)) => {
            if intent != RenderIntent::Perceptual {
                return Err(SurfaceColorError::UnsupportedRenderIntent(intent));
            }
            ColorVolume {
                primaries: renderer_primaries(description.primaries)?,
                transfer_function: renderer_transfer(description.transfer_function)?,
                min_luminance_x10k: description.luminances.min_luminance_x10k,
                max_luminance: description.luminances.max_luminance,
                reference_white: description.luminances.reference_white,
            }
        }
    };
    let representation = state.representation;
    let encoding = match representation.coefficients_and_range {
        None => PixelEncoding::default(),
        Some((MatrixCoefficients::Identity, range)) => PixelEncoding::Rgb {
            range: renderer_range(range),
        },
        Some((coefficients, range)) => {
            let (x_chroma_offset, y_chroma_offset) = representation
                .chroma_location
                .map(chroma_offsets)
                .unwrap_or((ChromaOffset::CositedEven, ChromaOffset::Midpoint));
            PixelEncoding::Ycbcr {
                matrix: ycbcr_matrix(coefficients)?,
                range: renderer_range(range),
                x_chroma_offset,
                y_chroma_offset,
            }
        }
    };
    let source = SourceColorDescriptor {
        volume,
        encoding,
        alpha_mode: match representation.alpha_mode {
            ProtocolAlphaMode::PremultipliedElectrical => ColorAlphaMode::PremultipliedElectrical,
            ProtocolAlphaMode::PremultipliedOptical => ColorAlphaMode::PremultipliedOptical,
            ProtocolAlphaMode::Straight => ColorAlphaMode::Straight,
        },
    };
    ColorTransformPlan::build(source, target).map_err(SurfaceColorError::Renderer)
}

fn renderer_primaries(value: ColorPrimaries) -> Result<RendererPrimaries, SurfaceColorError> {
    match value {
        ColorPrimaries::Srgb => Ok(RendererPrimaries::SRGB),
        ColorPrimaries::Bt2020 => Ok(RendererPrimaries::BT2020),
        ColorPrimaries::Custom(value) => Ok(custom_primaries(value)),
        other => Err(SurfaceColorError::UnsupportedPrimaries(other)),
    }
}

fn custom_primaries(value: Chromaticities) -> RendererPrimaries {
    let point = |value: tensor_protocol::Chromaticity| ColorChromaticity {
        x_millionths: value.x,
        y_millionths: value.y,
    };
    RendererPrimaries {
        red: point(value.red),
        green: point(value.green),
        blue: point(value.blue),
        white: point(value.white),
    }
}

fn renderer_transfer(value: TransferFunction) -> Result<ColorTransferFunction, SurfaceColorError> {
    match value {
        TransferFunction::Bt1886 => Ok(ColorTransferFunction::Bt1886),
        TransferFunction::Gamma22 => Ok(ColorTransferFunction::Gamma22),
        TransferFunction::Gamma28 => Ok(ColorTransferFunction::Gamma28),
        TransferFunction::ExtendedLinear => Ok(ColorTransferFunction::Linear),
        TransferFunction::Srgb | TransferFunction::CompoundPower24 => {
            Ok(ColorTransferFunction::Srgb)
        }
        TransferFunction::St2084Pq => Ok(ColorTransferFunction::St2084Pq),
        TransferFunction::Hlg => Ok(ColorTransferFunction::Hlg),
        TransferFunction::Power(exponent_x10k) => {
            Ok(ColorTransferFunction::Power { exponent_x10k })
        }
        other => Err(SurfaceColorError::UnsupportedTransferFunction(other)),
    }
}

fn renderer_range(value: ProtocolRange) -> PixelColorRange {
    match value {
        ProtocolRange::Full => PixelColorRange::Full,
        ProtocolRange::Limited => PixelColorRange::Limited,
    }
}

fn ycbcr_matrix(value: MatrixCoefficients) -> Result<YcbcrMatrix, SurfaceColorError> {
    match value {
        MatrixCoefficients::Identity => Err(SurfaceColorError::IdentityYcbcrMatrix),
        MatrixCoefficients::Bt709 => Ok(YcbcrMatrix::Bt709),
        MatrixCoefficients::Fcc => Ok(YcbcrMatrix::Fcc),
        MatrixCoefficients::Bt601 => Ok(YcbcrMatrix::Bt601),
        MatrixCoefficients::Smpte240 => Ok(YcbcrMatrix::Smpte240),
        MatrixCoefficients::Bt2020 => Ok(YcbcrMatrix::Bt2020),
        MatrixCoefficients::Bt2020ConstantLuminance => Ok(YcbcrMatrix::Bt2020ConstantLuminance),
        MatrixCoefficients::Ictcp => Ok(YcbcrMatrix::Ictcp),
    }
}

fn chroma_offsets(value: ChromaLocation) -> (ChromaOffset, ChromaOffset) {
    match value {
        ChromaLocation::Type0 => (ChromaOffset::CositedEven, ChromaOffset::Midpoint),
        ChromaLocation::Type1 => (ChromaOffset::Midpoint, ChromaOffset::Midpoint),
        ChromaLocation::Type2 => (ChromaOffset::CositedEven, ChromaOffset::CositedEven),
        ChromaLocation::Type3 => (ChromaOffset::Midpoint, ChromaOffset::CositedEven),
        ChromaLocation::Type4 => (ChromaOffset::CositedEven, ChromaOffset::Bottom),
        ChromaLocation::Type5 => (ChromaOffset::Midpoint, ChromaOffset::Bottom),
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(crate) enum SurfaceColorError {
    #[error("render intent {0:?} is not supported by the output color path")]
    UnsupportedRenderIntent(RenderIntent),
    #[error("color primaries {0:?} are not supported by the renderer lowering")]
    UnsupportedPrimaries(ColorPrimaries),
    #[error("transfer function {0:?} is not supported by the renderer lowering")]
    UnsupportedTransferFunction(TransferFunction),
    #[error("identity coefficients cannot be lowered as YCbCr")]
    IdentityYcbcrMatrix,
    #[error("shared renderer rejected the color plan: {0:?}")]
    Renderer(ColorPlanError),
}

#[cfg(test)]
mod tests {
    use tensor_protocol::{
        ColorLuminances, ColorRepresentation, ImageDescription, ImageDescriptionId,
    };
    use vulkan_renderer::{GamutMap, ToneMap};

    use super::*;

    #[test]
    fn unset_surface_state_preserves_the_existing_sdr_identity_path() {
        let plan = plan_surface_color(
            SurfaceColorState::default(),
            sdr_output_target(TextureFormat::Bgra8Srgb),
        )
        .unwrap();
        assert!(plan.is_identity());
    }

    #[test]
    fn pq_bt2020_surface_to_sdr_output_selects_tone_and_gamut_mapping() {
        let description = ImageDescription {
            id: ImageDescriptionId::new(9).unwrap(),
            primaries: ColorPrimaries::Bt2020,
            transfer_function: TransferFunction::St2084Pq,
            luminances: ColorLuminances::PQ,
            mastering: None,
        };
        let state = SurfaceColorState {
            image_description: Some((description, RenderIntent::Perceptual)),
            representation: ColorRepresentation::default(),
        };
        let plan = plan_surface_color(state, sdr_output_target(TextureFormat::Bgra8Srgb)).unwrap();

        assert_eq!(plan.gamut_map, GamutMap::Perceptual);
        assert_eq!(
            plan.tone_map,
            ToneMap::Bt2390 {
                source_peak: 10_000,
                target_peak: 80,
            }
        );
        assert_eq!(plan.working_format, TextureFormat::Rgba16Float);
    }
}
