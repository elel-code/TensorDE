//! Wallpaper Engine scene ingest and IR lowering.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/project-format.md`
//! - `reverse-engineered/gilder/docs/scene-pkg-format.md`
//! - `reverse-engineered/gilder/docs/scene-format.md`
//! - `reverse-engineered/gilder/docs/tex-format.md`
//! - `reverse-engineered/gilder/docs/material-format.md`
//! - `reverse-engineered/gilder/docs/effect-format.md`
//! - `reverse-engineered/gilder/docs/mdl-format.md`
//! - `references/gilder/godot/servers/rendering/storage/*`
//! - `references/gilder/godot/servers/rendering/rendering_device_graph.*`

use std::fmt;
use std::fs::File;
use std::path::Path;

use crate::engine::scene::{RenderingServer, SceneStorage, write_scene_binary};

mod ingest;
pub mod ir;
mod lower;
mod mdl;
mod pkg;
mod script_analysis;
mod shader_key;
mod shader_origin;
mod tex;

pub use ingest::{WeIngestError, ingest_wallpaper_engine_project};
pub use lower::{WeLowerError, lower_ir_to_scene_binary};
pub use mdl::{MdlMeshEntry, MdlMeshVertex, MdlModel, MdlParseError, parse_mdl_model};
pub use pkg::{ScenePackage, ScenePackageEntry, ScenePackageError};
pub use tex::{
    TexMetadata, TexParseError, TexUpload, TexUploadMip, decode_tex_upload, parse_tex_metadata,
};

