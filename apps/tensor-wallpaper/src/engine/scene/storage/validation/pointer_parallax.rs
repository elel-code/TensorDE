// Pointer-parallax scalar and per-object depth contract validation.

fn validate_pointer_parallax(document: &SceneBinaryDocument) -> Result<(), SceneStorageError> {
    let camera = document.camera_parallax;
    if ![camera.amount, camera.delay, camera.mouse_influence]
        .into_iter()
        .all(f32::is_finite)
        || camera.delay < 0.0
    {
        return Err(SceneStorageError::InvalidPointerParallaxBinding {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            reason: "non-finite scalar or negative delay",
        });
    }
    for binding in &document.object_parallax_depths {
        validate_range(
            "object_parallax_depth.object",
            binding.object.0,
            1,
            document.objects.len(),
        )?;
        if !binding.depth.into_iter().all(f32::is_finite) {
            return Err(SceneStorageError::InvalidPointerParallaxBinding {
                object: binding.object,
                reason: "non-finite depth",
            });
        }
    }
    if document
        .object_parallax_depths
        .windows(2)
        .any(|pair| pair[0].object.0 >= pair[1].object.0)
    {
        return Err(SceneStorageError::InvalidPointerParallaxBinding {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            reason: "records are not strictly ordered by object",
        });
    }
    Ok(())
}
