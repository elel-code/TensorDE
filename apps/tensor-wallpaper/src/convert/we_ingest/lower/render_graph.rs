//! Lower typed render graphs and their resource bindings into scene ABI records.

use crate::engine::render_graph::{RenderTargetRole, TextureBindingRole};
use crate::engine::scene::*;

use super::super::ir::WeSceneIr;
use super::render_state::*;
use super::{StringInterner, WeLowerError};
use crate::convert::we_ingest::shader_key::canonical_scene_shader_key;

pub(super) type LoweredRenderGraphs = (
    Vec<SceneRenderGraphRecord>,
    Vec<SceneRenderPassRecord>,
    Vec<SceneRenderBindingRecord>,
    Vec<SceneUnsupportedRecord>,
);

pub(super) fn lower_render_graphs(
    ir: &WeSceneIr,
    strings: &mut StringInterner,
) -> Result<LoweredRenderGraphs, WeLowerError> {
    let mut graphs = Vec::new();
    let mut passes = Vec::new();
    let mut bindings = Vec::new();
    let mut unsupported = Vec::new();

    for (graph_index, graph) in ir.render_graphs.iter().enumerate() {
        let pass_start = passes.len() as u32;
        let unsupported_start = unsupported.len() as u32;
        let object_handle = graph
            .passes
            .iter()
            .find_map(|pass| pass.object_index)
            .map(|index| index as u32)
            .unwrap_or(graph_index as u32);
        for (local_pass_index, pass) in graph.passes.iter().enumerate() {
            let binding_start = bindings.len() as u32;
            let previous_target = local_pass_index.checked_sub(1).map(|previous_index| {
                let previous = &graph.passes[previous_index];
                (previous.target, previous.target_name.as_deref())
            });
            for binding in &pass.bindings {
                bindings.push(lower_binding(
                    binding,
                    previous_target,
                    graph_index,
                    pass.id,
                    strings,
                )?);
            }
            passes.push(SceneRenderPassRecord {
                id: pass.id,
                role: lower_pass_role(pass.role),
                draw_primitive: lower_pass_draw_primitive(pass.draw_primitive),
                object: SceneObjectHandle(
                    pass.object_index
                        .map(|index| index as u32)
                        .unwrap_or(INVALID_OBJECT_ID),
                ),
                material: SceneMaterialHandle(
                    pass.material_index
                        .map(|index| index as u32)
                        .unwrap_or(INVALID_MATERIAL_ID),
                ),
                pass_index: pass.pass_index,
                shader_key: strings.optional_id(&canonical_scene_shader_key(
                    pass.shader.as_deref().unwrap_or_default(),
                )),
                target: lower_render_target(pass.target),
                target_name: strings.optional_id(pass.target_name.as_deref().unwrap_or_default()),
                binding_start,
                binding_count: pass.bindings.len() as u32,
                effect_binding_start: pass.effect_visibility.binding_start,
                effect_binding_count: pass.effect_visibility.binding_count,
                effect_visibility_policy: lower_effect_visibility_policy(
                    pass.effect_visibility.policy,
                ),
                pipeline_blend: lower_pipeline_blend(pass.state.pipeline_blend),
                scene_blend: lower_scene_blend(pass.state.scene_blend),
                depth_test: lower_depth_test(pass.state.depth_test),
                depth_write: pass.state.depth_write,
                cull_mode: lower_cull_mode(pass.state.cull_mode),
                color_write_mask: lower_color_write_mask(pass.state.color_write_mask),
                clear_target: pass.state.clear_target,
            });
        }
        for boundary in &graph.unsupported {
            unsupported.push(SceneUnsupportedRecord {
                object: SceneObjectHandle(
                    boundary
                        .object_index
                        .map(|index| index as u32)
                        .unwrap_or(INVALID_OBJECT_ID),
                ),
                pass_index: boundary.pass_index.unwrap_or(u32::MAX),
                feature: strings.id(&boundary.feature),
                expected_subsystem: strings.id(&boundary.expected_subsystem),
                containment: strings.id(&boundary.containment),
            });
        }
        graphs.push(SceneRenderGraphRecord {
            object: SceneObjectHandle(object_handle),
            activation_policy: lower_render_graph_activation_policy(graph.activation_policy),
            source_extent_domain: ir
                .objects
                .iter()
                .find(|object| object.handle == object_handle)
                .map(|object| match object.render_source_extent_domain {
                    super::super::ir::WeIrRenderSourceExtentDomain::PhysicalSurface => {
                        SceneRenderSourceExtentDomain::PhysicalSurface
                    }
                    super::super::ir::WeIrRenderSourceExtentDomain::OwnerAuthored => {
                        SceneRenderSourceExtentDomain::OwnerAuthored
                    }
                })
                .unwrap_or(SceneRenderSourceExtentDomain::OwnerAuthored),
            pass_start,
            pass_count: graph.passes.len() as u32,
            unsupported_start,
            unsupported_count: graph.unsupported.len() as u32,
        });
    }

    let unsupported_start = unsupported.len();
    for entry in &ir.unsupported {
        unsupported.push(SceneUnsupportedRecord {
            object: SceneObjectHandle(entry.object.unwrap_or(INVALID_OBJECT_ID)),
            pass_index: entry.pass_index.unwrap_or(u32::MAX),
            feature: strings.id(&entry.feature),
            expected_subsystem: strings.id(&entry.expected_subsystem),
            containment: strings.id(&entry.containment),
        });
    }
    if unsupported.len() != unsupported_start && graphs.is_empty() {
        graphs.push(SceneRenderGraphRecord {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            activation_policy: SceneRenderGraphActivationPolicy::Always,
            source_extent_domain: SceneRenderSourceExtentDomain::OwnerAuthored,
            pass_start: 0,
            pass_count: 0,
            unsupported_start: unsupported_start as u32,
            unsupported_count: (unsupported.len() - unsupported_start) as u32,
        });
    }

    Ok((graphs, passes, bindings, unsupported))
}

