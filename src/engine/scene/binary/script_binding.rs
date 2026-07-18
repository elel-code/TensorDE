use super::*;

pub(super) fn encode_script_bindings(
    audio_bindings: &[SceneAudioBandMaterialBindingRecord],
    text_providers: &[SceneTextProviderRecord],
    script_programs: &[SceneScriptProgramRecord],
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
    put_u32(
        &mut out,
        checked_u32(script_programs.len(), "script program count")?,
    );
    for program in script_programs {
        put_u32(&mut out, program.object.0);
        put_u32(&mut out, program.target.to_u32());
        put_string_id(&mut out, program.source);
        put_string_id(&mut out, program.properties_json);
        put_string_id(&mut out, program.initial_text);
        put_u32(&mut out, program.subscriptions.0);
        for value in program.initial_numeric {
            put_f32(&mut out, value);
        }
    }
    Ok(out)
}

pub(super) fn decode_script_bindings(
    data: &[u8],
) -> Result<
    (
        Vec<SceneAudioBandMaterialBindingRecord>,
        Vec<SceneTextProviderRecord>,
        Vec<SceneScriptProgramRecord>,
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
    let script_count = decoder.u32()? as usize;
    let mut script_programs = Vec::with_capacity(script_count);
    for _ in 0..script_count {
        let object = SceneObjectHandle(decoder.u32()?);
        let target_raw = decoder.u32()?;
        let target = SceneScriptTarget::from_u32(target_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("script program target", target_raw),
        )?;
        script_programs.push(SceneScriptProgramRecord {
            object,
            target,
            source: decoder.string_id()?,
            properties_json: decoder.string_id()?,
            initial_text: decoder.string_id()?,
            subscriptions: SceneScriptSubscriptions(decoder.u32()?),
            initial_numeric: [
                decoder.f32()?,
                decoder.f32()?,
                decoder.f32()?,
                decoder.f32()?,
            ],
        });
    }
    Ok((bindings, text_providers, script_programs))
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
        let scripts = [SceneScriptProgramRecord {
            object: SceneObjectHandle(3),
            target: SceneScriptTarget::Origin,
            source: SceneStringId(8),
            properties_json: SceneStringId(9),
            initial_text: SceneStringId::NONE,
            subscriptions: SceneScriptSubscriptions::FRAME,
            initial_numeric: [1.0, 2.0, 3.0, 0.0],
        }];
        let encoded = encode_script_bindings(&[], &providers, &scripts).expect("encode");
        let (audio, decoded, scripts) = decode_script_bindings(&encoded).expect("decode");
        assert!(audio.is_empty());
        assert_eq!(decoded, providers);
        assert_eq!(scripts[0].source, SceneStringId(8));
        assert_eq!(scripts[0].target, SceneScriptTarget::Origin);
    }
}
