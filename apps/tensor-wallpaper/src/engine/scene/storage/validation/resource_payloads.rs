// Resource and texture payload ownership/range validation.

fn validate_resources_and_textures(
    document: &SceneBinaryDocument,
) -> Result<(), SceneStorageError> {
    for resource in &document.resources {
        validate_string(document, "resource.path", resource.path)?;
        validate_string(document, "resource.source", resource.source)?;
        validate_payload(document, resource)?;
    }
    for texture in &document.textures {
        validate_resource(document, "texture.resource", texture.resource)?;
        validate_string(document, "texture.texv_tag", texture.texv_tag)?;
        validate_string(document, "texture.texb_tag", texture.texb_tag)?;
        validate_string(document, "texture.sequence_tag", texture.sequence_tag)?;
        validate_range(
            "texture.mip_range",
            texture.mip_start,
            texture.mip_count,
            document.texture_mips.len(),
        )?;
        validate_range(
            "texture.sequence_frame_range",
            texture.sequence_frame_start,
            texture.sequence_frame_count,
            document.texture_sequence_frames.len(),
        )?;
        let sequence_start = texture.sequence_frame_start as usize;
        let sequence_end = sequence_start + texture.sequence_frame_count as usize;
        let frames = &document.texture_sequence_frames[sequence_start..sequence_end];
        for frame in frames {
            if frame.resource_index != 0
                || ![
                    frame.duration,
                    frame.origin[0],
                    frame.origin[1],
                    frame.axis_x[0],
                    frame.axis_x[1],
                    frame.axis_y[0],
                    frame.axis_y[1],
                ]
                .into_iter()
                .all(f32::is_finite)
            {
                return Err(SceneStorageError::InvalidTextureSequence {
                    resource: texture.resource,
                    reason: "frame resource index must be zero and affine values must be finite",
                });
            }
        }
        if !frames.is_empty() && texture_sequence::texture_sequence_layout(frames).is_none() {
            return Err(SceneStorageError::InvalidTextureSequence {
                resource: texture.resource,
                reason: "runtime particle sampling requires a constant axis-aligned row-major frame grid",
            });
        }
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
