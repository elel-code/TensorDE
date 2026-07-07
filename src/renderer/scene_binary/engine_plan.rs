//! `.gscn` to engine-owned scene plan lowering.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/rendering_server_default.h`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;

use crate::core::SceneNodeKind;
use crate::core::scene::binary::{
    SCENE_BINARY_NONE_ID, SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SceneBinaryChunkKind,
    SceneBinaryMaterialPassRecord, SceneBinaryNodeRecord, decode_effect_pass_record,
    decode_texture_slot_record,
};
use crate::engine::scene_engine::ingest::gscn::{
    GscnGeometryFact, GscnMaterialFact, GscnMeshResourceFact, GscnObjectFact, GscnObjectKind,
    GscnPuppetResourceFact, GscnResourceFact, GscnSceneCounts, GscnSceneFacts,
};
use crate::engine::scene_engine::{SceneCullMode, SceneDepthTest, SceneEnginePlan};
use crate::renderer::RendererPlanError;

use super::BINARY_TEXTURE_ROLE_BASE_COLOR;
use super::effect_program::gscn_effect_programs;
use super::facts::{
    BinarySceneNames, BinarySceneResource, binary_name, binary_scene_names,
    binary_scene_package_root, binary_scene_puppet_animation_layer_count, binary_scene_resources,
    binary_scene_size, binary_scene_timeline_counts,
};
use super::mesh::{
    binary_scene_geometry_is_mesh_payload, binary_scene_mesh_vertices_indices,
    binary_scene_puppet_active_sources, binary_scene_puppet_clipping_records,
    binary_scene_puppet_clips, binary_scene_puppet_layers, binary_scene_puppet_skin,
};
use super::reader::BinarySceneReader;
use super::topology::{
    BinarySceneRetainedRenderable, BinarySceneRetainedTopology, binary_scene_retained_topology,
};

pub(in crate::renderer) fn scene_engine_plan_from_gscn_path_with_properties(
    source_path: PathBuf,
    snapshot_time_ms: u64,
    _render_properties: Option<&BTreeMap<String, Value>>,
) -> Result<SceneEnginePlan, RendererPlanError> {
    let mut reader = BinarySceneReader::open(&source_path)?;
    let names = binary_scene_names(&mut reader)?;
    let package_root = binary_scene_package_root(&source_path);
    let resources = binary_scene_resources(&mut reader, &names, &package_root)?;
    let topology = binary_scene_retained_topology(&mut reader, &resources)?;
    let (mesh_resources, puppet_resources) =
        gscn_mesh_and_puppet_resources(&mut reader, &names, &topology)?;
    let scene_size = binary_scene_size(&mut reader)?;
    let (timeline_channel_count, timeline_owner_count) = binary_scene_timeline_counts(&mut reader)?;
    let puppet_animation_layer_count = binary_scene_puppet_animation_layer_count(&mut reader)?;
    let particle_emitter_count = reader.chunk_count(SceneBinaryChunkKind::ParticleEmitter);
    let material_pass_count = reader.chunk_count(SceneBinaryChunkKind::MaterialPass);
    let effect_pass_count = reader.chunk_count(SceneBinaryChunkKind::EffectPass);
    let target_width = scene_size.map(|size| size.width).unwrap_or(3840).max(1);
    let target_height = scene_size.map(|size| size.height).unwrap_or(2160).max(1);

    Ok(GscnSceneFacts {
        source: Some(source_path),
        snapshot_time_ms,
        target_width,
        target_height,
        counts: GscnSceneCounts {
            timeline_channel_count,
            timeline_owner_count,
            puppet_animation_layer_count,
            particle_emitter_count,
            material_pass_count,
            effect_pass_count,
        },
        resources: gscn_resources(&resources),
        mesh_resources,
        puppet_resources,
        objects: gscn_objects(&mut reader, &names, &resources, &topology)?,
    }
    .into_plan())
}

fn gscn_resources(resources: &[BinarySceneResource]) -> Vec<GscnResourceFact> {
    resources
        .iter()
        .map(|resource| GscnResourceFact {
            id_name: (resource.id_name != SCENE_BINARY_NONE_ID).then_some(resource.id_name),
            source: resource.source.clone(),
            width: resource.width,
            height: resource.height,
            format: resource.format,
            mip_count: resource.mip_count,
            payload_bytes: resource.payload_bytes,
        })
        .collect()
}

