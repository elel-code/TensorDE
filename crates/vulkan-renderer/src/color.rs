//! Protocol-neutral color transformation planning.
//!
//! The planner is deliberately value-only and allocation-free. Applications
//! lower their protocol or media metadata into these descriptors on a cold or
//! commit path, then cache the resulting plan for frame recording.

use crate::TextureFormat;

mod shader;
pub use shader::{ColorShaderEncoding, ColorTransformShaderData};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Chromaticity {
    pub x_millionths: i32,
    pub y_millionths: i32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ColorPrimaries {
    pub red: Chromaticity,
    pub green: Chromaticity,
    pub blue: Chromaticity,
    pub white: Chromaticity,
}

impl ColorPrimaries {
    pub const SRGB: Self = Self {
        red: Chromaticity {
            x_millionths: 640_000,
            y_millionths: 330_000,
        },
        green: Chromaticity {
            x_millionths: 300_000,
            y_millionths: 600_000,
        },
        blue: Chromaticity {
            x_millionths: 150_000,
            y_millionths: 60_000,
        },
        white: Chromaticity {
            x_millionths: 312_700,
            y_millionths: 329_000,
        },
    };
    pub const BT2020: Self = Self {
        red: Chromaticity {
            x_millionths: 708_000,
            y_millionths: 292_000,
        },
        green: Chromaticity {
            x_millionths: 170_000,
            y_millionths: 797_000,
        },
        blue: Chromaticity {
            x_millionths: 131_000,
            y_millionths: 46_000,
        },
        white: Chromaticity {
            x_millionths: 312_700,
            y_millionths: 329_000,
        },
    };
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ColorTransferFunction {
    Linear,
    Srgb,
    Bt1886,
    Gamma22,
    Gamma28,
    St2084Pq,
    Hlg,
    Power { exponent_x10k: u32 },
}

impl ColorTransferFunction {
    pub const fn is_hdr(self) -> bool {
        matches!(self, Self::St2084Pq | Self::Hlg)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ColorVolume {
    pub primaries: ColorPrimaries,
    pub transfer_function: ColorTransferFunction,
    pub min_luminance_x10k: u32,
    pub max_luminance: u32,
    pub reference_white: u32,
}

impl ColorVolume {
    pub const SDR_SRGB: Self = Self {
        primaries: ColorPrimaries::SRGB,
        transfer_function: ColorTransferFunction::Srgb,
        min_luminance_x10k: 2_000,
        max_luminance: 80,
        reference_white: 80,
    };

    pub const fn is_hdr(self) -> bool {
        self.transfer_function.is_hdr() || self.max_luminance > 203
    }

    fn valid(self) -> bool {
        self.max_luminance > 0
            && self.reference_white > 0
            && u64::from(self.min_luminance_x10k) < u64::from(self.max_luminance) * 10_000
            && u64::from(self.min_luminance_x10k) < u64::from(self.reference_white) * 10_000
            && match self.transfer_function {
                ColorTransferFunction::Power { exponent_x10k } => {
                    (10_000..=100_000).contains(&exponent_x10k)
                }
                _ => true,
            }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ColorAlphaMode {
    #[default]
    PremultipliedElectrical,
    PremultipliedOptical,
    Straight,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum YcbcrMatrix {
    Bt709,
    Fcc,
    Bt601,
    Smpte240,
    Bt2020,
    Bt2020ConstantLuminance,
    Ictcp,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ColorRange {
    Full,
    Limited,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChromaOffset {
    CositedEven,
    Midpoint,
    Bottom,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PixelEncoding {
    Rgb {
        range: ColorRange,
    },
    Ycbcr {
        matrix: YcbcrMatrix,
        range: ColorRange,
        x_chroma_offset: ChromaOffset,
        y_chroma_offset: ChromaOffset,
    },
}

impl Default for PixelEncoding {
    fn default() -> Self {
        Self::Rgb {
            range: ColorRange::Full,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SourceColorDescriptor {
    pub volume: ColorVolume,
    pub encoding: PixelEncoding,
    pub alpha_mode: ColorAlphaMode,
}

impl Default for SourceColorDescriptor {
    fn default() -> Self {
        Self {
            volume: ColorVolume::SDR_SRGB,
            encoding: PixelEncoding::default(),
            alpha_mode: ColorAlphaMode::PremultipliedElectrical,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TargetColorDescriptor {
    pub volume: ColorVolume,
    pub format: TextureFormat,
    /// Whether the native presentation owner can publish/reset HDR metadata.
    pub hdr_metadata_supported: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GamutMap {
    None,
    Perceptual,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ToneMap {
    None,
    Bt2390 { source_peak: u32, target_peak: u32 },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ColorTransformPlan {
    pub source: SourceColorDescriptor,
    pub target: TargetColorDescriptor,
    pub decode_to_linear: bool,
    pub gamut_map: GamutMap,
    pub tone_map: ToneMap,
    pub encode_from_linear: bool,
    pub working_format: TextureFormat,
    pub publish_hdr_metadata: bool,
    gamut_matrix_millionths: [i32; 9],
}

impl ColorTransformPlan {
    pub fn build(
        source: SourceColorDescriptor,
        target: TargetColorDescriptor,
    ) -> Result<Self, ColorPlanError> {
        if !source.volume.valid() {
            return Err(ColorPlanError::InvalidSourceVolume);
        }
        if !target.volume.valid() {
            return Err(ColorPlanError::InvalidTargetVolume);
        }
        let gamut_matrix_millionths =
            gamut_matrix_millionths(source.volume.primaries, target.volume.primaries)?;
        if target.volume.is_hdr() && !hdr_target_format(target.format) {
            return Err(ColorPlanError::TargetFormatCannotRepresentHdr(
                target.format,
            ));
        }
        if target.volume.is_hdr() && !target.hdr_metadata_supported {
            return Err(ColorPlanError::MissingHdrMetadataPath);
        }

        let gamut_map = if source.volume.primaries == target.volume.primaries {
            GamutMap::None
        } else {
            GamutMap::Perceptual
        };
        let tone_map = if source.volume.max_luminance > target.volume.max_luminance {
            ToneMap::Bt2390 {
                source_peak: source.volume.max_luminance,
                target_peak: target.volume.max_luminance,
            }
        } else {
            ToneMap::None
        };
        let transfer_changes = source.volume.transfer_function != target.volume.transfer_function;
        let representation_changes = source.encoding != PixelEncoding::default()
            || source.alpha_mode != ColorAlphaMode::PremultipliedElectrical;
        let needs_linear = transfer_changes
            || gamut_map != GamutMap::None
            || tone_map != ToneMap::None
            || representation_changes;

        Ok(Self {
            source,
            target,
            decode_to_linear: needs_linear,
            gamut_map,
            tone_map,
            encode_from_linear: needs_linear,
            working_format: if needs_linear {
                TextureFormat::Rgba16Float
            } else {
                target.format
            },
            publish_hdr_metadata: target.volume.is_hdr(),
            gamut_matrix_millionths,
        })
    }

    pub fn is_identity(self) -> bool {
        !self.decode_to_linear
            && self.gamut_map == GamutMap::None
            && self.tone_map == ToneMap::None
            && !self.encode_from_linear
    }

    /// Linear-light source-RGB to target-RGB matrix selected on the cold
    /// planning path. The fixed-point storage keeps the retained plan fully
    /// comparable while frame lowering only performs nine integer-to-float
    /// conversions.
    pub fn gamut_matrix(self) -> [[f32; 3]; 3] {
        let value = |index| self.gamut_matrix_millionths[index] as f32 / 1_000_000.0;
        [
            [value(0), value(1), value(2)],
            [value(3), value(4), value(5)],
            [value(6), value(7), value(8)],
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorPlanError {
    InvalidSourceVolume,
    InvalidTargetVolume,
    TargetFormatCannotRepresentHdr(TextureFormat),
    MissingHdrMetadataPath,
    InvalidSourcePrimaries,
    InvalidTargetPrimaries,
}

fn gamut_matrix_millionths(
    source: ColorPrimaries,
    target: ColorPrimaries,
) -> Result<[i32; 9], ColorPlanError> {
    let source_to_xyz = rgb_to_xyz(source).ok_or(ColorPlanError::InvalidSourcePrimaries)?;
    let target_to_xyz = rgb_to_xyz(target).ok_or(ColorPlanError::InvalidTargetPrimaries)?;
    if source == target {
        return Ok([1_000_000, 0, 0, 0, 1_000_000, 0, 0, 0, 1_000_000]);
    }
    let xyz_to_target = inverse(target_to_xyz).ok_or(ColorPlanError::InvalidTargetPrimaries)?;
    let matrix = multiply(xyz_to_target, source_to_xyz);
    let mut fixed = [0_i32; 9];
    for (destination, value) in fixed.iter_mut().zip(matrix.into_iter().flatten()) {
        let scaled = (value * 1_000_000.0).round();
        if !scaled.is_finite() || scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
            return Err(ColorPlanError::InvalidSourcePrimaries);
        }
        *destination = scaled as i32;
    }
    Ok(fixed)
}

fn rgb_to_xyz(primaries: ColorPrimaries) -> Option<[[f64; 3]; 3]> {
    let red = xy_to_xyz(primaries.red)?;
    let green = xy_to_xyz(primaries.green)?;
    let blue = xy_to_xyz(primaries.blue)?;
    let white = xy_to_xyz(primaries.white)?;
    let unscaled = [
        [red[0], green[0], blue[0]],
        [red[1], green[1], blue[1]],
        [red[2], green[2], blue[2]],
    ];
    let scale = multiply_vector(inverse(unscaled)?, white);
    Some([
        [red[0] * scale[0], green[0] * scale[1], blue[0] * scale[2]],
        [red[1] * scale[0], green[1] * scale[1], blue[1] * scale[2]],
        [red[2] * scale[0], green[2] * scale[1], blue[2] * scale[2]],
    ])
}

fn xy_to_xyz(point: Chromaticity) -> Option<[f64; 3]> {
    let x = f64::from(point.x_millionths) / 1_000_000.0;
    let y = f64::from(point.y_millionths) / 1_000_000.0;
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) || y == 0.0 || x + y > 1.0 {
        return None;
    }
    Some([x / y, 1.0, (1.0 - x - y) / y])
}

fn multiply(left: [[f64; 3]; 3], right: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut result = [[0.0; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            result[row][column] = (0..3)
                .map(|inner| left[row][inner] * right[inner][column])
                .sum();
        }
    }
    result
}

fn multiply_vector(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

fn inverse(matrix: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let determinant = matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]);
    if !determinant.is_finite() || determinant.abs() < 1.0e-12 {
        return None;
    }
    let inverse = 1.0 / determinant;
    Some([
        [
            (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1]) * inverse,
            (matrix[0][2] * matrix[2][1] - matrix[0][1] * matrix[2][2]) * inverse,
            (matrix[0][1] * matrix[1][2] - matrix[0][2] * matrix[1][1]) * inverse,
        ],
        [
            (matrix[1][2] * matrix[2][0] - matrix[1][0] * matrix[2][2]) * inverse,
            (matrix[0][0] * matrix[2][2] - matrix[0][2] * matrix[2][0]) * inverse,
            (matrix[0][2] * matrix[1][0] - matrix[0][0] * matrix[1][2]) * inverse,
        ],
        [
            (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]) * inverse,
            (matrix[0][1] * matrix[2][0] - matrix[0][0] * matrix[2][1]) * inverse,
            (matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0]) * inverse,
        ],
    ])
}

const fn hdr_target_format(format: TextureFormat) -> bool {
    matches!(
        format,
        TextureFormat::A2R10G10B10UnormPack32
            | TextureFormat::A2B10G10R10UnormPack32
            | TextureFormat::Rgba16Float
            | TextureFormat::Rgba32Float
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr_target(format: TextureFormat, metadata: bool) -> TargetColorDescriptor {
        TargetColorDescriptor {
            volume: ColorVolume {
                primaries: ColorPrimaries::BT2020,
                transfer_function: ColorTransferFunction::St2084Pq,
                min_luminance_x10k: 50,
                max_luminance: 1_000,
                reference_white: 203,
            },
            format,
            hdr_metadata_supported: metadata,
        }
    }

    #[test]
    fn matching_sdr_is_a_direct_identity_plan() {
        let source = SourceColorDescriptor::default();
        let target = TargetColorDescriptor {
            volume: ColorVolume::SDR_SRGB,
            format: TextureFormat::Bgra8Srgb,
            hdr_metadata_supported: false,
        };
        let plan = ColorTransformPlan::build(source, target).unwrap();

        assert!(plan.is_identity());
        assert_eq!(plan.working_format, TextureFormat::Bgra8Srgb);
        assert!(!plan.publish_hdr_metadata);
    }

    #[test]
    fn hdr_to_hdr_uses_linear_gamut_mapping_without_tone_mapping() {
        let target = hdr_target(TextureFormat::A2R10G10B10UnormPack32, true);
        let source = SourceColorDescriptor {
            volume: target.volume,
            ..SourceColorDescriptor::default()
        };
        let plan = ColorTransformPlan::build(source, target).unwrap();

        assert!(plan.is_identity());
        assert!(plan.publish_hdr_metadata);
        assert_eq!(plan.working_format, target.format);
    }

    #[test]
    fn hdr_to_sdr_selects_linear_bt2390_tone_mapping() {
        let source = SourceColorDescriptor {
            volume: hdr_target(TextureFormat::Rgba16Float, true).volume,
            ..SourceColorDescriptor::default()
        };
        let target = TargetColorDescriptor {
            volume: ColorVolume::SDR_SRGB,
            format: TextureFormat::Bgra8Srgb,
            hdr_metadata_supported: false,
        };
        let plan = ColorTransformPlan::build(source, target).unwrap();

        assert_eq!(plan.working_format, TextureFormat::Rgba16Float);
        assert_eq!(plan.gamut_map, GamutMap::Perceptual);
        assert_eq!(
            plan.tone_map,
            ToneMap::Bt2390 {
                source_peak: 1_000,
                target_peak: 80,
            }
        );
        assert!(!plan.publish_hdr_metadata);
    }

    #[test]
    fn hdr_target_fails_closed_without_format_or_metadata_support() {
        assert_eq!(
            ColorTransformPlan::build(
                SourceColorDescriptor::default(),
                hdr_target(TextureFormat::Bgra8Srgb, true),
            ),
            Err(ColorPlanError::TargetFormatCannotRepresentHdr(
                TextureFormat::Bgra8Srgb
            ))
        );
        assert_eq!(
            ColorTransformPlan::build(
                SourceColorDescriptor::default(),
                hdr_target(TextureFormat::Rgba16Float, false),
            ),
            Err(ColorPlanError::MissingHdrMetadataPath)
        );
    }
}
