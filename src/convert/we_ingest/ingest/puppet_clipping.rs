//! MDLV0023 token-one clipping graph lowering.

use super::WeIrBuilder;
use crate::convert::we_ingest::ir::{WeIrMaterial, WeIrMaterialPass, WeIrMaterialTexture};
use crate::engine::render_graph::{
    DepthTestMode, PassState, PipelineBlendMode, RenderGraph, RenderPassNode, RenderPassRole,
    RenderTargetRole, RenderTargetSpec, TextureBindingRole,
};
use crate::engine::scene::abi::{SceneCullMode, SceneDepthTest, ScenePipelineBlend};

const FULL_ALPHA_MASK: &str = "_rt_FullAlphaMask";

#[allow(dead_code)]
pub(super) fn apply_token_one_graph(
    builder: &mut WeIrBuilder,
    object: u32,
    base_material: u32,
    graph: &mut RenderGraph,
) {
    let clipping_meshes = builder
        .meshes
        .iter()
        .enumerate()
        .filter(|(_, mesh)| mesh.object == object)
        .filter(|(mesh, _)| {
            builder
                .mesh_clipping_subdraws
                .iter()
                .any(|subdraw| subdraw.mesh == *mesh as u32)
        })
        .map(|(mesh, _)| mesh as u32)
        .collect::<Vec<_>>();
    let [mesh] = clipping_meshes.as_slice() else {
        return;
    };
    let [scene_pass] = graph.passes.as_slice() else {
        return;
    };
    let Some(clipped_shader) = scene_pass.shader.as_deref().and_then(clipped_shader_key) else {
        return;
    };
    let subdraws = builder
        .mesh_clipping_subdraws
        .iter()
        .filter(|subdraw| subdraw.mesh == *mesh)
        .cloned()
        .collect::<Vec<_>>();
    if subdraws.is_empty()
        || subdraws.iter().any(|subdraw| {
            subdraw.raw_flags != 0
                || subdraw.mask_resource.is_none()
                || subdraw.target_source_count == 0
                || subdraw.mask_source_count == 0
        })
    {
        return;
    }
    let original = scene_pass.clone();
    let mask_resources = subdraws
        .iter()
        .map(|subdraw| subdraw.mask_resource.unwrap())
        .collect::<Vec<_>>();
    let Some(mask_materials) = mask_resources
        .into_iter()
        .map(|resource| clipping_mask_material(builder, base_material, resource))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };

    let mut passes = Vec::with_capacity(2 + subdraws.len() * 2);
    push_visible_pass(&mut passes, &original, RenderPassRole::MeshVisiblePrefix);
    for (subdraw, mask_material) in mask_materials.into_iter().enumerate() {
        passes.push(RenderPassNode {
            id: 0,
            role: RenderPassRole::MeshClippingMask,
            object_index: Some(object as usize),
            material_index: Some(mask_material as usize),
            pass_index: subdraw as u32,
            shader: Some("we/clippingmaskimage4__PUPPETSKINNING_1".to_owned()),
            target: RenderTargetRole::FirstClassEffectTarget,
            target_name: Some(FULL_ALPHA_MASK.to_owned()),
            target_extent: None,
            target_format: Some("r8".to_owned()),
            bindings: Vec::new(),
            effect_visibility: crate::engine::render_graph::RenderPassEffectVisibility::NONE,
            state: PassState {
                pipeline_blend: PipelineBlendMode::Normal,
                depth_test: DepthTestMode::Disabled,
                ..PassState::default()
            },
        });
        let mut target = original.clone();
        target.role = RenderPassRole::MeshClippedTarget;
        target.pass_index = subdraw as u32;
        target.shader = Some(clipped_shader.to_owned());
        target.bindings.push(TextureBindingRole::GraphTarget {
            slot: 8,
            role: RenderTargetRole::FirstClassEffectTarget,
            name: Some(FULL_ALPHA_MASK.to_owned()),
        });
        passes.push(target);
    }
    push_visible_pass(&mut passes, &original, RenderPassRole::MeshVisibleRemainder);
    for (id, pass) in passes.iter_mut().enumerate() {
        pass.id = id as u32;
    }
    graph.passes = passes;
    graph.target_specs.push(RenderTargetSpec {
        role: RenderTargetRole::FirstClassEffectTarget,
        name: FULL_ALPHA_MASK.to_owned(),
        format: "r8".to_owned(),
        width_divisor_milli: 2_000,
        height_divisor_milli: 2_000,
    });
}

fn push_visible_pass(
    passes: &mut Vec<RenderPassNode>,
    original: &RenderPassNode,
    role: RenderPassRole,
) {
    let mut pass = original.clone();
    pass.role = role;
    pass.pass_index = 0;
    passes.push(pass);
}

fn clipped_shader_key(shader: &str) -> Option<&'static str> {
    match shader.to_ascii_lowercase().as_str() {
        "we/puppet-opacity-final" => Some("we/puppet-opacity-clipping-final"),
        "we/puppet-iris-waterripple-final" => Some("we/puppet-iris-waterripple-clipping-final"),
        _ => None,
    }
}

fn clipping_mask_material(
    builder: &mut WeIrBuilder,
    base_material: u32,
    mask_resource: u32,
) -> Option<u32> {
    let base = builder.materials.get(base_material as usize)?.clone();
    let base_pass = builder
        .material_passes
        .get(base.pass_start as usize)?
        .clone();
    let source = builder
        .material_textures
        .iter()
        .skip(base_pass.texture_start as usize)
        .take(base_pass.texture_count as usize)
        .find(|texture| texture.slot == 0)?
        .clone();
    let handle = builder.materials.len() as u32;
    let texture_start = builder.material_textures.len() as u32;
    builder.material_textures.push(WeIrMaterialTexture {
        slot: 0,
        resource: source.resource,
        path: source.path,
    });
    builder.material_textures.push(WeIrMaterialTexture {
        slot: 1,
        resource: Some(mask_resource),
        path: String::new(),
    });
    let pass_start = builder.material_passes.len() as u32;
    builder.material_passes.push(WeIrMaterialPass {
        material: handle,
        shader_key: "we/clippingmaskimage4__PUPPETSKINNING_1".to_owned(),
        target: FULL_ALPHA_MASK.to_owned(),
        texture_start,
        texture_count: 2,
        constant_start: builder.material_constants.len() as u32,
        constant_count: 0,
        pipeline_blend: ScenePipelineBlend::Normal,
        depth_test: SceneDepthTest::Disabled,
        depth_write: false,
        cull_mode: SceneCullMode::None,
        alpha_writing: String::new(),
        clear_target: false,
    });
    builder.materials.push(WeIrMaterial {
        handle,
        resource: base.resource,
        pass_start,
        pass_count: 1,
    });
    Some(handle)
}
