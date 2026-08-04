//! MDLV0023 token-one clipping graph lowering.

use super::WeIrBuilder;
use crate::convert::we_ingest::ir::{
    WeIrMaterial, WeIrMaterialPass, WeIrMaterialTexture, WeIrShaderOrigin,
};
use crate::convert::we_ingest::shader_key::canonical_scene_shader_key;
use crate::engine::render_graph::{
    ColorWriteMask, DepthTestMode, PassState, PipelineBlendMode, RenderGraph,
    RenderPassDrawPrimitive, RenderPassNode, RenderPassRole, RenderTargetExtentDomain,
    RenderTargetRole, RenderTargetSpec, TextureBindingRole, UnsupportedGraphBoundary,
};
use crate::engine::scene::abi::{SceneCullMode, SceneDepthTest, ScenePipelineBlend};

const FULL_ALPHA_MASK: &str = "_rt_FullAlphaMask";

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
    if clipping_meshes.is_empty() {
        return;
    }
    let [mesh] = clipping_meshes.as_slice() else {
        report_unsupported(
            graph,
            object,
            None,
            format!(
                "mdlv0023-token-one-clipping-mesh-count:{}",
                clipping_meshes.len()
            ),
        );
        return;
    };
    let scene_mesh_passes = graph
        .passes
        .iter()
        .enumerate()
        .filter(|(_, pass)| {
            pass.draw_primitive == RenderPassDrawPrimitive::ObjectMesh
                && pass.target == RenderTargetRole::SceneColor
        })
        .collect::<Vec<_>>();
    let compatible_scene_mesh_passes = scene_mesh_passes
        .iter()
        .filter_map(|(index, pass)| {
            pass.shader
                .as_deref()
                .and_then(clipped_shader_key)
                .map(|shader| (*index, (*pass).clone(), shader))
        })
        .collect::<Vec<_>>();
    if compatible_scene_mesh_passes.len() != scene_mesh_passes.len()
        || compatible_scene_mesh_passes.is_empty()
    {
        if scene_mesh_passes.len() == 1 && compatible_scene_mesh_passes.is_empty() {
            let shader = scene_mesh_passes[0].1.shader.as_deref().unwrap_or("<none>");
            report_unsupported(
                graph,
                object,
                None,
                format!("mdlv0023-token-one-clipping-unsupported-shader:{shader}"),
            );
        } else {
            report_unsupported(
                graph,
                object,
                None,
                format!(
                    "mdlv0023-token-one-clipping-terminal-mesh-pass-count:{}",
                    scene_mesh_passes.len()
                ),
            );
        }
        return;
    }
    let mut clipping_terminals = Vec::with_capacity(compatible_scene_mesh_passes.len());
    for (terminal_index, mut original, clipped_shader) in compatible_scene_mesh_passes {
        if original
            .bindings
            .iter()
            .any(|binding| matches!(binding, TextureBindingRole::PreviousGraphTarget { .. }))
        {
            let Some((role, name)) = terminal_index
                .checked_sub(1)
                .and_then(|index| graph.passes.get(index))
                .map(|pass| (pass.target, pass.target_name.clone()))
            else {
                report_unsupported(
                    graph,
                    object,
                    Some(original.pass_index),
                    "mdlv0023-token-one-clipping-terminal-previous-target-missing".to_owned(),
                );
                return;
            };
            for binding in &mut original.bindings {
                if let TextureBindingRole::PreviousGraphTarget { slot } = binding {
                    *binding = TextureBindingRole::GraphTarget {
                        slot: *slot,
                        role,
                        name: name.clone(),
                    };
                }
            }
        }
        clipping_terminals.push((terminal_index, original, clipped_shader));
    }
    let subdraws = builder
        .mesh_clipping_subdraws
        .iter()
        .filter(|subdraw| subdraw.mesh == *mesh)
        .cloned()
        .collect::<Vec<_>>();
    if let Some((subdraw, reason)) = subdraws.iter().enumerate().find_map(|(index, subdraw)| {
        let reason = if subdraw.raw_flags != 0 {
            Some(format!("flags-0x{:08x}", subdraw.raw_flags))
        } else if subdraw.mask_resource.is_none() {
            Some("missing-mask-resource".to_owned())
        } else if subdraw.target_source_count == 0 {
            Some("empty-target-source-list".to_owned())
        } else if subdraw.mask_source_count == 0 {
            Some("empty-mask-source-list".to_owned())
        } else {
            None
        };
        reason.map(|reason| (index as u32, reason))
    }) {
        report_unsupported(
            graph,
            object,
            Some(subdraw),
            format!("mdlv0023-token-one-clipping-invalid-subdraw:{reason}"),
        );
        return;
    }
    let mask_resources = subdraws
        .iter()
        .filter_map(|subdraw| subdraw.mask_resource)
        .collect::<Vec<_>>();
    let Some(mask_materials) = mask_resources
        .into_iter()
        .map(|resource| clipping_mask_material(builder, base_material, resource))
        .collect::<Option<Vec<_>>>()
    else {
        report_unsupported(
            graph,
            object,
            None,
            "mdlv0023-token-one-clipping-mask-material".to_owned(),
        );
        return;
    };

    let has_visible_prefix = builder.mesh_clipping_slices.iter().any(|slice| {
        slice.mesh == *mesh
            && slice.role == crate::convert::we_ingest::ir::WeIrMeshClippingSliceRole::VisiblePrefix
    });
    let visible_remainders = builder
        .mesh_clipping_slices
        .iter()
        .filter(|slice| {
            slice.mesh == *mesh
                && slice.role
                    == crate::convert::we_ingest::ir::WeIrMeshClippingSliceRole::VisibleRemainder
        })
        .map(|slice| slice.subdraw)
        .collect::<std::collections::BTreeSet<_>>();
    let terminal_count = clipping_terminals.len();
    let mut passes =
        Vec::with_capacity(graph.passes.len() + terminal_count * (1 + subdraws.len() * 3));
    for (pass_index, pass) in graph.passes.iter().enumerate() {
        let Some((_, original, clipped_shader)) = clipping_terminals
            .iter()
            .find(|(terminal_index, _, _)| *terminal_index == pass_index)
        else {
            passes.push(pass.clone());
            continue;
        };
        if has_visible_prefix {
            push_visible_pass(
                &mut passes,
                original,
                RenderPassRole::MeshVisiblePrefix,
                u32::MAX,
            );
        }
        for (subdraw, mask_material) in mask_materials.iter().copied().enumerate() {
            passes.push(RenderPassNode {
                id: 0,
                role: RenderPassRole::MeshClippingMask,
                draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
                object_index: Some(object as usize),
                material_index: Some(mask_material as usize),
                pass_index: subdraw as u32,
                shader: Some("we/clippingmaskimage4__PUPPETSKINNING_1".to_owned()),
                target: RenderTargetRole::FirstClassEffectTarget,
                target_name: Some(FULL_ALPHA_MASK.to_owned()),
                target_extent: None,
                target_format: Some("r8".to_owned()),
                bindings: Vec::new(),
                effect_visibility: original.effect_visibility.clone(),
                state: PassState {
                    pipeline_blend: PipelineBlendMode::Translucent,
                    depth_test: DepthTestMode::Disabled,
                    clear_target: true,
                    ..PassState::default()
                },
            });
            let mut target = original.clone();
            target.role = RenderPassRole::MeshClippedTarget;
            target.pass_index = subdraw as u32;
            target.shader = Some(clipped_shader.clone());
            target.state.pipeline_blend = PipelineBlendMode::Translucent;
            target.state.color_write_mask = ColorWriteMask::Rgb;
            target.bindings.push(TextureBindingRole::GraphTarget {
                slot: 8,
                role: RenderTargetRole::FirstClassEffectTarget,
                name: Some(FULL_ALPHA_MASK.to_owned()),
            });
            passes.push(target);
            if visible_remainders.contains(&(subdraw as u32)) {
                push_visible_pass(
                    &mut passes,
                    original,
                    RenderPassRole::MeshVisibleRemainder,
                    subdraw as u32,
                );
            }
        }
    }
    for (id, pass) in passes.iter_mut().enumerate() {
        pass.id = id as u32;
    }
    graph.passes = passes;
    graph.target_specs.push(RenderTargetSpec {
        role: RenderTargetRole::FirstClassEffectTarget,
        name: FULL_ALPHA_MASK.to_owned(),
        format: "r8".to_owned(),
        // The clipping mesh is projected in the scene domain, so this
        // generated mask follows the physical SceneColor extent.
        extent_domain: RenderTargetExtentDomain::PhysicalSurface,
        width_divisor_milli: 2_000,
        height_divisor_milli: 2_000,
    });
}

