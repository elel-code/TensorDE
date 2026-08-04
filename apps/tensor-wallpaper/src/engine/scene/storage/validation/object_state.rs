use super::*;

pub(super) fn validate_object_state(
    document: &SceneBinaryDocument,
) -> Result<(), SceneStorageError> {
    let mut camera_object = None;
    for object in &document.objects {
        validate_string(document, "object.name", object.name)?;
        validate_optional_resource(document, "object.resource", object.resource)?;
        validate_optional_material(document, "object.material", object.material)?;
        validate_string(document, "object.attachment", object.attachment)?;
        validate_range(
            "object.effect_range",
            object.effect_start,
            object.effect_count,
            document.object_effects.len(),
        )?;
        validate_range(
            "object.render_graph",
            object.render_graph,
            u32::from(object.render_graph != u32::MAX),
            document.render_graphs.len(),
        )?;
        if object.kind == SceneObjectKind::Camera {
            if camera_object.replace(object.id).is_some() {
                return Err(SceneStorageError::InvalidCameraLayer {
                    object: object.id,
                    reason: "multiple active camera layers require an authored queue resolver",
                });
            }
            if !object.camera_zoom.is_finite() || object.camera_zoom <= 0.0 {
                return Err(SceneStorageError::InvalidCameraLayer {
                    object: object.id,
                    reason: "base zoom must be finite and positive",
                });
            }
        } else if object.camera_zoom != 1.0 {
            return Err(SceneStorageError::InvalidCameraLayer {
                object: object.id,
                reason: "non-camera objects must not carry camera zoom",
            });
        }
    }
    let mut material_scalar_selectors = std::collections::BTreeSet::new();
    for program in &document.script_programs {
        validate_range(
            "script_program.object",
            program.object.0,
            1,
            document.objects.len(),
        )?;
        validate_string(document, "script_program.source", program.source)?;
        validate_string(
            document,
            "script_program.properties_json",
            program.properties_json,
        )?;
        validate_string(
            document,
            "script_program.initial_text",
            program.initial_text,
        )?;
        if program.subscriptions == SceneScriptSubscriptions::NONE
            || !program.initial_numeric.into_iter().all(f32::is_finite)
        {
            return Err(SceneStorageError::InvalidScriptProgram {
                object: program.object,
                reason: "empty subscriptions or non-finite initial value",
            });
        }
        if program.target == SceneScriptTarget::MaterialScalar {
            validate_range(
                "script_program.material_constant",
                program.selector,
                1,
                document.material_constants.len(),
            )?;
            if !material_scalar_selectors.insert(program.selector) {
                return Err(SceneStorageError::InvalidScriptProgram {
                    object: program.object,
                    reason: "duplicate material scalar selector",
                });
            }
        } else if program.selector != 0 {
            return Err(SceneStorageError::InvalidScriptProgram {
                object: program.object,
                reason: "object script target has a nonzero selector",
            });
        }
    }
    Ok(())
}
