use super::*;

pub(super) fn encode_audio_band_material_bindings(
    bindings: &[SceneAudioBandMaterialBindingRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(
        &mut out,
        checked_u32(bindings.len(), "audio band material binding count")?,
    );
    for binding in bindings {
        put_u32(&mut out, binding.object.0);
        put_u32(&mut out, binding.target.to_u32());
        put_u32(&mut out, binding.spectrum_resolution);
        put_u32(&mut out, binding.band_index);
        put_f32(&mut out, binding.smoothing);
        put_f32(&mut out, binding.minimum_multiplier);
        put_f32(&mut out, binding.maximum_multiplier);
        put_f32(&mut out, binding.initial_value);
    }
    Ok(out)
}

pub(super) fn decode_audio_band_material_bindings(
    data: &[u8],
) -> Result<Vec<SceneAudioBandMaterialBindingRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut bindings = Vec::with_capacity(count);
    for _ in 0..count {
        let object = SceneObjectHandle(decoder.u32()?);
        let target_raw = decoder.u32()?;
        let target = SceneAudioBandMaterialTarget::from_u32(target_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("audio band material target", target_raw),
        )?;
        bindings.push(SceneAudioBandMaterialBindingRecord {
            object,
            target,
            spectrum_resolution: decoder.u32()?,
            band_index: decoder.u32()?,
            smoothing: decoder.f32()?,
            minimum_multiplier: decoder.f32()?,
            maximum_multiplier: decoder.f32()?,
            initial_value: decoder.f32()?,
        });
    }
    Ok(bindings)
}
