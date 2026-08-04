//! Value-only color and pixel-representation state.
//!
//! Wire adapters validate protocol enums and creator lifetimes before building
//! these immutable values. No Wayland object, file descriptor, ICC parser, or
//! renderer handle crosses this boundary.

use serde::{Deserialize, Serialize};

/// Permanent compositor identity for one immutable image description.
///
/// Zero is reserved by `wp_image_description_v1` and is therefore impossible
/// to construct through this type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ImageDescriptionId(u64);

impl ImageDescriptionId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Rendering intent attached with an image description.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum RenderIntent {
    #[default]
    Perceptual,
    Relative,
    Saturation,
    Absolute,
    RelativeBlackPointCompensation,
    AbsoluteNoAdaptation,
}

/// CIE 1931 xy coordinate multiplied by one million.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Chromaticity {
    pub x: i32,
    pub y: i32,
}

impl Chromaticity {
    pub const SCALE: i32 = 1_000_000;

    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Whether this is a finite point in the physical CIE xy triangle.
    pub const fn is_physical(self) -> bool {
        self.x >= 0
            && self.y > 0
            && self.x <= Self::SCALE
            && self.y <= Self::SCALE
            && self.x.saturating_add(self.y) <= Self::SCALE
    }
}

/// Explicit RGB primaries and white point.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Chromaticities {
    pub red: Chromaticity,
    pub green: Chromaticity,
    pub blue: Chromaticity,
    pub white: Chromaticity,
}

impl Chromaticities {
    pub const SRGB: Self = Self {
        red: Chromaticity::new(640_000, 330_000),
        green: Chromaticity::new(300_000, 600_000),
        blue: Chromaticity::new(150_000, 60_000),
        white: Chromaticity::new(312_700, 329_000),
    };
    pub const BT2020: Self = Self {
        red: Chromaticity::new(708_000, 292_000),
        green: Chromaticity::new(170_000, 797_000),
        blue: Chromaticity::new(131_000, 46_000),
        white: Chromaticity::new(312_700, 329_000),
    };

    pub const fn is_physical(self) -> bool {
        self.red.is_physical()
            && self.green.is_physical()
            && self.blue.is_physical()
            && self.white.is_physical()
    }
}

/// Named or explicitly parameterized primary color volume.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ColorPrimaries {
    Srgb,
    PalM,
    Pal,
    Ntsc,
    GenericFilm,
    Bt2020,
    Cie1931Xyz,
    DciP3,
    DisplayP3,
    AdobeRgb,
    Custom(Chromaticities),
}

impl ColorPrimaries {
    pub const fn chromaticities(self) -> Option<Chromaticities> {
        match self {
            Self::Srgb => Some(Chromaticities::SRGB),
            Self::Bt2020 => Some(Chromaticities::BT2020),
            Self::Custom(value) => Some(value),
            _ => None,
        }
    }
}

/// Electrical-to-optical transfer characteristic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum TransferFunction {
    Bt1886,
    Gamma22,
    Gamma28,
    St240,
    ExtendedLinear,
    Log100,
    Log316,
    XvYcc,
    Srgb,
    ExtendedSrgb,
    St2084Pq,
    St428,
    Hlg,
    CompoundPower24,
    /// Pure power exponent multiplied by 10,000.
    Power(u32),
}

impl TransferFunction {
    pub const fn is_hdr(self) -> bool {
        matches!(self, Self::St2084Pq | Self::Hlg)
    }

    pub fn valid(self) -> bool {
        match self {
            Self::Power(exponent) => (10_000..=100_000).contains(&exponent),
            _ => true,
        }
    }
}

/// Primary color volume and reference-white luminances.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ColorLuminances {
    /// Minimum luminance in cd/m² multiplied by 10,000.
    pub min_luminance_x10k: u32,
    /// Maximum luminance in cd/m².
    pub max_luminance: u32,
    /// Reference white luminance in cd/m².
    pub reference_white: u32,
}

impl ColorLuminances {
    pub const SDR: Self = Self::new(2_000, 80, 80);
    pub const PQ: Self = Self::new(50, 10_000, 203);
    pub const HLG: Self = Self::new(50, 1_000, 203);

    pub const fn new(min_luminance_x10k: u32, max_luminance: u32, reference_white: u32) -> Self {
        Self {
            min_luminance_x10k,
            max_luminance,
            reference_white,
        }
    }

    pub fn valid(self) -> bool {
        let max_x10k = u64::from(self.max_luminance) * 10_000;
        let reference_x10k = u64::from(self.reference_white) * 10_000;
        u64::from(self.min_luminance_x10k) < max_x10k
            && u64::from(self.min_luminance_x10k) < reference_x10k
    }
}

/// Optional mastering-display and content-light metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct MasteringMetadata {
    pub primaries: Chromaticities,
    pub min_luminance_x10k: u32,
    pub max_luminance: u32,
    pub max_content_light_level: Option<u32>,
    pub max_frame_average_light_level: Option<u32>,
}