fn gscn_mesh_and_puppet_resources(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    topology: &BinarySceneRetainedTopology,
) -> Result<(Vec<GscnMeshResourceFact>, Vec<GscnPuppetResourceFact>), RendererPlanError> {
    let mut seen_meshes = BTreeSet::new();
    let mut seen_puppets = BTreeSet::new();
    let mut mesh_resources = Vec::new();
    let mut puppet_resources = Vec::new();

    for renderable in &topology.renderables {
        if !binary_scene_geometry_is_mesh_payload(renderable.geometry) {
            continue;
        }

        if seen_meshes.insert(renderable.node.geometry_index) {
            let (vertices, indices) =
                binary_scene_mesh_vertices_indices(reader, renderable.geometry)?;
            mesh_resources.push(GscnMeshResourceFact {
                source_record: renderable.node.geometry_index,
                vertices,
                indices,
            });
        }

        if renderable.node.puppet_index == SCENE_BINARY_NONE_ID
            || !seen_puppets.insert(renderable.node.puppet_index)
        {
            continue;
        }
        let Some(puppet) = renderable.puppet_record else {
            continue;
        };
        let skin = if puppet.bone_count > 0 {
            Some(binary_scene_puppet_skin(reader, names, puppet, true)?)
        } else {
            None
        };
        let clipping_records = if puppet.clipping_record_count > 0 && skin.is_some() {
            binary_scene_puppet_clipping_records(reader, names, puppet)?
        } else {
            Vec::new()
        };
        let clipping_active_sources = if puppet.active_source_count > 0 && skin.is_some() {
            binary_scene_puppet_active_sources(reader, names, puppet)?
        } else {
            Vec::new()
        };
        puppet_resources.push(GscnPuppetResourceFact {
            source_record: renderable.node.puppet_index,
            skin,
            clips: binary_scene_puppet_clips(reader, puppet)?,
            layers: binary_scene_puppet_layers(reader, puppet)?,
            clipping_records,
            clipping_active_sources,
        });
    }

    Ok((mesh_resources, puppet_resources))
}

fn gscn_objects(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    topology: &BinarySceneRetainedTopology,
) -> Result<Vec<GscnObjectFact>, RendererPlanError> {
    let mut objects = Vec::with_capacity(topology.renderables.len());
    let mut next_named_fbo = 0u32;
    for renderable in &topology.renderables {
        let layer_index = renderable.layer_index.min(u32::MAX as usize) as u32;
        objects.push(GscnObjectFact {
            layer_index,
            kind: gscn_object_kind(renderable.kind),
            geometry: gscn_geometry(renderable),
            material: gscn_material_fact(reader, names, renderable.material)?,
            source_resource_index: gscn_source_resource_index(
                reader,
                resources,
                renderable.node,
                renderable.material,
            )?,
            effects: gscn_effect_programs(
                reader,
                names,
                resources,
                renderable.material,
                &mut next_named_fbo,
            )?,
        });
    }
    Ok(objects)
}

fn gscn_object_kind(kind: SceneNodeKind) -> GscnObjectKind {
    match kind {
        SceneNodeKind::Image => GscnObjectKind::Image,
        SceneNodeKind::Video => GscnObjectKind::Video,
        SceneNodeKind::Color => GscnObjectKind::Color,
        SceneNodeKind::Text => GscnObjectKind::Text,
        SceneNodeKind::Path => GscnObjectKind::Path,
        SceneNodeKind::ParticleEmitter => GscnObjectKind::ParticleEmitter,
        SceneNodeKind::Rectangle | SceneNodeKind::Ellipse => GscnObjectKind::SolidShape,
        SceneNodeKind::Group
        | SceneNodeKind::Shader
        | SceneNodeKind::AudioResponse
        | SceneNodeKind::Audio
        | SceneNodeKind::Script
        | SceneNodeKind::Unknown => GscnObjectKind::Generic,
    }
}

