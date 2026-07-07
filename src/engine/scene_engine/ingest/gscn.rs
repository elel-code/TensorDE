//! `.gscn` fact lowering into the engine-owned scene plan.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/rendering_server_default.h`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::core::scene::{
    SceneMeshPuppetClippingRecord, SceneMeshSkin, SceneMeshVertex, ScenePuppetAnimationClip,
    ScenePuppetAnimationLayer,
};

use super::super::{
    SceneBlendContract, SceneCullMode, SceneDepthTest, SceneEnginePlan, SceneGeometryId,
    SceneMaterialContract, SceneMaterialRenderState, SceneObject, SceneObjectEffectProgram,
    SceneObjectGeometry, SceneObjectId, ScenePuppetId, SceneResource, SceneResourceId,
    SceneTextureFormat,
};
use crate::engine::scene_engine::SceneEffectProgram;

#[derive(Debug, Clone, PartialEq)]
pub struct GscnSceneFacts {
    pub source: Option<PathBuf>,
    pub snapshot_time_ms: u64,
    pub target_width: u32,
    pub target_height: u32,
    pub counts: GscnSceneCounts,
    pub resources: Vec<GscnResourceFact>,
    pub mesh_resources: Vec<GscnMeshResourceFact>,
    pub puppet_resources: Vec<GscnPuppetResourceFact>,
    pub objects: Vec<GscnObjectFact>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GscnSceneCounts {
    pub timeline_channel_count: usize,
    pub timeline_owner_count: usize,
    pub puppet_animation_layer_count: usize,
    pub particle_emitter_count: usize,
    pub material_pass_count: usize,
    pub effect_pass_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GscnResourceFact {
    pub id_name: Option<u32>,
    pub source: Option<PathBuf>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<SceneTextureFormat>,
    pub mip_count: Option<u32>,
    pub payload_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GscnMeshResourceFact {
    pub source_record: u32,
    pub vertices: Vec<SceneMeshVertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GscnPuppetResourceFact {
    pub source_record: u32,
    pub skin: Option<SceneMeshSkin>,
    pub clips: Vec<ScenePuppetAnimationClip>,
    pub layers: Vec<ScenePuppetAnimationLayer>,
    pub clipping_records: Vec<SceneMeshPuppetClippingRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GscnObjectFact {
    pub layer_index: u32,
    pub kind: GscnObjectKind,
    pub geometry: GscnGeometryFact,
    pub material: GscnMaterialFact,
    pub source_resource_index: Option<u32>,
    pub effects: Vec<SceneEffectProgram>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GscnObjectKind {
    Image,
    Video,
    Color,
    Text,
    Path,
    ParticleEmitter,
    SolidShape,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GscnGeometryFact {
    Quad,
    Mesh {
        geometry_record_index: u32,
        vertex_count: u32,
        index_count: u32,
    },
    Puppet {
        geometry_record_index: u32,
        puppet_record_index: u32,
        vertex_count: u32,
        index_count: u32,
        bone_count: u32,
        skin_vertex_count: u32,
        clip_count: u32,
        layer_count: u32,
        clipping_record_count: u32,
    },
    ParticleEmitter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GscnMaterialFact {
    pub shader: Option<String>,
    pub blend_code: Option<u16>,
    pub depth_test: SceneDepthTest,
    pub depth_write: bool,
    pub cull_mode: SceneCullMode,
}

impl GscnSceneFacts {
    pub fn into_plan(self) -> SceneEnginePlan {
        let GscnSceneFacts {
            source,
            snapshot_time_ms,
            target_width,
            target_height,
            counts,
            resources: texture_resources,
            mesh_resources,
            puppet_resources,
            objects: object_facts,
        } = self;

        let texture_ids = engine_texture_resource_ids(&texture_resources);
        let geometry_facts = engine_geometry_resource_facts(mesh_resources);
        let puppet_facts = engine_puppet_resource_facts(puppet_resources);
        let geometry_ids = engine_geometry_ids(&geometry_facts);
        let puppet_ids = engine_puppet_ids(&puppet_facts);
        let effects = engine_effect_programs(&object_facts);
        let objects = engine_objects(&texture_ids, &geometry_ids, &puppet_ids, object_facts);
        let resources = engine_resources(
            texture_resources,
            &texture_ids,
            geometry_facts,
            &geometry_ids,
            puppet_facts,
            &puppet_ids,
        );
        SceneEnginePlan {
            source,
            snapshot_time_ms,
            target_width: target_width.max(1),
            target_height: target_height.max(1),
            resources,
            objects,
            effects,
            timeline_channel_count: counts.timeline_channel_count,
            timeline_owner_count: counts.timeline_owner_count,
            puppet_animation_layer_count: counts.puppet_animation_layer_count,
            particle_emitter_count: counts.particle_emitter_count,
            material_pass_count: counts.material_pass_count,
            effect_pass_count: counts.effect_pass_count,
        }
    }
}

fn engine_effect_programs(objects: &[GscnObjectFact]) -> Vec<SceneObjectEffectProgram> {
    objects
        .iter()
        .flat_map(|object| {
            object
                .effects
                .iter()
                .cloned()
                .map(|program| SceneObjectEffectProgram {
                    object: SceneObjectId(object.layer_index),
                    program,
                })
        })
        .collect()
}

fn engine_texture_resource_ids(resources: &[GscnResourceFact]) -> Vec<Option<SceneResourceId>> {
    resources
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            resource
                .source
                .as_ref()
                .map(|_| engine_resource_id(index, resource))
        })
        .collect()
}

fn engine_geometry_resource_facts(
    mesh_resources: Vec<GscnMeshResourceFact>,
) -> BTreeMap<u32, GscnMeshResourceFact> {
    let mut resources = BTreeMap::new();
    for resource in mesh_resources {
        resources.entry(resource.source_record).or_insert(resource);
    }
    resources
}

fn engine_puppet_resource_facts(
    puppet_resources: Vec<GscnPuppetResourceFact>,
) -> BTreeMap<u32, GscnPuppetResourceFact> {
    let mut resources = BTreeMap::new();
    for resource in puppet_resources {
        resources.entry(resource.source_record).or_insert(resource);
    }
    resources
}

fn engine_geometry_ids(
    resources: &BTreeMap<u32, GscnMeshResourceFact>,
) -> BTreeMap<u32, SceneGeometryId> {
    resources
        .keys()
        .enumerate()
        .map(|(index, source_record)| {
            (
                *source_record,
                SceneGeometryId(index.min(u32::MAX as usize) as u32),
            )
        })
        .collect()
}

fn engine_puppet_ids(
    resources: &BTreeMap<u32, GscnPuppetResourceFact>,
) -> BTreeMap<u32, ScenePuppetId> {
    resources
        .keys()
        .enumerate()
        .map(|(index, source_record)| {
            (
                *source_record,
                ScenePuppetId(index.min(u32::MAX as usize) as u32),
            )
        })
        .collect()
}

fn engine_resources(
    resources: Vec<GscnResourceFact>,
    texture_ids: &[Option<SceneResourceId>],
    geometry_facts: BTreeMap<u32, GscnMeshResourceFact>,
    geometry_ids: &BTreeMap<u32, SceneGeometryId>,
    puppet_facts: BTreeMap<u32, GscnPuppetResourceFact>,
    puppet_ids: &BTreeMap<u32, ScenePuppetId>,
) -> Vec<SceneResource> {
    let mut output = resources
        .into_iter()
        .enumerate()
        .filter_map(|(index, resource)| {
            Some(SceneResource::Texture {
                id: texture_ids.get(index).copied().flatten()?,
                source: resource.source?,
                width: resource.width,
                height: resource.height,
                format: resource.format,
                mip_count: resource.mip_count,
                payload_bytes: resource.payload_bytes,
            })
        })
        .collect::<Vec<_>>();
    output.extend(
        geometry_facts
            .into_iter()
            .filter_map(|(source_record, fact)| {
                Some(SceneResource::MeshGeometry {
                    id: *geometry_ids.get(&source_record)?,
                    source_record: fact.source_record,
                    vertices: fact.vertices,
                    indices: fact.indices,
                })
            }),
    );
    output.extend(
        puppet_facts
            .into_iter()
            .filter_map(|(source_record, fact)| {
                Some(SceneResource::PuppetRig {
                    id: *puppet_ids.get(&source_record)?,
                    source_record: fact.source_record,
                    skin: fact.skin,
                    clips: fact.clips,
                    layers: fact.layers,
                    clipping_records: fact.clipping_records,
                })
            }),
    );
    output
}

fn engine_objects(
    texture_ids: &[Option<SceneResourceId>],
    geometry_ids: &BTreeMap<u32, SceneGeometryId>,
    puppet_ids: &BTreeMap<u32, ScenePuppetId>,
    objects: Vec<GscnObjectFact>,
) -> Vec<SceneObject> {
    objects
        .into_iter()
        .map(|object| SceneObject {
            id: SceneObjectId(object.layer_index),
            geometry: engine_geometry(object.geometry, geometry_ids, puppet_ids),
            material: engine_material(object.kind, object.material),
            source: engine_source_resource(texture_ids, object.source_resource_index),
        })
        .collect()
}

fn engine_resource_id(index: usize, resource: &GscnResourceFact) -> SceneResourceId {
    SceneResourceId(
        resource
            .id_name
            .unwrap_or_else(|| index.min(u32::MAX as usize) as u32),
    )
}

fn engine_source_resource(
    resource_ids: &[Option<SceneResourceId>],
    source_resource_index: Option<u32>,
) -> Option<SceneResourceId> {
    let index = source_resource_index? as usize;
    resource_ids.get(index).copied().flatten()
}

fn engine_geometry(
    geometry: GscnGeometryFact,
    geometry_ids: &BTreeMap<u32, SceneGeometryId>,
    puppet_ids: &BTreeMap<u32, ScenePuppetId>,
) -> SceneObjectGeometry {
    match geometry {
        GscnGeometryFact::Quad => SceneObjectGeometry::Quad,
        GscnGeometryFact::Mesh {
            geometry_record_index,
            vertex_count,
            index_count,
        } => SceneObjectGeometry::Mesh {
            geometry: geometry_ids
                .get(&geometry_record_index)
                .copied()
                .unwrap_or(SceneGeometryId(u32::MAX)),
            vertex_count,
            index_count,
        },
        GscnGeometryFact::Puppet {
            geometry_record_index,
            puppet_record_index,
            vertex_count,
            index_count,
            ..
        } => SceneObjectGeometry::Puppet {
            geometry: geometry_ids
                .get(&geometry_record_index)
                .copied()
                .unwrap_or(SceneGeometryId(u32::MAX)),
            puppet: puppet_ids
                .get(&puppet_record_index)
                .copied()
                .unwrap_or(ScenePuppetId(u32::MAX)),
            vertex_count,
            index_count,
        },
        GscnGeometryFact::ParticleEmitter => SceneObjectGeometry::ParticleEmitter,
    }
}

fn engine_material(kind: GscnObjectKind, material: GscnMaterialFact) -> SceneMaterialContract {
    SceneMaterialContract {
        shader: material
            .shader
            .unwrap_or_else(|| default_shader_name(kind).to_owned()),
        blend: material
            .blend_code
            .map(we_material_blend_contract)
            .unwrap_or(SceneBlendContract::TranslucentAlpha),
        render_state: SceneMaterialRenderState {
            depth_test: material.depth_test,
            depth_write: material.depth_write,
            cull_mode: material.cull_mode,
        },
    }
}

fn default_shader_name(kind: GscnObjectKind) -> &'static str {
    match kind {
        GscnObjectKind::Video => "we/video",
        GscnObjectKind::Color => "we/color",
        GscnObjectKind::SolidShape => "we/color",
        GscnObjectKind::Text => "we/text",
        GscnObjectKind::Path => "we/path",
        GscnObjectKind::ParticleEmitter => "we/particle",
        GscnObjectKind::Image | GscnObjectKind::Generic => "we/genericimage4",
    }
}

fn we_material_blend_contract(code: u16) -> SceneBlendContract {
    match code {
        2 => SceneBlendContract::Additive,
        3 => SceneBlendContract::AlphaToCoverage,
        6 => SceneBlendContract::NormalReplace,
        8 => SceneBlendContract::ShaderColorBlend(28),
        _ => SceneBlendContract::TranslucentAlpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gscn_facts_lower_to_engine_owned_plan() {
        let plan = GscnSceneFacts {
            source: Some(PathBuf::from("/tmp/scene.gscn")),
            snapshot_time_ms: 250,
            target_width: 3840,
            target_height: 2160,
            counts: GscnSceneCounts {
                timeline_channel_count: 2,
                timeline_owner_count: 1,
                puppet_animation_layer_count: 3,
                particle_emitter_count: 4,
                material_pass_count: 5,
                effect_pass_count: 6,
            },
            resources: vec![
                GscnResourceFact {
                    id_name: Some(77),
                    source: Some(PathBuf::from("/tmp/albedo.gtex")),
                    width: Some(64),
                    height: Some(32),
                    format: Some(SceneTextureFormat::Bc7UnormBlock),
                    mip_count: Some(1),
                    payload_bytes: Some(8192),
                },
                GscnResourceFact {
                    id_name: None,
                    source: None,
                    width: None,
                    height: None,
                    format: None,
                    mip_count: None,
                    payload_bytes: None,
                },
            ],
            mesh_resources: vec![GscnMeshResourceFact {
                source_record: 12,
                vertices: vec![SceneMeshVertex::default(); 4],
                indices: vec![0, 1, 2, 2, 3, 0],
            }],
            puppet_resources: vec![GscnPuppetResourceFact {
                source_record: 3,
                skin: None,
                clips: vec![ScenePuppetAnimationClip {
                    id: 9,
                    name: None,
                    fps: 30.0,
                    frame_count: 1,
                    looping: true,
                    bones: Vec::new(),
                }],
                layers: vec![ScenePuppetAnimationLayer {
                    clip_id: 9,
                    name: None,
                    additive: false,
                    lock_transforms: false,
                    blend: 1.0,
                    visible: true,
                    rate: 1.0,
                    initial_phase: 0.0,
                }],
                clipping_records: Vec::new(),
            }],
            objects: vec![
                GscnObjectFact {
                    layer_index: 0,
                    kind: GscnObjectKind::Image,
                    geometry: GscnGeometryFact::Puppet {
                        geometry_record_index: 12,
                        puppet_record_index: 3,
                        vertex_count: 4,
                        index_count: 6,
                        bone_count: 2,
                        skin_vertex_count: 4,
                        clip_count: 1,
                        layer_count: 1,
                        clipping_record_count: 0,
                    },
                    material: GscnMaterialFact {
                        shader: None,
                        blend_code: Some(8),
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                    },
                    source_resource_index: Some(0),
                    effects: Vec::new(),
                },
                GscnObjectFact {
                    layer_index: 1,
                    kind: GscnObjectKind::SolidShape,
                    geometry: GscnGeometryFact::Quad,
                    material: GscnMaterialFact {
                        shader: None,
                        blend_code: Some(6),
                        depth_test: SceneDepthTest::Disabled,
                        depth_write: false,
                        cull_mode: SceneCullMode::None,
                    },
                    source_resource_index: Some(1),
                    effects: Vec::new(),
                },
            ],
        }
        .into_plan();

        assert_eq!(plan.resources.len(), 3);
        assert_eq!(plan.objects.len(), 2);
        assert_eq!(plan.effects.len(), 0);
        let SceneResource::Texture {
            id,
            format,
            mip_count,
            payload_bytes,
            ..
        } = &plan.resources[0]
        else {
            panic!("expected texture resource");
        };
        assert_eq!(*id, SceneResourceId(77));
        assert_eq!(*format, Some(SceneTextureFormat::Bc7UnormBlock));
        assert_eq!(*mip_count, Some(1));
        assert_eq!(*payload_bytes, Some(8192));
        let SceneResource::MeshGeometry {
            id,
            vertices,
            indices,
            ..
        } = &plan.resources[1]
        else {
            panic!("expected mesh geometry resource");
        };
        assert_eq!(*id, SceneGeometryId(0));
        assert_eq!(vertices.len(), 4);
        assert_eq!(indices.len(), 6);
        let SceneResource::PuppetRig {
            id, clips, layers, ..
        } = &plan.resources[2]
        else {
            panic!("expected puppet rig resource");
        };
        assert_eq!(*id, ScenePuppetId(0));
        assert_eq!(clips.len(), 1);
        assert_eq!(layers.len(), 1);
        assert_eq!(plan.objects[0].source, Some(SceneResourceId(77)));
        assert_eq!(
            plan.objects[0].geometry,
            SceneObjectGeometry::Puppet {
                geometry: SceneGeometryId(0),
                puppet: ScenePuppetId(0),
                vertex_count: 4,
                index_count: 6,
            }
        );
        assert_eq!(plan.objects[0].material.shader, "we/genericimage4");
        assert_eq!(
            plan.objects[0].material.blend,
            SceneBlendContract::ShaderColorBlend(28)
        );
        assert_eq!(plan.objects[1].source, None);
        assert_eq!(plan.objects[1].material.shader, "we/color");
        assert_eq!(
            plan.objects[1].material.blend,
            SceneBlendContract::NormalReplace
        );
        assert_eq!(plan.timeline_channel_count, 2);
        assert_eq!(plan.effect_pass_count, 6);
    }
}
