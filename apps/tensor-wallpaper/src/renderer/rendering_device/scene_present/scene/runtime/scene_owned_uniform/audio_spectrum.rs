use super::payload::write_strided_values;
use super::*;

pub(super) fn write_audio_spectrum(
    destination: &mut [u8],
    spectrum: &StereoSpectrum64,
    channel: SceneAudioSpectrumChannel,
    resolution: SceneAudioSpectrumResolution,
    array_stride: u32,
) -> Result<(), String> {
    use SceneAudioSpectrumChannel::{Left, Right};
    use SceneAudioSpectrumResolution::{Bands16, Bands32, Bands64};

    let channel64 = match channel {
        Left => &spectrum.left,
        Right => &spectrum.right,
    };
    match resolution {
        Bands64 => write_strided_values(destination, channel64, array_stride),
        Bands32 => {
            let channel32 = StereoSpectrum64::max_pool_32(channel64);
            write_strided_values(destination, &channel32, array_stride)
        }
        Bands16 => {
            let channel32 = StereoSpectrum64::max_pool_32(channel64);
            let channel16 = StereoSpectrum64::max_pool_16(&channel32);
            write_strided_values(destination, &channel16, array_stride)
        }
    }
}
