//! Binary encoding for authored pointer-driven scene bindings.

use super::*;

pub(super) fn encode_pointer_bindings(
    camera: SceneCameraParallaxRecord,
    object_depths: &[SceneObjectParallaxDepthRecord],
) -> Result<Vec<u8>, SceneBinaryError> {
    let mut out = Vec::new();
    put_bool(&mut out, camera.enabled);
    put_f32(&mut out, camera.amount);
    put_f32(&mut out, camera.delay);
    put_f32(&mut out, camera.mouse_influence);
    put_u32(
        &mut out,
        checked_u32(object_depths.len(), "object parallax depth count")?,
    );
    for record in object_depths {
        put_u32(&mut out, record.object.0);
        put_f32(&mut out, record.depth[0]);
        put_f32(&mut out, record.depth[1]);
    }
    Ok(out)
}

pub(super) fn decode_pointer_bindings(
    data: &[u8],
) -> Result<
    (
        SceneCameraParallaxRecord,
        Vec<SceneObjectParallaxDepthRecord>,
    ),
    SceneBinaryError,
> {
    let mut decoder = Decoder::new(data);
    let camera = SceneCameraParallaxRecord {
        enabled: decoder.bool()?,
        amount: decoder.f32()?,
        delay: decoder.f32()?,
        mouse_influence: decoder.f32()?,
    };
    let count = decoder.u32()? as usize;
    let mut object_depths = Vec::with_capacity(count);
    for _ in 0..count {
        object_depths.push(SceneObjectParallaxDepthRecord {
            object: SceneObjectHandle(decoder.u32()?),
            depth: [decoder.f32()?, decoder.f32()?],
        });
    }
    Ok((camera, object_depths))
}
