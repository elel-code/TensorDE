//! Legacy wallpaper layer assembly from binary `.gscn` records.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use crate::core::scene::binary::{
    SCENE_BINARY_NONE_ID, SceneBinaryGeometryRecord, SceneBinaryMaterialPassRecord,
    SceneBinaryNodeRecord,
};
use crate::core::{
    FitMode, SceneBlendMode, SceneNodeKind, ScenePathFillRule, SceneTextAlign, SceneTextureRegion,
};
use crate::renderer::{RendererPlanError, SceneRenderLayer};

use super::dynamic_state::BinarySceneDynamicState;
use super::facts::{BinarySceneNames, BinarySceneResource, binary_name};
use super::mesh::binary_scene_mesh;
use super::reader::BinarySceneReader;
use super::schema::{
    BINARY_NODE_FLAG_COLOR, BINARY_NODE_FLAG_STROKE_COLOR, BINARY_NODE_FLAG_STROKE_WIDTH,
};
use super::topology::BinarySceneRetainedTopology;

mod material;
mod node_state;
mod particle;

pub(in crate::renderer::scene_binary) use material::binary_scene_material_texture_slots_cached;

use material::{
    binary_scene_alpha_texture_mode, binary_scene_alpha_texture_slot,
    binary_scene_image_effect_passes_cached,
};
use node_state::{BinarySceneNodeState, binary_scene_effective_node_states};
use particle::binary_scene_particle_render_layer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinarySceneRenderLayerFilter {
    All,
}

impl BinarySceneRenderLayerFilter {
    fn allows_kind(self, _kind: SceneNodeKind) -> bool {
        match self {
            Self::All => true,
        }
    }
}

pub(super) fn binary_scene_render_layers(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    topology: &BinarySceneRetainedTopology,
    snapshot_time_ms: u64,
    dynamic_state: Option<&BinarySceneDynamicState>,
) -> Result<Vec<SceneRenderLayer>, RendererPlanError> {
    let mut layers = Vec::new();
    binary_scene_render_layers_into(
        reader,
        names,
        resources,
        topology,
        snapshot_time_ms,
        dynamic_state,
        BinarySceneRenderLayerFilter::All,
        &mut layers,
    )?;
    Ok(layers)
}

