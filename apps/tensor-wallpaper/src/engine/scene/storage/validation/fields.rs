use super::super::*;

pub(super) fn validate_project(document: &SceneBinaryDocument) -> Result<(), SceneStorageError> {
    let project = &document.project;
    validate_string(document, "project.title", project.title)?;
    validate_string(document, "project.wallpaper_type", project.wallpaper_type)?;
    validate_string(document, "project.scene_file", project.scene_file)?;
    validate_string(document, "project.preview", project.preview)?;
    validate_string(document, "project.properties_json", project.properties_json)
}

pub(super) fn validate_string(
    document: &SceneBinaryDocument,
    field: &'static str,
    id: SceneStringId,
) -> Result<(), SceneStorageError> {
    if !id.is_some() || (id.0 as usize) < document.strings.len() {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidStringId { field, id })
    }
}

pub(super) fn validate_resource(
    document: &SceneBinaryDocument,
    field: &'static str,
    id: SceneResourceId,
) -> Result<(), SceneStorageError> {
    if document.resources.iter().any(|resource| resource.id == id) {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidResourceId { field, id })
    }
}

pub(super) fn validate_optional_resource(
    document: &SceneBinaryDocument,
    field: &'static str,
    id: SceneResourceId,
) -> Result<(), SceneStorageError> {
    if !id.is_some() {
        Ok(())
    } else {
        validate_resource(document, field, id)
    }
}

pub(super) fn validate_optional_material(
    document: &SceneBinaryDocument,
    field: &'static str,
    handle: SceneMaterialHandle,
) -> Result<(), SceneStorageError> {
    if handle.0 == INVALID_MATERIAL_ID || (handle.0 as usize) < document.materials.len() {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidMaterialHandle { field, handle })
    }
}

pub(super) fn validate_payload(
    document: &SceneBinaryDocument,
    resource: &SceneResourceRecord,
) -> Result<(), SceneStorageError> {
    let Ok(start) = usize::try_from(resource.payload_offset) else {
        return Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        });
    };
    let Ok(len) = usize::try_from(resource.payload_len) else {
        return Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        });
    };
    let Some(end) = start.checked_add(len) else {
        return Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        });
    };
    if end <= document.resource_payload.len() {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidPayloadRange {
            resource: resource.id,
            offset: resource.payload_offset,
            len: resource.payload_len,
            payload_len: document.resource_payload.len(),
        })
    }
}

pub(super) fn validate_texture_payload(
    document: &SceneBinaryDocument,
    texture: SceneResourceId,
    offset: u64,
    len: u64,
) -> Result<(), SceneStorageError> {
    let valid = usize::try_from(offset)
        .ok()
        .and_then(|start| {
            usize::try_from(len)
                .ok()
                .and_then(|len| start.checked_add(len))
        })
        .is_some_and(|end| end <= document.texture_payload.len());
    if valid {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidTexturePayloadRange {
            texture,
            offset,
            len,
            payload_len: document.texture_payload.len(),
        })
    }
}

pub(super) fn validate_range(
    field: &'static str,
    start: u32,
    count: u32,
    len: usize,
) -> Result<(), SceneStorageError> {
    if start == u32::MAX && count == 0 {
        return Ok(());
    }
    let start_usize = start as usize;
    let count_usize = count as usize;
    let Some(end) = start_usize.checked_add(count_usize) else {
        return Err(SceneStorageError::InvalidRange {
            field,
            start,
            count,
            len,
        });
    };
    if end <= len {
        Ok(())
    } else {
        Err(SceneStorageError::InvalidRange {
            field,
            start,
            count,
            len,
        })
    }
}