pub fn convert_wallpaper_engine_project_to_scene_binary(
    project_root: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<WeConvertSummary, WeConvertError> {
    let project_root = project_root.as_ref();
    let mut ir = ingest_wallpaper_engine_project(project_root)?;
    ingest::compile_authored_shader_programs(project_root, &mut ir)?;
    let binary = lower_ir_to_scene_binary(&ir)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| WeConvertError::CreateOutputDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let mut output = File::create(output_path).map_err(|source| WeConvertError::CreateOutput {
        path: output_path.to_path_buf(),
        source,
    })?;
    write_scene_binary(&binary, &mut output)?;
    let storage = SceneStorage::from_document(binary)?;
    let render_plan = RenderingServer::new(&storage).renderer_scene_render_plan();
    Ok(WeConvertSummary {
        output_path: output_path.to_path_buf(),
        object_count: render_plan.object_count,
        resource_count: render_plan.resource_count,
        material_count: render_plan.material_count,
        effect_count: render_plan.effect_count,
        mesh_count: render_plan.mesh_count,
        mesh_vertex_count: render_plan.mesh_vertex_count,
        mesh_index_count: render_plan.mesh_index_count,
        mesh_source_record_count: storage.document().mesh_source_records.len(),
        mesh_clipping_subdraw_count: storage.document().mesh_clipping_subdraws.len(),
        mesh_clipping_slice_count: storage.document().mesh_clipping_slices.len(),
        puppet_count: storage.puppets().len(),
        puppet_bone_count: storage.document().puppet_bones.len(),
        puppet_animation_clip_count: storage.puppet_animation_clips().len(),
        puppet_animation_track_count: storage.document().puppet_animation_tracks.len(),
        puppet_animation_transform_sample_count: storage
            .document()
            .puppet_animation_transform_samples
            .len(),
        puppet_animation_opacity_sample_count: storage
            .document()
            .puppet_animation_opacity_samples
            .len(),
        object_transform_track_count: storage.object_transform_tracks().len(),
        object_transform_channel_count: storage.document().object_transform_channels.len(),
        object_transform_keyframe_count: storage.document().object_transform_keyframes.len(),
        script_program_count: storage.script_programs().len(),
        render_graph_count: render_plan.render_graph_count,
        shader_contract_count: render_plan.shader_contract_count,
        descriptor_heap_resource_count: render_plan.descriptor_heap_resource_count,
        descriptor_heap_sampler_count: render_plan.descriptor_heap_sampler_count,
        fifo_latest_ready_present_required: render_plan.fifo_latest_ready_present_required,
        resource_payload_bytes: render_plan.resource_payload_bytes,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeConvertSummary {
    pub output_path: std::path::PathBuf,
    pub object_count: usize,
    pub resource_count: usize,
    pub material_count: usize,
    pub effect_count: usize,
    pub mesh_count: usize,
    pub mesh_vertex_count: usize,
    pub mesh_index_count: usize,
    pub mesh_source_record_count: usize,
    pub mesh_clipping_subdraw_count: usize,
    pub mesh_clipping_slice_count: usize,
    pub puppet_count: usize,
    pub puppet_bone_count: usize,
    pub puppet_animation_clip_count: usize,
    pub puppet_animation_track_count: usize,
    pub puppet_animation_transform_sample_count: usize,
    pub puppet_animation_opacity_sample_count: usize,
    pub object_transform_track_count: usize,
    pub object_transform_channel_count: usize,
    pub object_transform_keyframe_count: usize,
    pub script_program_count: usize,
    pub render_graph_count: usize,
    pub shader_contract_count: usize,
    pub descriptor_heap_resource_count: u32,
    pub descriptor_heap_sampler_count: u32,
    pub fifo_latest_ready_present_required: bool,
    pub resource_payload_bytes: usize,
}

#[derive(Debug)]
pub enum WeConvertError {
    Ingest(WeIngestError),
    Lower(WeLowerError),
    Binary(crate::engine::scene::SceneBinaryError),
    Storage(crate::engine::scene::SceneStorageError),
    CreateOutputDir {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    CreateOutput {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for WeConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ingest(err) => write!(f, "{err}"),
            Self::Lower(err) => write!(f, "{err}"),
            Self::Binary(err) => write!(f, "{err}"),
            Self::Storage(err) => write!(f, "{err}"),
            Self::CreateOutputDir { path, source } => {
                write!(
                    f,
                    "failed to create output directory {}: {source}",
                    path.display()
                )
            }
            Self::CreateOutput { path, source } => {
                write!(
                    f,
                    "failed to create output file {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for WeConvertError {}

impl From<WeIngestError> for WeConvertError {
    fn from(value: WeIngestError) -> Self {
        Self::Ingest(value)
    }
}

impl From<WeLowerError> for WeConvertError {
    fn from(value: WeLowerError) -> Self {
        Self::Lower(value)
    }
}

impl From<crate::engine::scene::SceneBinaryError> for WeConvertError {
    fn from(value: crate::engine::scene::SceneBinaryError) -> Self {
        Self::Binary(value)
    }
}

impl From<crate::engine::scene::SceneStorageError> for WeConvertError {
    fn from(value: crate::engine::scene::SceneStorageError) -> Self {
        Self::Storage(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn converts_wallpaper_engine_project_to_scene_binary_with_ir_mesh_summary() {
        let root =
            std::env::temp_dir().join(format!("gilder-we-convert-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("models")).expect("models");
        fs::create_dir_all(root.join("materials")).expect("materials");
        fs::write(
            root.join("project.json"),
            r#"{"type":"scene","file":"scene.json","title":"Demo"}"#,
        )
        .expect("project");
        fs::write(
            root.join("scene.json"),
            r#"{"general":{"orthogonalprojection":{"width":1920,"height":1080}},"objects":[{"id":7,"name":"layer","image":"models/layer.json"}]}"#,
        )
        .expect("scene");
        fs::write(
            root.join("models/layer.json"),
            r#"{"width":64,"height":64,"material":"materials/layer.json"}"#,
        )
        .expect("model");
        fs::write(
            root.join("materials/layer.json"),
            r#"{"passes":[{"shader":"genericimage4","textures":[null]}]}"#,
        )
        .expect("material");
        let output = root.join("out.gscene");

        let summary =
            convert_wallpaper_engine_project_to_scene_binary(&root, &output).expect("convert");

        assert_eq!(summary.object_count, 1);
        assert_eq!(summary.mesh_count, 1);
        assert_eq!(summary.mesh_vertex_count, 4);
        assert_eq!(summary.mesh_index_count, 6);
        assert_eq!(summary.puppet_count, 0);
        assert_eq!(summary.puppet_animation_clip_count, 0);
        assert_eq!(summary.shader_contract_count, 1);
        assert_eq!(summary.descriptor_heap_resource_count, 3);
        assert_eq!(summary.descriptor_heap_sampler_count, 1);
        assert!(summary.fifo_latest_ready_present_required);
        assert!(output.is_file());

        let _ = fs::remove_dir_all(root);
    }
}