fn binary_scene_render_layers_into(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    topology: &BinarySceneRetainedTopology,
    snapshot_time_ms: u64,
    dynamic_state: Option<&BinarySceneDynamicState>,
    filter: BinarySceneRenderLayerFilter,
    layers: &mut Vec<SceneRenderLayer>,
) -> Result<(), RendererPlanError> {
    layers.clear();
    reader.puppet_attachment_delta_cache.clear();
    let node_states =
        binary_scene_effective_node_states(reader, names, snapshot_time_ms, dynamic_state, true)?;
    layers.reserve(topology.renderables.len());
    for renderable in &topology.renderables {
        if !filter.allows_kind(renderable.render_layer_kind()) {
            continue;
        }
        let Some(effective_state) = node_states.get(renderable.node_index) else {
            return Err(RendererPlanError::PackageLoad(format!(
                "binary scene retained topology node index {} is out of range",
                renderable.node_index
            )));
        };
        let mut node_state = effective_state.state;
        if !effective_state.visible {
            node_state.opacity = 0.0;
        }
        if let Some(particle) = renderable.particle {
            if let Some(layer) = binary_scene_particle_render_layer(
                reader,
                names,
                resources,
                renderable.node,
                particle,
                renderable.material_index,
                renderable.material,
                node_state,
            )? {
                layers.push(layer);
            }
            continue;
        }
        let layer = binary_scene_render_layer(
            reader,
            names,
            resources,
            renderable.node,
            renderable.geometry,
            renderable.material_index,
            renderable.material,
            renderable.kind,
            node_state,
        )?;
        layers.push(layer);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn binary_scene_render_layer(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    node: SceneBinaryNodeRecord,
    geometry: SceneBinaryGeometryRecord,
    material_index: u32,
    material: Option<SceneBinaryMaterialPassRecord>,
    kind: SceneNodeKind,
    node_state: BinarySceneNodeState,
) -> Result<SceneRenderLayer, RendererPlanError> {
    let material_texture_slots = if let Some(material) = material {
        binary_scene_material_texture_slots_cached(reader, material_index, material, resources)?
    } else {
        Vec::new()
    };
    let image_effect_passes = if let Some(material) = material {
        binary_scene_image_effect_passes_cached(reader, names, material_index, material, resources)?
    } else {
        Vec::new()
    };
    let node_resource = binary_resource_by_name(resources, node.resource_name);
    let source = node_resource
        .and_then(|resource| resource.source.clone())
        .or_else(|| {
            material_texture_slots
                .iter()
                .find(|slot| slot.slot == 0)
                .map(|slot| slot.source.clone())
        });
    let blend_mode = material
        .map(|material| binary_scene_blend_mode(material.blend_mode))
        .unwrap_or_default();
    let layer_id = binary_name(names, node.id_name)
        .unwrap_or("binary-node")
        .to_owned();
    Ok(SceneRenderLayer {
        id: layer_id.clone(),
        kind,
        source,
        texture_slots: material_texture_slots,
        alpha_texture_slot: material.and_then(binary_scene_alpha_texture_slot),
        alpha_texture_mode: material
            .map(binary_scene_alpha_texture_mode)
            .unwrap_or_default(),
        image_effect_passes,
        composite_key: None,
        texture_region: None::<SceneTextureRegion>,
        effect_motion: Default::default(),
        blend_mode,
        audio: Vec::new(),
        color: binary_scene_flagged_color(node.flags, BINARY_NODE_FLAG_COLOR, node.color_rgba),
        stroke_color: binary_scene_flagged_color(
            node.flags,
            BINARY_NODE_FLAG_STROKE_COLOR,
            node.stroke_color_rgba,
        ),
        stroke_width: (node.flags & BINARY_NODE_FLAG_STROKE_WIDTH != 0)
            .then_some(f64::from(node.stroke_width)),
        corner_radius: node_state.corner_radius,
        width: node_state.width,
        height: node_state.height,
        mesh: binary_scene_mesh(
            reader,
            names,
            node.geometry_index,
            geometry,
            node.puppet_index,
        )?,
        text: binary_name(names, node.text_name).map(str::to_owned),
        font_size: (node.font_size > 0.0).then_some(f64::from(node.font_size)),
        font_family: binary_name(names, node.font_family_name).map(str::to_owned),
        font_source: binary_resource_by_name(resources, node.font_resource_name)
            .and_then(|resource| resource.source.clone()),
        font_weight: binary_name(names, node.font_weight_name).map(str::to_owned),
        text_align: binary_scene_text_align(node.text_align),
        path_data: None,
        path_fill_rule: ScenePathFillRule::default(),
        fit: binary_scene_fit(node.fit),
        opacity: node_state.opacity,
        transform: node_state.transform,
    })
}

pub(super) fn binary_resource_by_name(
    resources: &[BinarySceneResource],
    id_name: u32,
) -> Option<&BinarySceneResource> {
    (id_name != SCENE_BINARY_NONE_ID)
        .then(|| {
            resources
                .iter()
                .find(|resource| resource.id_name == id_name)
        })
        .flatten()
}

fn binary_scene_blend_mode(code: u16) -> SceneBlendMode {
    match code {
        2 => SceneBlendMode::Additive,
        3 => SceneBlendMode::Multiply,
        4 => SceneBlendMode::Screen,
        5 => SceneBlendMode::Max,
        6 => SceneBlendMode::Normal,
        7 => SceneBlendMode::Modulate,
        8 => SceneBlendMode::HslColor,
        9 => SceneBlendMode::AlphaToCoverage,
        _ => SceneBlendMode::Alpha,
    }
}

fn binary_scene_fit(code: u16) -> FitMode {
    match code {
        2 => FitMode::Contain,
        3 => FitMode::Stretch,
        4 => FitMode::Tile,
        5 => FitMode::Center,
        _ => FitMode::Cover,
    }
}

fn binary_scene_text_align(code: u16) -> Option<SceneTextAlign> {
    match code {
        2 => Some(SceneTextAlign::Middle),
        3 => Some(SceneTextAlign::End),
        1 => Some(SceneTextAlign::Start),
        _ => None,
    }
}

fn binary_scene_flagged_color(flags: u16, flag: u16, rgba: u32) -> Option<String> {
    (flags & flag != 0).then(|| binary_scene_rgba_hex(rgba))
}

fn binary_scene_rgba_hex(rgba: u32) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        rgba >> 24,
        (rgba >> 16) & 0xff,
        (rgba >> 8) & 0xff
    )
}
