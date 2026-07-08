//! `.gscn` renderable topology extraction.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/particle-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use crate::core::SceneNodeKind;
use crate::core::scene::binary::{
    SCENE_BINARY_GEOMETRY_PRIMITIVE_PARTICLES, SCENE_BINARY_NONE_ID, SceneBinaryGeometryRecord,
    SceneBinaryMaterialPassRecord, SceneBinaryNodeRecord, SceneBinaryParticleEmitterRecord,
    SceneBinaryPuppetRecord,
};
use crate::renderer::RendererPlanError;

use super::facts::BinarySceneResource;
use super::reader::BinarySceneReader;
use super::render_layers::{binary_resource_by_name, binary_scene_material_texture_slots_cached};

#[derive(Debug, Clone)]
pub(super) struct BinarySceneRetainedTopology {
    pub(super) renderables: Vec<BinarySceneRetainedRenderable>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BinarySceneRetainedRenderable {
    pub(super) layer_index: usize,
    pub(super) node_index: usize,
    pub(super) node: SceneBinaryNodeRecord,
    pub(super) geometry: SceneBinaryGeometryRecord,
    pub(super) kind: SceneNodeKind,
    pub(super) material_index: u32,
    pub(super) material: Option<SceneBinaryMaterialPassRecord>,
    pub(super) particle: Option<SceneBinaryParticleEmitterRecord>,
    pub(super) puppet_record: Option<SceneBinaryPuppetRecord>,
}

impl BinarySceneRetainedRenderable {
    pub(super) fn is_particle(&self) -> bool {
        self.particle.is_some()
    }

    pub(super) fn render_layer_kind(&self) -> SceneNodeKind {
        if self.is_particle() {
            SceneNodeKind::Image
        } else {
            self.kind
        }
    }
}

pub(super) fn binary_scene_retained_topology(
    reader: &mut BinarySceneReader,
    resources: &[BinarySceneResource],
) -> Result<BinarySceneRetainedTopology, RendererPlanError> {
    let node_records = reader.node_records_cached()?;
    let mut renderables = Vec::new();
    for (node_index, node) in node_records.iter().copied().enumerate() {
        let Some(kind) = binary_scene_node_kind(node.kind) else {
            continue;
        };
        if !binary_scene_node_kind_is_renderable(kind)
            || node.geometry_index == SCENE_BINARY_NONE_ID
        {
            continue;
        }
        let geometry = reader.geometry_record_cached(node.geometry_index)?;
        let material = if node.material_index == SCENE_BINARY_NONE_ID {
            None
        } else {
            Some(reader.material_record_cached(node.material_index)?)
        };
        let particle = if binary_scene_node_is_particle_renderable(kind, geometry) {
            if node.particle_index == SCENE_BINARY_NONE_ID {
                None
            } else {
                let particle = reader.particle_record_cached(node.particle_index)?;
                if particle.particle_count() == 0
                    || !binary_scene_particle_has_base_texture(
                        reader,
                        resources,
                        node,
                        node.material_index,
                        material,
                    )?
                {
                    None
                } else {
                    Some(particle)
                }
            }
        } else {
            None
        };
        if binary_scene_node_is_particle_renderable(kind, geometry) && particle.is_none() {
            continue;
        }
        let puppet_record = if node.puppet_index == SCENE_BINARY_NONE_ID {
            None
        } else {
            Some(reader.puppet_record_cached(node.puppet_index)?)
        };
        renderables.push(BinarySceneRetainedRenderable {
            layer_index: renderables.len(),
            node_index,
            node,
            geometry,
            kind,
            material_index: node.material_index,
            material,
            particle,
            puppet_record,
        });
    }
    Ok(BinarySceneRetainedTopology { renderables })
}

fn binary_scene_node_is_particle_renderable(
    kind: SceneNodeKind,
    geometry: SceneBinaryGeometryRecord,
) -> bool {
    kind == SceneNodeKind::ParticleEmitter
        || geometry.primitive_kind == SCENE_BINARY_GEOMETRY_PRIMITIVE_PARTICLES
}

fn binary_scene_particle_has_base_texture(
    reader: &mut BinarySceneReader,
    resources: &[BinarySceneResource],
    node: SceneBinaryNodeRecord,
    material_index: u32,
    material: Option<SceneBinaryMaterialPassRecord>,
) -> Result<bool, RendererPlanError> {
    let node_resource = binary_resource_by_name(resources, node.resource_name);
    if node_resource.is_some_and(|resource| resource.source.is_some()) {
        return Ok(true);
    }
    let material_texture_slots = if let Some(material) = material {
        binary_scene_material_texture_slots_cached(reader, material_index, material, resources)?
    } else {
        Vec::new()
    };
    Ok(material_texture_slots
        .iter()
        .any(|slot| slot.slot == 0 && !slot.source.as_os_str().is_empty()))
}

fn binary_scene_node_kind(code: u16) -> Option<SceneNodeKind> {
    Some(match code {
        1 => SceneNodeKind::Image,
        2 => SceneNodeKind::Video,
        3 => SceneNodeKind::Color,
        4 => SceneNodeKind::Rectangle,
        5 => SceneNodeKind::Ellipse,
        6 => SceneNodeKind::Text,
        7 => SceneNodeKind::Path,
        10 => SceneNodeKind::ParticleEmitter,
        11 => SceneNodeKind::AudioResponse,
        _ => return None,
    })
}

fn binary_scene_node_kind_is_renderable(kind: SceneNodeKind) -> bool {
    matches!(
        kind,
        SceneNodeKind::Image
            | SceneNodeKind::Video
            | SceneNodeKind::Color
            | SceneNodeKind::Rectangle
            | SceneNodeKind::Ellipse
            | SceneNodeKind::Text
            | SceneNodeKind::Path
            | SceneNodeKind::ParticleEmitter
            | SceneNodeKind::AudioResponse
    )
}
