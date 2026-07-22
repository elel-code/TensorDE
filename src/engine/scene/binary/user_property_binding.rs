use super::*;

pub(super) fn encode_user_property_bindings(
    bindings: &[SceneUserPropertyBindingRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_u32(
        &mut out,
        checked_u32(bindings.len(), "user property binding count")?,
    );
    for binding in bindings {
        put_u32(&mut out, binding.object.0);
        put_string_id(&mut out, binding.property);
        put_u32(&mut out, binding.target.to_u32());
        match binding.predicate {
            SceneUserPropertyPredicate::BooleanEquals(value) => {
                put_u32(&mut out, 1);
                put_u32(&mut out, u32::from(value));
            }
            SceneUserPropertyPredicate::StringEquals(value) => {
                put_u32(&mut out, 2);
                put_string_id(&mut out, value);
            }
        }
    }
    Ok(out)
}

pub(super) fn decode_user_property_bindings(
    data: &[u8],
) -> Result<Vec<SceneUserPropertyBindingRecord>, SceneBinaryError> {
    let mut decoder = Decoder::new(data);
    let count = decoder.u32()? as usize;
    let mut bindings = Vec::with_capacity(count);
    for _ in 0..count {
        let object = SceneObjectHandle(decoder.u32()?);
        let property = decoder.string_id()?;
        let target_raw = decoder.u32()?;
        let target = SceneUserPropertyTarget::from_u32(target_raw).ok_or(
            SceneBinaryError::InvalidChunkValue("user property target", target_raw),
        )?;
        let predicate_kind = decoder.u32()?;
        let predicate_value = decoder.u32()?;
        let predicate = match predicate_kind {
            1 => match predicate_value {
                0 => SceneUserPropertyPredicate::BooleanEquals(false),
                1 => SceneUserPropertyPredicate::BooleanEquals(true),
                value => {
                    return Err(SceneBinaryError::InvalidChunkValue(
                        "user property boolean predicate",
                        value,
                    ));
                }
            },
            2 => SceneUserPropertyPredicate::StringEquals(SceneStringId(predicate_value)),
            value => {
                return Err(SceneBinaryError::InvalidChunkValue(
                    "user property predicate",
                    value,
                ));
            }
        };
        bindings.push(SceneUserPropertyBindingRecord {
            object,
            property,
            target,
            predicate,
        });
    }
    Ok(bindings)
}
