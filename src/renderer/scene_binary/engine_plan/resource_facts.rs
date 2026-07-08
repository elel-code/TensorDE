//! `.gscn` resource fact lowering for the scene engine ingest boundary.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `references/godot/servers/rendering/storage/`

use std::collections::BTreeSet;

use crate::core::scene::binary::SCENE_BINARY_NONE_ID;
use crate::engine::scene_engine::SceneObjectId;
use crate::engine::scene_engine::ingest::gscn::{
    GscnLayerAlphaMaskRtMethod8MdlvGeometryFact, GscnMeshResourceFact, GscnPuppetResourceFact,
    GscnResourceFact,
};
use crate::renderer::RendererPlanError;

use super::super::facts::{BinarySceneNames, BinarySceneResource, binary_name};
use super::super::mdlv::binary_scene_mdlv_first_entry_geometry;
use super::super::mesh::{
    binary_scene_geometry_is_mesh_payload, binary_scene_mesh_vertices_indices,
    binary_scene_puppet_active_sources, binary_scene_puppet_clipping_records,
    binary_scene_puppet_clips, binary_scene_puppet_layers, binary_scene_puppet_skin,
};
use super::super::reader::BinarySceneReader;
use super::super::topology::BinarySceneRetainedTopology;

pub(super) fn gscn_resources(resources: &[BinarySceneResource]) -> Vec<GscnResourceFact> {
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

pub(super) fn gscn_mesh_and_puppet_resources(
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

pub(super) fn gscn_layer_alpha_mask_rt_method8_mdlv_geometries(
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    topology: &BinarySceneRetainedTopology,
) -> Result<Vec<GscnLayerAlphaMaskRtMethod8MdlvGeometryFact>, RendererPlanError> {
    let mut geometries = Vec::new();
    for renderable in &topology.renderables {
        let Some(puppet) = renderable.puppet_record else {
            continue;
        };
        if puppet.clipping_record_count == 0 {
            continue;
        }
        let Some(puppet_source) = binary_name(names, renderable.node.puppet_source_name) else {
            continue;
        };
        let Some(resource) = resources
            .iter()
            .find(|resource| binary_scene_resource_matches_puppet_source(resource, puppet_source))
        else {
            return Err(RendererPlanError::PackageLoad(format!(
                "binary scene object {} references puppet source {puppet_source:?} but no we-puppet-mdl resource matches it",
                renderable.layer_index
            )));
        };
        let Some(source) = &resource.source else {
            continue;
        };
        let Some(geometry) = binary_scene_mdlv_first_entry_geometry(source)? else {
            continue;
        };
        geometries.push(GscnLayerAlphaMaskRtMethod8MdlvGeometryFact {
            object: SceneObjectId(renderable.layer_index.min(u32::MAX as usize) as u32),
            entry_owner_index: geometry.entry_owner_index,
            layout_key: geometry.layout_key,
            vertex_stride_bytes: geometry.vertex_stride_bytes,
            vertex_count: geometry.vertex_count,
            index_count: geometry.index_count,
            vertex_payload: geometry.vertex_payload,
            index_payload: geometry.index_payload,
            source_records: geometry.source_records,
            subdraws: geometry.subdraws,
        });
    }
    Ok(geometries)
}

fn binary_scene_resource_matches_puppet_source(
    resource: &BinarySceneResource,
    puppet_source: &str,
) -> bool {
    if resource.kind != 5 || resource.role.as_deref() != Some("we-puppet-mdl") {
        return false;
    }
    let puppet_source = binary_scene_normalized_source_suffix(puppet_source);
    resource
        .original_source
        .as_ref()
        .or(resource.source.as_ref())
        .is_some_and(|source| {
            binary_scene_normalized_source_suffix(&source.to_string_lossy())
                .ends_with(&puppet_source)
        })
}

fn binary_scene_normalized_source_suffix(source: &str) -> String {
    source
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_owned()
}
