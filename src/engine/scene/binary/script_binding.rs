use super::*;

pub(super) fn encode_script_bindings(
    script_programs: &[SceneScriptProgramRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
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
) -> Result<Vec<SceneScriptProgramRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
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
    Ok(script_programs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_chunk_round_trips_rquickjs_programs_without_old_layout() {
        let scripts = [SceneScriptProgramRecord {
            object: SceneObjectHandle(3),
            target: SceneScriptTarget::Origin,
            source: SceneStringId(8),
            properties_json: SceneStringId(9),
            initial_text: SceneStringId::NONE,
            subscriptions: SceneScriptSubscriptions::FRAME,
            initial_numeric: [1.0, 2.0, 3.0, 0.0],
        }];
        let encoded = encode_script_bindings(&scripts).expect("encode");
        let scripts = decode_script_bindings(&encoded).expect("decode");
        assert_eq!(scripts[0].source, SceneStringId(8));
        assert_eq!(scripts[0].target, SceneScriptTarget::Origin);
    }
}
