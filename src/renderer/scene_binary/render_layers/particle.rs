//! Particle render-layer assembly for legacy `.gscn` wallpaper plans.
//!
//! References:
//! - `reverse-engineered/docs/particle-format.md`
//! - `reverse-engineered/docs/scene-format.md`

use crate::core::scene::binary::{
    SceneBinaryMaterialPassRecord, SceneBinaryNodeRecord, SceneBinaryParticleEmitterRecord,
};
use crate::core::{SceneNodeKind, ScenePathFillRule, SceneTextureRegion};
use crate::renderer::{
    RendererPlanError, SceneRenderAlphaTextureMode, SceneRenderLayer, SceneRenderTextureSlot,
};

use super::super::facts::{BinarySceneNames, BinarySceneResource, binary_name};
use super::super::reader::BinarySceneReader;
use super::material::binary_scene_material_texture_slots_cached;
use super::node_state::BinarySceneNodeState;
use super::{
    binary_resource_by_name, binary_scene_blend_mode, binary_scene_fit, binary_scene_rgba_hex,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn binary_scene_particle_render_layer(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    node: SceneBinaryNodeRecord,
    particle: SceneBinaryParticleEmitterRecord,
    material_index: u32,
    material: Option<SceneBinaryMaterialPassRecord>,
    node_state: BinarySceneNodeState,
) -> Result<Option<SceneRenderLayer>, RendererPlanError> {
    let particle_count = particle.particle_count();
    if particle_count == 0 {
        return Ok(None);
    }

    let node_resource = binary_resource_by_name(resources, node.resource_name);
    let material_texture_slots = if let Some(material) = material {
        let slots = binary_scene_material_texture_slots_cached(
            reader,
            material_index,
            material,
            resources,
        )?;
        if slots.is_empty() {
            binary_scene_particle_base_texture_slot(node_resource)
        } else {
            slots
        }
    } else {
        binary_scene_particle_base_texture_slot(node_resource)
    };
    let source = node_resource
        .and_then(|resource| resource.source.clone())
        .or_else(|| {
            material_texture_slots
                .iter()
                .find(|slot| slot.slot == 0)
                .map(|slot| slot.source.clone())
        });
    let Some(source) = source else {
        return Ok(None);
    };
    let particle_width = f64::from(particle.particle_width);
    let particle_height = f64::from(particle.particle_height);
    if !particle_width.is_finite()
        || !particle_height.is_finite()
        || particle_width <= 0.0
        || particle_height <= 0.0
    {
        return Ok(None);
    }
    // The particle base image is already represented by SceneRenderLayer::source.
    // Retaining slot 0 here clones thousands of identical paths per frame.
    let texture_slots = material_texture_slots
        .iter()
        .filter(|slot| slot.slot != 0)
        .cloned()
        .collect::<Vec<_>>();
    let blend_mode = material
        .map(|material| binary_scene_blend_mode(material.blend_mode))
        .unwrap_or_default();
    let mut transform = node_state.transform;
    transform.anchor_x = 0.5;
    transform.anchor_y = 0.5;
    Ok(Some(SceneRenderLayer {
        id: binary_name(names, node.id_name)
            .unwrap_or("binary-particle-emitter")
            .to_owned(),
        kind: SceneNodeKind::Image,
        source: Some(source),
        texture_slots,
        alpha_texture_slot: None,
        alpha_texture_mode: SceneRenderAlphaTextureMode::Multiply,
        image_effect_passes: Vec::new(),
        composite_key: None,
        texture_region: None::<SceneTextureRegion>,
        effect_motion: Default::default(),
        blend_mode,
        audio: Vec::new(),
        color: Some(binary_scene_rgba_hex(particle.color_rgba)),
        stroke_color: None,
        stroke_width: None,
        corner_radius: None,
        width: Some(particle_width.max(1.0)),
        height: Some(particle_height.max(1.0)),
        mesh: None,
        text: None,
        font_size: None,
        font_family: None,
        font_source: None,
        font_weight: None,
        text_align: None,
        path_data: None,
        path_fill_rule: ScenePathFillRule::default(),
        fit: binary_scene_fit(node.fit),
        opacity: node_state.opacity.clamp(0.0, 1.0),
        transform,
    }))
}

fn binary_scene_particle_base_texture_slot(
    resource: Option<&BinarySceneResource>,
) -> Vec<SceneRenderTextureSlot> {
    let Some(resource) = resource else {
        return Vec::new();
    };
    let Some(source) = resource.source.clone() else {
        return Vec::new();
    };
    vec![SceneRenderTextureSlot {
        slot: 0,
        source,
        width: resource.width,
        height: resource.height,
    }]
}