fn gscn_geometry(renderable: &BinarySceneRetainedRenderable) -> GscnGeometryFact {
    if renderable.is_particle() {
        return GscnGeometryFact::ParticleEmitter;
    }
    let geometry_record_index = renderable.node.geometry_index;
    let vertex_count = renderable.geometry.vertex_count;
    let index_count = renderable.geometry.index_count;
    if binary_scene_geometry_is_mesh_payload(renderable.geometry) {
        if renderable.node.puppet_index == SCENE_BINARY_NONE_ID {
            GscnGeometryFact::Mesh {
                geometry_record_index,
                vertex_count,
                index_count,
            }
        } else if let Some(puppet) = renderable.puppet_record {
            GscnGeometryFact::Puppet {
                geometry_record_index,
                puppet_record_index: renderable.node.puppet_index,
                vertex_count,
                index_count,
                bone_count: puppet.bone_count,
                skin_vertex_count: puppet.skin_vertex_count,
                clip_count: puppet.clip_count,
                layer_count: puppet.animation_layer_count,
                clipping_record_count: puppet.clipping_record_count,
            }
        } else {
            GscnGeometryFact::Puppet {
                geometry_record_index,
                puppet_record_index: renderable.node.puppet_index,
                vertex_count,
                index_count,
                bone_count: 0,
                skin_vertex_count: 0,
                clip_count: 0,
                layer_count: 0,
                clipping_record_count: 0,
            }
        }
    } else {
        GscnGeometryFact::Quad
    }
}

fn gscn_source_resource_index(
    reader: &mut BinarySceneReader,
    resources: &[BinarySceneResource],
    node: SceneBinaryNodeRecord,
    material: Option<SceneBinaryMaterialPassRecord>,
) -> Result<Option<u32>, RendererPlanError> {
    if let Some((index, _resource)) = resources
        .iter()
        .enumerate()
        .find(|(_, resource)| resource.id_name == node.resource_name && resource.source.is_some())
    {
        return Ok(Some(index.min(u32::MAX as usize) as u32));
    }
    let Some(material) = material else {
        return Ok(None);
    };
    let slots = reader.record_range(
        SceneBinaryChunkKind::TextureSlots,
        SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
        material.first_texture_slot,
        material.texture_slot_count,
        decode_texture_slot_record,
    )?;
    for slot in slots {
        if slot.slot != 0 && slot.role_flags & BINARY_TEXTURE_ROLE_BASE_COLOR == 0 {
            continue;
        }
        let Some(resource) = resources.get(slot.resource_index as usize) else {
            continue;
        };
        if resource.source.is_none() {
            continue;
        }
        return Ok(Some(slot.resource_index));
    }
    Ok(None)
}

fn gscn_material_fact(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    material: Option<SceneBinaryMaterialPassRecord>,
) -> Result<GscnMaterialFact, RendererPlanError> {
    Ok(GscnMaterialFact {
        shader: gscn_effect_shader_name(reader, names, material)?,
        blending: material
            .and_then(|material| binary_name(names, material.blending_name))
            .map(str::to_owned),
        depth_test: material
            .map(|material| gscn_depth_test(material.depth_test))
            .unwrap_or(SceneDepthTest::Disabled),
        depth_write: material
            .map(|material| gscn_depth_write(material.depth_write))
            .unwrap_or(false),
        cull_mode: material
            .map(|material| gscn_cull_mode(material.cull_mode))
            .unwrap_or(SceneCullMode::None),
    })
}

fn gscn_depth_test(code: u16) -> SceneDepthTest {
    match code {
        1 => SceneDepthTest::LessEqual,
        2 => SceneDepthTest::Disabled,
        _ => SceneDepthTest::Disabled,
    }
}

fn gscn_depth_write(code: u16) -> bool {
    matches!(code, 1)
}

fn gscn_cull_mode(code: u16) -> SceneCullMode {
    match code {
        2 => SceneCullMode::Back,
        3 => SceneCullMode::Front,
        _ => SceneCullMode::None,
    }
}

fn gscn_effect_shader_name(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    material: Option<SceneBinaryMaterialPassRecord>,
) -> Result<Option<String>, RendererPlanError> {
    if let Some(material) = material
        && material.effect_pass_count > 0
    {
        let passes = reader.record_range(
            SceneBinaryChunkKind::EffectPass,
            reader.layout_record_size(SceneBinaryChunkKind::EffectPass)?,
            material.first_effect_pass,
            material.effect_pass_count,
            decode_effect_pass_record,
        )?;
        if let Some(shader) = passes
            .iter()
            .rev()
            .find_map(|pass| binary_name(names, pass.shader_name))
        {
            return Ok(Some(shader.to_owned()));
        }
    }
    Ok(None)
}
