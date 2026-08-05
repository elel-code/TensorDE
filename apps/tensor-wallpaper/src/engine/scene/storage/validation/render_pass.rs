use super::*;

pub(super) fn validate_framebuffer_composite_mesh(
    document: &SceneBinaryDocument,
    pass: &SceneRenderPassRecord,
) -> Result<(), SceneStorageError> {
    if pass.draw_primitive == SceneRenderPassDrawPrimitive::FramebufferCompositeMesh
        && (pass.role != SceneRenderPassKind::BaseMaterial || pass.object.0 == INVALID_OBJECT_ID)
    {
        return Err(SceneStorageError::InvalidRange {
            field: "render_pass.framebuffer_composite_mesh_contract",
            start: pass.object.0,
            count: pass.target.to_u32(),
            len: document.objects.len(),
        });
    }
    Ok(())
}