fn report_unsupported(
    graph: &mut RenderGraph,
    object: u32,
    pass_index: Option<u32>,
    feature: String,
) {
    graph.unsupported.push(UnsupportedGraphBoundary {
        object_index: Some(object as usize),
        pass_index,
        feature,
        expected_subsystem: "convert/we_ingest MDLV0023 clipping graph lowering".to_owned(),
        containment: "authored-clipping-subdraw-not-lowered".to_owned(),
    });
}

fn push_visible_pass(
    passes: &mut Vec<RenderPassNode>,
    original: &RenderPassNode,
    role: RenderPassRole,
    pass_index: u32,
) {
    let mut pass = original.clone();
    pass.role = role;
    pass.pass_index = pass_index;
    passes.push(pass);
}

fn clipped_shader_key(shader: &str) -> Option<String> {
    match canonical_scene_shader_key(shader)
        .to_ascii_lowercase()
        .as_str()
    {
        "we/genericimage4__puppetskinning_1" => {
            Some("we/genericimage4__PUPPETSKINNING_1__CLIPPINGTARGET_1__CLIPPINGUVS_1".to_owned())
        }
        "we/puppet-opacity-final" => Some("we/puppet-opacity-clipping-final".to_owned()),
        "we/puppet-iris-waterripple-final" => {
            Some("we/puppet-iris-waterripple-clipping-final".to_owned())
        }
        "we/puppet-effect-composite" => Some("we/puppet-effect-composite-clipping".to_owned()),
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
        shader_source_key: "clippingmaskimage4".to_owned(),
        shader_origin: WeIrShaderOrigin::EngineBuiltIn,
        target: FULL_ALPHA_MASK.to_owned(),
        texture_start,
        texture_count: 2,
        constant_start: builder.material_constants.len() as u32,
        constant_count: 0,
        pipeline_blend: ScenePipelineBlend::Translucent,
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
