use super::*;

pub(super) fn encode_script_bindings(
    audio_bindings: &[SceneAudioBandMaterialBindingRecord],
    text_providers: &[SceneTextProviderRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(
        &mut out,
        checked_u32(audio_bindings.len(), "audio band material binding count")?,
    );
    for binding in audio_bindings {
        put_u32(&mut out, binding.object.0);
        put_u32(&mut out, binding.target.to_u32());
        put_u32(&mut out, binding.spectrum_resolution);
        put_u32(&mut out, binding.band_index);
        put_f32(&mut out, binding.smoothing);
        put_f32(&mut out, binding.minimum_multiplier);
        put_f32(&mut out, binding.maximum_multiplier);
        put_f32(&mut out, binding.initial_value);
    }
    put_u32(
        &mut out,
        checked_u32(text_providers.len(), "text provider count")?,
    );
    for provider in text_providers {
        put_u32(&mut out, provider.object.0);
        put_u32(&mut out, provider.kind.to_u32());
        put_string_id(&mut out, provider.initial_text);
        put_string_id(&mut out, provider.source_data);
        put_u32(&mut out, provider.update_interval_seconds);
    }
    Ok(out)
}

pub(super) fn decode_script_bindings(
    data: &[u8],
) -> Result<
    (
        Vec<SceneAudioBandMaterialBindingRecord>,
        Vec<SceneTextProviderRecord>,
    ),
    SceneBinaryError,
> {
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
    let text_count = decoder.u32()? as usize;
    let mut text_providers = Vec::with_capacity(text_count);
    for _ in 0..text_count {
        let object = SceneObjectHandle(decoder.u32()?);
        let kind_raw = decoder.u32()?;
        let kind = SceneTextProviderKind::from_u32(kind_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("text provider kind", kind_raw),
        )?;
        text_providers.push(SceneTextProviderRecord {
            object,
            kind,
            initial_text: decoder.string_id()?,
            source_data: decoder.string_id()?,
            update_interval_seconds: decoder.u32()?,
        });
    }
    Ok((bindings, text_providers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_chunk_round_trips_typed_text_providers_without_old_layout() {
        let providers = [SceneTextProviderRecord {
            object: SceneObjectHandle(3),
            kind: SceneTextProviderKind::ChineseClock,
            initial_text: SceneStringId(7),
            source_data: SceneStringId::NONE,
            update_interval_seconds: 60,
        }];
        let encoded = encode_script_bindings(&[], &providers).expect("encode");
        let (audio, decoded) = decode_script_bindings(&encoded).expect("decode");
        assert!(audio.is_empty());
        assert_eq!(decoded, providers);
    }
}
