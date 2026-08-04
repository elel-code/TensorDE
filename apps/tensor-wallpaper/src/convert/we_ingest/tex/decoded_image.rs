//! Embedded image level conversion into WE runtime upload channel layouts.

use image::GenericImageView;

use super::{DecodedLevel, TexParseError};

pub(super) fn decode_image_level(
    runtime_format: u32,
    payload: &[u8],
) -> Result<DecodedLevel, TexParseError> {
    let image =
        image::load_from_memory(payload).map_err(|err| TexParseError::Image(err.to_string()))?;
    let (width, height) = image.dimensions();
    let bytes = match runtime_format {
        0 | 1 | 2 | 3 | 5 => image.to_rgba8().into_raw(),
        8 => image.to_luma_alpha8().into_raw(),
        9 => image.to_luma8().into_raw(),
        4 | 6 | 7 | 12 => {
            return Err(TexParseError::Image(format!(
                "embedded image cannot be lowered directly to block-compressed runtime format {runtime_format}"
            )));
        }
        value => return Err(TexParseError::UnsupportedRuntimeFormat(value)),
    };
    Ok(DecodedLevel {
        width,
        height,
        bytes,
    })
}