impl MasteringMetadata {
    pub fn valid(self) -> bool {
        self.primaries.is_physical()
            && u64::from(self.min_luminance_x10k) < u64::from(self.max_luminance) * 10_000
            && match (
                self.max_content_light_level,
                self.max_frame_average_light_level,
            ) {
                (Some(content), Some(average)) => average <= content,
                _ => true,
            }
    }
}

/// Immutable parametric image description.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ImageDescription {
    pub id: ImageDescriptionId,
    pub primaries: ColorPrimaries,
    pub transfer_function: TransferFunction,
    pub luminances: ColorLuminances,
    pub mastering: Option<MasteringMetadata>,
}

impl ImageDescription {
    pub const fn srgb(id: ImageDescriptionId) -> Self {
        Self {
            id,
            primaries: ColorPrimaries::Srgb,
            transfer_function: TransferFunction::CompoundPower24,
            luminances: ColorLuminances::SDR,
            mastering: None,
        }
    }

    pub fn validate(self) -> Result<Self, ImageDescriptionError> {
        if !self.transfer_function.valid() {
            return Err(ImageDescriptionError::InvalidTransferFunction);
        }
        if let ColorPrimaries::Custom(primaries) = self.primaries
            && !primaries.is_physical()
        {
            return Err(ImageDescriptionError::InvalidPrimaries);
        }
        if !self.luminances.valid() {
            return Err(ImageDescriptionError::InvalidLuminances);
        }
        if let Some(mastering) = self.mastering
            && !mastering.valid()
        {
            return Err(ImageDescriptionError::InvalidMasteringMetadata);
        }
        Ok(self)
    }

    pub const fn is_hdr(self) -> bool {
        self.transfer_function.is_hdr() || self.luminances.max_luminance > 203
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageDescriptionError {
    InvalidTransferFunction,
    InvalidPrimaries,
    InvalidLuminances,
    InvalidMasteringMetadata,
}

/// Alpha interpretation from `wp_color_representation_v1`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ColorAlphaMode {
    #[default]
    PremultipliedElectrical,
    PremultipliedOptical,
    Straight,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum MatrixCoefficients {
    Identity,
    Bt709,
    Fcc,
    Bt601,
    Smpte240,
    Bt2020,
    Bt2020ConstantLuminance,
    Ictcp,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ColorRange {
    Full,
    Limited,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ChromaLocation {
    Type0,
    Type1,
    Type2,
    Type3,
    Type4,
    Type5,
}

/// Committed color representation for one surface buffer.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ColorRepresentation {
    pub alpha_mode: ColorAlphaMode,
    pub coefficients_and_range: Option<(MatrixCoefficients, ColorRange)>,
    pub chroma_location: Option<ChromaLocation>,
}

/// Complete committed color state for one renderable surface.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SurfaceColorState {
    pub image_description: Option<(ImageDescription, RenderIntent)>,
    pub representation: ColorRepresentation,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_description_identity_rejects_protocol_reserved_zero() {
        assert_eq!(ImageDescriptionId::new(0), None);
        assert_eq!(
            ImageDescriptionId::new(7).map(ImageDescriptionId::get),
            Some(7)
        );
    }

    #[test]
    fn parametric_validation_rejects_bad_power_and_hdr_metadata() {
        let id = ImageDescriptionId::new(1).unwrap();
        let bad_power = ImageDescription {
            transfer_function: TransferFunction::Power(9_999),
            ..ImageDescription::srgb(id)
        };
        assert_eq!(
            bad_power.validate(),
            Err(ImageDescriptionError::InvalidTransferFunction)
        );

        let bad_metadata = MasteringMetadata {
            primaries: Chromaticities::BT2020,
            min_luminance_x10k: 50,
            max_luminance: 1_000,
            max_content_light_level: Some(400),
            max_frame_average_light_level: Some(500),
        };
        let description = ImageDescription {
            mastering: Some(bad_metadata),
            ..ImageDescription::srgb(id)
        };
        assert_eq!(
            description.validate(),
            Err(ImageDescriptionError::InvalidMasteringMetadata)
        );
    }

    #[test]
    fn standard_sdr_and_pq_descriptions_validate() {
        let sdr = ImageDescription::srgb(ImageDescriptionId::new(1).unwrap());
        assert_eq!(sdr.validate(), Ok(sdr));

        let pq = ImageDescription {
            id: ImageDescriptionId::new(2).unwrap(),
            primaries: ColorPrimaries::Bt2020,
            transfer_function: TransferFunction::St2084Pq,
            luminances: ColorLuminances::PQ,
            mastering: None,
        };
        assert_eq!(pq.validate(), Ok(pq));
        assert!(pq.is_hdr());
    }
}