pub(super) fn lower_binding(
    binding: &TextureBindingRole,
    previous_target: Option<(RenderTargetRole, Option<&str>)>,
    graph_index: usize,
    pass_id: u32,
    strings: &mut StringInterner,
) -> Result<SceneRenderBindingRecord, WeLowerError> {
    Ok(match binding {
        TextureBindingRole::SourceTexture => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::SourceTexture,
            slot: 0,
            target: SceneRenderTargetKind::SceneColor,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::TextureSlot { slot } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::TextureSlot,
            slot: *slot,
            target: SceneRenderTargetKind::SceneColor,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::AlphaTextureSlot { slot } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::AlphaTextureSlot,
            slot: *slot,
            target: SceneRenderTargetKind::SceneColor,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::PreviousGraphTarget { slot } => {
            let (target, name) =
                previous_target.ok_or(WeLowerError::MissingPreviousGraphTarget {
                    graph_index,
                    pass_id,
                    slot: *slot,
                })?;
            SceneRenderBindingRecord {
                kind: SceneRenderBindingKind::PreviousGraphTarget,
                slot: *slot,
                target: lower_render_target(target),
                name: strings.optional_id(name.unwrap_or_default()),
            }
        }
        TextureBindingRole::GraphTarget { slot, role, name } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::GraphTarget,
            slot: *slot,
            target: lower_render_target(*role),
            name: strings.optional_id(name.as_deref().unwrap_or_default()),
        },
        TextureBindingRole::NamedFboBind { slot, name } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::NamedFboBind,
            slot: *slot,
            target: SceneRenderTargetKind::NamedFbo,
            name: strings.id(name),
        },
        TextureBindingRole::EffectTarget { slot, name } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::EffectTarget,
            slot: *slot,
            target: SceneRenderTargetKind::FirstClassEffectTarget,
            name: strings.id(name),
        },
        TextureBindingRole::VideoFrame { media_instance } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::VideoFrame,
            slot: *media_instance,
            target: SceneRenderTargetKind::VideoExternalImage,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::AudioUniform => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::AudioUniform,
            slot: 0,
            target: SceneRenderTargetKind::Temporary,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::SystemUniform => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::SystemUniform,
            slot: 0,
            target: SceneRenderTargetKind::Temporary,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::PassConstant { name } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::PassConstant,
            slot: 0,
            target: SceneRenderTargetKind::Temporary,
            name: strings.id(name),
        },
    })
}
