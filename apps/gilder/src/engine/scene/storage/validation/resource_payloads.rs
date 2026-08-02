// Resource and texture payload ownership/range validation.

fn validate_resources_and_textures(document: &SceneBinaryDocument) -> Result<(), SceneStorageError> {
    for resource in &document.resources {
        validate_string(document, "resource.path", resource.path)?;
        validate_string(document, "resource.source", resource.source)?;
        validate_payload(document, resource)?;
    }
    for texture in &document.textures {
        validate_resource(document, "texture.resource", texture.resource)?;
        validate_string(document, "texture.texv_tag", texture.texv_tag)?;
        validate_string(document, "texture.texb_tag", texture.texb_tag)?;
        validate_range(
            "texture.mip_range",
            texture.mip_start,
            texture.mip_count,
            document.texture_mips.len(),
        )?;
        validate_texture_payload(
            document,
            texture.resource,
            texture.payload_offset,
            texture.payload_len,
        )?;
        for mip in document
            .texture_mips
            .iter()
            .skip(texture.mip_start as usize)
            .take(texture.mip_count as usize)
        {
            validate_texture_payload(
                document,
                texture.resource,
                mip.payload_offset,
                mip.payload_len,
            )?;
        }
    }
    Ok(())
}
