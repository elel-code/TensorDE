//! Wallpaper Engine project ingest into scene IR.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/project-format.md`
//! - `reverse-engineered/docs/scene-pkg-format.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/tex-format.md`

mod animation_layer;
mod asset_source;
mod builtin_effect_texture;
mod effect_target;
mod final_effect;
mod foliage_ripple;
mod image_plane;
mod json_value;
mod material_instance;
mod pipeline_state;
mod puppet_clipping;
mod puppet_material;
mod puppet_model;
mod ripple_flow;
mod shader_combo;
mod shader_contract;
mod text_layer;
mod texture_resolver;
mod transform_animation;
mod utility_layer;
mod waterwaves_displacement;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[cfg(test)]
use std::fs;

use serde_json::Value;

use crate::engine::render_graph::{WeEffectPassContract, WeImageGraphContract, we_image_graph};
use crate::engine::scene::abi::{
    SceneCullMode, SceneDepthTest, SceneObjectKind as SceneAbiObjectKind, SceneResourceKind,
    SceneVec3,
};

use super::ir::*;
use super::mdl::parse_mdl_model;
use super::pkg::ScenePackageError;
use super::tex::{
    TexParseError, block_compression::transcode_texture_upload, decode_tex_upload,
    texture_alpha_coverage_rows,
};
use animation_layer::animation_layer_initial_progress;
use asset_source::WeAssetSource;
use builtin_effect_texture::apply_builtin_effect_texture_defaults;
use effect_target::{image_target_role, scale_divisor_to_milli};
use image_plane::image_plane_extent;
use json_value::{
    bound_bool, bound_string, compact_json, infer_project_type, non_empty_string,
    normalize_we_path, parse_color4, parse_json_bytes, parse_vec3, value_f32, value_i32, value_i64,
    value_u32,
};
use material_instance::{
    effect_shader_variant_key, file_texture_bindings, material_pass_constant_names,
    material_texture_bindings, merged_material_constants, push_instance_combo_overrides,
    push_instance_texture_overrides,
};
use pipeline_state::{
    cull_mode_from_we, depth_test_from_we, pipeline_blend_from_we, pipeline_blend_string,
    scene_blend_from_color_blend_mode,
};
use shader_combo::parse_shader_combo_definitions;
use shader_contract::build_shader_contract_records;
use text_layer::{
    ingest_text_layer, retained_text_effect_is_supported,
    retained_text_effect_requires_dependency_composite, text_layer_value,
};
use texture_resolver::texture_candidates;
use transform_animation::ingest_object_transform_tracks;
use utility_layer::{FULL_FRAMEBUFFER_TARGET, is_runtime_render_target, utility_layer_kind};

pub fn ingest_wallpaper_engine_project(
    project_root: impl AsRef<Path>,
) -> Result<WeSceneIr, WeIngestError> {
    let project_root = project_root.as_ref().to_path_buf();
    let source = WeAssetSource::open(project_root.clone())?;
    let project_asset = source.read_required_asset("project.json")?;
    let project_json = parse_json_bytes("project.json", &project_asset.bytes)?;
    let project = parse_project_ir(&project_json)?;
    if project.wallpaper_type != "scene" {
        return Err(WeIngestError::UnsupportedProjectType {
            wallpaper_type: project.wallpaper_type,
        });
    }

    let scene_asset = source.read_required_asset(&project.scene_file)?;
    let scene_json = parse_json_bytes(&project.scene_file, &scene_asset.bytes)?;
    let scene = parse_scene_root_ir(&scene_json);

    let mut builder = WeIrBuilder::new(project_root, source, project, scene);
    builder.add_existing_resource(
        "project.json",
        SceneResourceKind::ProjectJson,
        project_asset.source,
        project_asset.bytes,
    );
    builder.add_existing_resource(
        builder.project.scene_file.clone(),
        SceneResourceKind::SceneJson,
        scene_asset.source,
        scene_asset.bytes,
    );

    for (index, object) in scene_json
        .get("objects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        builder.ingest_object(index, object)?;
    }

    builder.finish()
}

#[derive(Debug)]
pub enum WeIngestError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Package(ScenePackageError),
    Json {
        path: String,
        source: serde_json::Error,
    },
    Tex {
        path: String,
        source: TexParseError,
    },
    MissingAsset(String),
    UnsafePath(String),
    UnsupportedProjectType {
        wallpaper_type: String,
    },
    InvalidProject(String),
}

impl fmt::Display for WeIngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "failed to read {}: {source}", path.display()),
            Self::Package(err) => write!(f, "{err}"),
            Self::Json { path, source } => write!(f, "failed to parse WE JSON {path}: {source}"),
            Self::Tex { path, source } => write!(f, "failed to parse WE texture {path}: {source}"),
            Self::MissingAsset(path) => write!(f, "missing WE asset {path}"),
            Self::UnsafePath(path) => write!(f, "unsafe WE asset path {path}"),
            Self::UnsupportedProjectType { wallpaper_type } => {
                write!(
                    f,
                    "Wallpaper Engine type {wallpaper_type:?} is not a scene wallpaper"
                )
            }
            Self::InvalidProject(message) => write!(f, "invalid WE project: {message}"),
        }
    }
}

impl std::error::Error for WeIngestError {}

impl From<ScenePackageError> for WeIngestError {
    fn from(value: ScenePackageError) -> Self {
        Self::Package(value)
    }
}

fn parse_project_ir(project: &Value) -> Result<WeProjectIr, WeIngestError> {
    let scene_file = project
        .get("file")
        .and_then(Value::as_str)
        .map(normalize_we_path)
        .unwrap_or_else(|| "scene.json".to_owned());
    let wallpaper_type = project
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_else(|| infer_project_type(&scene_file))
        .to_ascii_lowercase();
    let properties_json = project
        .pointer("/general/properties")
        .map(compact_json)
        .unwrap_or_else(|| "{}".to_owned());
    Ok(WeProjectIr {
        title: project
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        wallpaper_type,
        scene_file,
        preview: project
            .get("preview")
            .and_then(Value::as_str)
            .map(normalize_we_path)
            .unwrap_or_default(),
        properties_json,
    })
}

fn parse_scene_root_ir(scene: &Value) -> WeSceneRootIr {
    let general = scene.get("general").unwrap_or(&Value::Null);
    let camera = scene.get("camera").unwrap_or(&Value::Null);
    let projection = general.get("orthogonalprojection").unwrap_or(&Value::Null);
    WeSceneRootIr {
        logical_width: value_u32(projection.get("width")).unwrap_or(0),
        logical_height: value_u32(projection.get("height")).unwrap_or(0),
        clear_color: parse_color4(general.get("clearcolor"), [0.0, 0.0, 0.0, 1.0]),
        ambient_color: parse_color4(general.get("ambientcolor"), [0.3, 0.3, 0.3, 1.0]),
        skylight_color: parse_color4(general.get("skylightcolor"), [0.3, 0.3, 0.3, 1.0]),
        camera_eye: parse_vec3(camera.get("eye")).unwrap_or_default(),
        camera_center: parse_vec3(camera.get("center")).unwrap_or(SceneVec3 {
            x: 0.0,
            y: 0.0,
            z: -1.0,
        }),
        camera_up: parse_vec3(camera.get("up")).unwrap_or(SceneVec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        }),
    }
}

struct WeIrBuilder {
    project_root: PathBuf,
    source: WeAssetSource,
    project: WeProjectIr,
    scene: WeSceneRootIr,
    resources: Vec<WeIrResource>,
    resource_by_path: BTreeMap<String, u32>,
    textures: Vec<WeIrTexture>,
    texture_by_path: BTreeMap<String, u32>,
    objects: Vec<WeIrObject>,
    object_effects: Vec<WeIrObjectEffect>,
    object_animation_layers: Vec<WeIrObjectAnimationLayer>,
    object_transform_tracks: Vec<WeIrObjectTransformTrack>,
    object_transform_channels: Vec<WeIrObjectTransformChannel>,
    object_transform_keyframes: Vec<WeIrObjectTransformKeyframe>,
    puppet_animation_clips: Vec<WeIrPuppetAnimationClip>,
    puppet_animation_tracks: Vec<WeIrPuppetAnimationTrack>,
    puppet_animation_transform_samples: Vec<WeIrPuppetAnimationTransformSample>,
    puppet_animation_opacity_samples: Vec<f32>,
    materials: Vec<WeIrMaterial>,
    material_by_path: BTreeMap<String, u32>,
    puppet_material_by_base: BTreeMap<u32, u32>,
    material_passes: Vec<WeIrMaterialPass>,
    material_textures: Vec<WeIrMaterialTexture>,
    material_constants: Vec<WeIrMaterialConstant>,
    meshes: Vec<WeIrMesh>,
    mesh_vertices: Vec<WeIrMeshVertex>,
    mesh_indices: Vec<u32>,
    mesh_source_records: Vec<WeIrMeshSourceRecord>,
    mesh_clipping_subdraws: Vec<WeIrMeshClippingSubdraw>,
    mesh_clipping_source_ordinals: Vec<u32>,
    mesh_clipping_slices: Vec<WeIrMeshClippingSlice>,
    puppets: Vec<WeIrPuppet>,
    puppet_bones: Vec<WeIrPuppetBone>,
    puppet_attachments: Vec<WeIrPuppetAttachment>,
    effects: Vec<WeIrEffect>,
    effect_by_path: BTreeMap<String, u32>,
    effect_passes: Vec<WeIrEffectPass>,
    effect_bindings: Vec<WeIrEffectBinding>,
    effect_combos: Vec<WeIrEffectCombo>,
    shader_combo_definitions: Vec<WeIrShaderComboDefinition>,
    shader_combo_defaults_by_shader: BTreeMap<String, BTreeMap<String, i64>>,
    effect_fbos: Vec<WeIrEffectFbo>,
    render_graphs: Vec<crate::engine::render_graph::RenderGraph>,
    image_targets: Vec<WeIrImageTarget>,
    shader_contracts: Vec<WeIrShaderContract>,
    unsupported: Vec<WeIrUnsupported>,
}

impl WeIrBuilder {
    fn new(
        project_root: PathBuf,
        source: WeAssetSource,
        project: WeProjectIr,
        scene: WeSceneRootIr,
    ) -> Self {
        Self {
            project_root,
            source,
            project,
            scene,
            resources: Vec::new(),
            resource_by_path: BTreeMap::new(),
            textures: Vec::new(),
            texture_by_path: BTreeMap::new(),
            objects: Vec::new(),
            object_effects: Vec::new(),
            object_animation_layers: Vec::new(),
            object_transform_tracks: Vec::new(),
            object_transform_channels: Vec::new(),
            object_transform_keyframes: Vec::new(),
            puppet_animation_clips: Vec::new(),
            puppet_animation_tracks: Vec::new(),
            puppet_animation_transform_samples: Vec::new(),
            puppet_animation_opacity_samples: Vec::new(),
            materials: Vec::new(),
            material_by_path: BTreeMap::new(),
            puppet_material_by_base: BTreeMap::new(),
            material_passes: Vec::new(),
            material_textures: Vec::new(),
            material_constants: Vec::new(),
            meshes: Vec::new(),
            mesh_vertices: Vec::new(),
            mesh_indices: Vec::new(),
            mesh_source_records: Vec::new(),
            mesh_clipping_subdraws: Vec::new(),
            mesh_clipping_source_ordinals: Vec::new(),
            mesh_clipping_slices: Vec::new(),
            puppets: Vec::new(),
            puppet_bones: Vec::new(),
            puppet_attachments: Vec::new(),
            effects: Vec::new(),
            effect_by_path: BTreeMap::new(),
            effect_passes: Vec::new(),
            effect_bindings: Vec::new(),
            effect_combos: Vec::new(),
            shader_combo_definitions: Vec::new(),
            shader_combo_defaults_by_shader: BTreeMap::new(),
            effect_fbos: Vec::new(),
            render_graphs: Vec::new(),
            image_targets: Vec::new(),
            shader_contracts: Vec::new(),
            unsupported: Vec::new(),
        }
    }

    fn finish(mut self) -> Result<WeSceneIr, WeIngestError> {
        self.build_shader_contracts();
        Ok(WeSceneIr {
            project_root: self.project_root,
            project: self.project,
            scene: self.scene,
            resources: self.resources,
            textures: self.textures,
            objects: self.objects,
            object_effects: self.object_effects,
            object_animation_layers: self.object_animation_layers,
            object_transform_tracks: self.object_transform_tracks,
            object_transform_channels: self.object_transform_channels,
            object_transform_keyframes: self.object_transform_keyframes,
            puppet_animation_clips: self.puppet_animation_clips,
            puppet_animation_tracks: self.puppet_animation_tracks,
            puppet_animation_transform_samples: self.puppet_animation_transform_samples,
            puppet_animation_opacity_samples: self.puppet_animation_opacity_samples,
            materials: self.materials,
            material_passes: self.material_passes,
            material_textures: self.material_textures,
            material_constants: self.material_constants,
            meshes: self.meshes,
            mesh_vertices: self.mesh_vertices,
            mesh_indices: self.mesh_indices,
            mesh_source_records: self.mesh_source_records,
            mesh_clipping_subdraws: self.mesh_clipping_subdraws,
            mesh_clipping_source_ordinals: self.mesh_clipping_source_ordinals,
            mesh_clipping_slices: self.mesh_clipping_slices,
            puppets: self.puppets,
            puppet_bones: self.puppet_bones,
            puppet_attachments: self.puppet_attachments,
            effects: self.effects,
            effect_passes: self.effect_passes,
            effect_bindings: self.effect_bindings,
            effect_combos: self.effect_combos,
            shader_combo_definitions: self.shader_combo_definitions,
            effect_fbos: self.effect_fbos,
            render_graphs: self.render_graphs,
            image_targets: self.image_targets,
            shader_contracts: self.shader_contracts,
            unsupported: self.unsupported,
        })
    }

    fn add_existing_resource(
        &mut self,
        path: impl Into<String>,
        kind: SceneResourceKind,
        source: WeIrResourceSource,
        payload: Vec<u8>,
    ) -> u32 {
        let path = normalize_we_path(&path.into());
        if let Some(handle) = self.resource_by_path.get(&path) {
            return *handle;
        }
        let handle = self.resources.len() as u32;
        self.resources.push(WeIrResource {
            handle,
            kind,
            path: path.clone(),
            source,
            payload,
        });
        self.resource_by_path.insert(path, handle);
        handle
    }

    fn add_required_resource(
        &mut self,
        path: &str,
        kind: SceneResourceKind,
    ) -> Result<u32, WeIngestError> {
        let path = normalize_we_path(path);
        if let Some(handle) = self.resource_by_path.get(&path) {
            return Ok(*handle);
        }
        let asset = self.source.read_required_asset(&path)?;
        Ok(self.add_existing_resource(path, kind, asset.source, asset.bytes))
    }

    fn add_optional_resource(
        &mut self,
        path: &str,
        kind: SceneResourceKind,
    ) -> Result<Option<u32>, WeIngestError> {
        let path = normalize_we_path(path);
        if let Some(handle) = self.resource_by_path.get(&path) {
            return Ok(Some(*handle));
        }
        let Some(asset) = self.source.read_optional_asset(&path)? else {
            return Ok(None);
        };
        Ok(Some(self.add_existing_resource(
            path,
            kind,
            asset.source,
            asset.bytes,
        )))
    }

    fn ingest_object(&mut self, index: usize, value: &Value) -> Result<(), WeIngestError> {
        let handle = self.objects.len() as u32;
        let we_id = value_u32(value.get("id")).unwrap_or(handle);
        let name = bound_string(value.get("name")).unwrap_or_default();
        let image_path = bound_string(value.get("image"))
            .or_else(|| bound_string(value.get("model")))
            .unwrap_or_default();
        let text_value = text_layer_value(value);
        let utility_layer = utility_layer_kind(&image_path);
        let mut resource = None;
        let mut material = None;
        let mut kind = SceneAbiObjectKind::Unsupported;

        if let Some(text) = text_value.as_deref() {
            kind = SceneAbiObjectKind::Text;
            match ingest_text_layer(self, handle, value, text)? {
                Some((font_resource, text_material)) => {
                    resource = Some(font_resource);
                    material = Some(text_material);
                }
                None => {
                    self.unsupported.push(WeIrUnsupported {
                        object: Some(handle),
                        pass_index: None,
                        feature: "text-layer-retained-fallback-unavailable".to_owned(),
                        expected_subsystem: "convert/we_ingest text glyph lowering".to_owned(),
                        containment: "text-object-kept-without-render-graph".to_owned(),
                    });
                }
            }
        } else if !image_path.is_empty() {
            let image_kind = if image_path.ends_with(".mdl") {
                SceneResourceKind::Mdl
            } else {
                SceneResourceKind::ModelJson
            };
            resource = self.add_optional_resource(&image_path, image_kind)?;
            if image_kind == SceneResourceKind::Mdl {
                kind = SceneAbiObjectKind::Puppet;
                material = self.add_mdl_model(handle, &image_path, resource)?;
            } else if let Some(resource_handle) = resource {
                let payload = self.resources[resource_handle as usize].payload.clone();
                match parse_json_bytes(&image_path, &payload) {
                    Ok(model_json) => {
                        let puppet_path = bound_string(model_json.get("puppet"));
                        kind = if puppet_path.is_some() {
                            SceneAbiObjectKind::Puppet
                        } else {
                            SceneAbiObjectKind::Image
                        };
                        if let Some(material_path) = bound_string(model_json.get("material"))
                            .or_else(|| bound_string(value.get("material")))
                        {
                            material = Some(self.add_material(&material_path)?);
                        }
                        if let Some(puppet_path) = puppet_path {
                            let mdl_resource =
                                self.add_optional_resource(&puppet_path, SceneResourceKind::Mdl)?;
                            let mdl_material =
                                self.add_mdl_model(handle, &puppet_path, mdl_resource)?;
                            material = mdl_material.or(material);
                        } else {
                            if let Some((width, height)) = image_plane_extent(&model_json, value) {
                                self.add_image_plane_mesh(handle, material, width, height);
                            } else if utility_layer
                                == Some(WeIrUtilityLayerKind::FullscreenPostprocess)
                            {
                                // Fullscreen utility passes use the canonical renderer triangle.
                            } else {
                                self.unsupported.push(WeIrUnsupported {
                                    object: Some(handle),
                                    pass_index: None,
                                    feature: format!("model-missing-image-plane-size:{image_path}"),
                                    expected_subsystem:
                                        "convert/we_ingest image-plane mesh lowering".to_owned(),
                                    containment: "object-kept-without-mesh".to_owned(),
                                });
                            }
                        }
                    }
                    Err(err) => {
                        self.unsupported.push(WeIrUnsupported {
                            object: Some(handle),
                            pass_index: None,
                            feature: format!("model-json-parse-failed:{image_path}:{err}"),
                            expected_subsystem: "convert/we_ingest model descriptor".to_owned(),
                            containment: "object-kept-without-material".to_owned(),
                        });
                    }
                }
            } else {
                self.unsupported.push(WeIrUnsupported {
                    object: Some(handle),
                    pass_index: None,
                    feature: format!("missing-model-resource:{image_path}"),
                    expected_subsystem: "convert/we_ingest asset source".to_owned(),
                    containment: "object-kept-without-resource".to_owned(),
                });
            }
        }

        let mut effect_instances = Vec::new();
        for effect in value
            .get("effects")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(effect_file) = bound_string(effect.get("file")) else {
                continue;
            };
            let effect_handle = self.add_effect(&effect_file)?;
            let instance_id = value_u32(effect.get("id")).unwrap_or(effect_handle);
            let visible = bound_bool(effect.get("visible")).unwrap_or(true);
            self.object_effects.push(WeIrObjectEffect {
                object: handle,
                effect: effect_handle,
                instance_id,
                visible,
            });
            if visible {
                effect_instances.push((effect_handle, effect.clone()));
            }
        }

        let color_blend_mode = value_i32(value.get("colorBlendMode")).unwrap_or(0);
        let retained_text_effect_instances;
        let retained_text_requires_dependency_composite;
        let render_effect_instances = if kind == SceneAbiObjectKind::Text {
            retained_text_requires_dependency_composite =
                effect_instances.iter().any(|(effect, _)| {
                    retained_text_effect_requires_dependency_composite(self, *effect)
                });
            retained_text_effect_instances = effect_instances
                .iter()
                .filter(|(effect, _)| retained_text_effect_is_supported(self, *effect))
                .cloned()
                .collect::<Vec<_>>();
            if retained_text_effect_instances.len() != effect_instances.len() {
                self.unsupported.push(WeIrUnsupported {
                    object: Some(handle),
                    pass_index: None,
                    feature: "some-text-layer-effects-deferred-until-dynamic-glyph-atlas"
                        .to_owned(),
                    expected_subsystem: "scene text semantic runtime".to_owned(),
                    containment: "retained-text-renders-catalog-supported-effects-only".to_owned(),
                });
            }
            retained_text_effect_instances.as_slice()
        } else {
            retained_text_requires_dependency_composite = false;
            effect_instances.as_slice()
        };
        let render_graph = if retained_text_requires_dependency_composite {
            self.unsupported.push(WeIrUnsupported {
                object: Some(handle),
                pass_index: None,
                feature: "text-clipping-mask-needs-dependency-composite".to_owned(),
                expected_subsystem: "scene RenderingDevice dependency composite".to_owned(),
                containment: "masked-text-hidden-instead-of-unmasked-solid-fallback".to_owned(),
            });
            None
        } else if let Some(material_handle) = material {
            Some(self.add_render_graph_for_object(
                handle,
                material_handle,
                render_effect_instances,
                color_blend_mode,
                utility_layer,
                kind == SceneAbiObjectKind::Puppet,
            )?)
        } else {
            None
        };
        self.add_object_animation_layers(handle, value);
        ingest_object_transform_tracks(
            handle,
            value,
            &mut self.object_transform_tracks,
            &mut self.object_transform_channels,
            &mut self.object_transform_keyframes,
            &mut self.unsupported,
        );

        self.objects.push(WeIrObject {
            handle,
            we_id,
            name,
            kind,
            resource,
            material,
            parent_we_id: value_u32(value.get("parent")),
            attachment: bound_string(value.get("attachment")).unwrap_or_default(),
            origin: parse_vec3(value.get("origin")).unwrap_or_default(),
            angles: parse_vec3(value.get("angles")).unwrap_or_default(),
            scale: parse_vec3(value.get("scale")).unwrap_or(SceneVec3::ONE),
            // Retained glyph textures already contain both the glyph and outline colors.
            color: if kind == SceneAbiObjectKind::Text && material.is_some() {
                SceneVec3::ONE
            } else {
                parse_vec3(value.get("color")).unwrap_or(SceneVec3::ONE)
            },
            alpha: value_f32(value.get("alpha"))
                .filter(|alpha| alpha.is_finite())
                .unwrap_or(1.0)
                .clamp(0.0, 1.0),
            visible: bound_bool(value.get("visible")).unwrap_or(true),
            color_blend_mode,
            sort_order: value_i32(value.get("sortorder")).unwrap_or(index as i32),
            utility_layer,
            render_graph,
        });
        Ok(())
    }

    fn add_object_animation_layers(&mut self, object: u32, value: &Value) {
        for (local_index, layer) in value
            .get("animationlayers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(animation_id) = value_u32(layer.get("animation")) else {
                self.unsupported.push(WeIrUnsupported {
                    object: Some(object),
                    pass_index: None,
                    feature: "animation-layer-missing-animation-id".to_owned(),
                    expected_subsystem: "convert/we_ingest animationlayers parser".to_owned(),
                    containment: "animation-layer-skipped".to_owned(),
                });
                continue;
            };
            self.object_animation_layers.push(WeIrObjectAnimationLayer {
                object,
                animation_id,
                layer_index: value_u32(layer.get("index")).unwrap_or(local_index as u32),
                additive: bound_bool(layer.get("additive")).unwrap_or(false),
                autosort: bound_bool(layer.get("autosort")).unwrap_or(false),
                visible: bound_bool(layer.get("visible")).unwrap_or(true),
                playback_rate: value_f32(layer.get("rate")).unwrap_or(1.0),
                blend_weight: value_f32(layer.get("blend")).unwrap_or(1.0),
                initial_progress: animation_layer_initial_progress(layer),
            });
        }
    }

    fn add_mdl_model(
        &mut self,
        object: u32,
        image_path: &str,
        resource: Option<u32>,
    ) -> Result<Option<u32>, WeIngestError> {
        let Some(resource) = resource else {
            self.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: format!("missing-mdl-resource:{image_path}"),
                expected_subsystem: "convert/we_ingest asset source".to_owned(),
                containment: "object-kept-without-resource".to_owned(),
            });
            return Ok(None);
        };
        let payload = self.resources[resource as usize].payload.clone();
        let model = match parse_mdl_model(&payload) {
            Ok(model) => model,
            Err(err) => {
                self.unsupported.push(WeIrUnsupported {
                    object: Some(object),
                    pass_index: None,
                    feature: format!("mdl-parse-failed:{image_path}:{err}"),
                    expected_subsystem: "convert/we_ingest MDLV0023 mesh parser".to_owned(),
                    containment: "object-kept-without-mdl-mesh".to_owned(),
                });
                return Ok(None);
            }
        };
        let materials = self.add_mdl_materials(object, image_path, &model.material_paths)?;
        let materials = self.specialize_puppet_materials(object, image_path, materials, &model);
        let material = materials.first().copied().flatten();
        let mut clipping_mask_resources = Vec::with_capacity(model.entries.len());
        for (entry_index, entry) in model.entries.iter().enumerate() {
            let material_path = model
                .material_paths
                .get(entry_index)
                .or_else(|| model.material_paths.first())
                .map(String::as_str);
            let mut entry_resources = Vec::with_capacity(entry.clipping_subdraws.len());
            for subdraw in &entry.clipping_subdraws {
                entry_resources.push(self.add_texture(&subdraw.mask_resource, material_path)?);
            }
            clipping_mask_resources.push(entry_resources);
        }
        let (mesh_start, mesh_count) = self.add_mdl_meshes(
            object,
            image_path,
            &model.entries,
            &materials,
            &clipping_mask_resources,
        );
        self.add_mdl_puppet(
            object,
            resource,
            mesh_start,
            mesh_count,
            &model.bones,
            &model.attachments,
            &model.animations,
        );
        Ok(material)
    }

    fn add_mdl_materials(
        &mut self,
        object: u32,
        image_path: &str,
        material_paths: &[String],
    ) -> Result<Vec<Option<u32>>, WeIngestError> {
        if material_paths.is_empty() {
            self.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: format!("mdl-has-no-material-paths:{image_path}"),
                expected_subsystem: "convert/we_ingest MDLV0023 material table".to_owned(),
                containment: "mdl-meshes-kept-without-material".to_owned(),
            });
            return Ok(Vec::new());
        }

        material_paths
            .iter()
            .map(|path| self.add_material(path).map(Some))
            .collect()
    }

    fn add_mdl_puppet(
        &mut self,
        object: u32,
        resource: u32,
        mesh_start: u32,
        mesh_count: u32,
        bones: &[super::mdl::MdlBone],
        attachments: &[super::mdl::MdlAttachment],
        animations: &[super::mdl::MdlAnimationClip],
    ) {
        let puppet = self.puppets.len() as u32;
        let bone_start = self.puppet_bones.len() as u32;
        let attachment_start = self.puppet_attachments.len() as u32;
        for bone in bones {
            self.puppet_bones.push(WeIrPuppetBone {
                puppet,
                bone_index: bone.bone_index,
                name: bone.name.clone(),
                simulation_type: bone.simulation_type,
                parent_index: bone.parent_index,
                local_bind_matrix: bone.local_bind_matrix,
                simulation_json: bone.simulation_json.clone(),
            });
        }
        for attachment in attachments {
            self.puppet_attachments.push(WeIrPuppetAttachment {
                puppet,
                bone_index: attachment.bone_index,
                name: attachment.name.clone(),
                local_matrix: attachment.local_matrix,
            });
        }
        let clip_start = self.puppet_animation_clips.len() as u32;
        for animation in animations {
            self.push_mdl_puppet_animation(puppet, animation);
        }
        self.puppets.push(WeIrPuppet {
            object,
            resource,
            mesh_start,
            mesh_count,
            bone_start,
            bone_count: self.puppet_bones.len() as u32 - bone_start,
            attachment_start,
            attachment_count: self.puppet_attachments.len() as u32 - attachment_start,
        });
        let clip_count = self.puppet_animation_clips.len() as u32 - clip_start;
        if clip_count != 0 && bones.is_empty() {
            self.unsupported.push(WeIrUnsupported {
                object: Some(object),
                pass_index: None,
                feature: "mdla-animation-without-mdls-bone-table".to_owned(),
                expected_subsystem: "convert/we_ingest MDLA0006 animation lowering".to_owned(),
                containment: "animation-records-kept-with-track-ordinal-bone-indices".to_owned(),
            });
        }
    }

    fn push_mdl_puppet_animation(&mut self, puppet: u32, animation: &super::mdl::MdlAnimationClip) {
        let clip = self.puppet_animation_clips.len() as u32;
        let track_start = self.puppet_animation_tracks.len() as u32;
        for track in &animation.tracks {
            let sample_start = self.puppet_animation_transform_samples.len() as u32;
            let opacity_sample_start = self.puppet_animation_opacity_samples.len() as u32;
            self.puppet_animation_transform_samples
                .extend(
                    track
                        .samples
                        .iter()
                        .map(|sample| WeIrPuppetAnimationTransformSample {
                            translation: sample.translation,
                            rotation: sample.rotation,
                            scale: sample.scale,
                        }),
                );
            self.puppet_animation_opacity_samples
                .extend(track.opacity_samples.iter().copied());
            self.puppet_animation_tracks.push(WeIrPuppetAnimationTrack {
                clip,
                bone_index: track.bone_index,
                track_flags: track.track_flags,
                sample_start,
                sample_count: self.puppet_animation_transform_samples.len() as u32 - sample_start,
                opacity_flags: track.opacity_flags,
                opacity_sample_start,
                opacity_sample_count: self.puppet_animation_opacity_samples.len() as u32
                    - opacity_sample_start,
            });
        }
        self.puppet_animation_clips.push(WeIrPuppetAnimationClip {
            puppet,
            clip_id: animation.clip_id,
            flags: animation.flags,
            name: animation.name.clone(),
            playback: animation.playback.clone(),
            fps: animation.fps,
            frame_count: animation.frame_count,
            frame_metadata: animation.frame_metadata,
            track_start,
            track_count: self.puppet_animation_tracks.len() as u32 - track_start,
        });
    }

    fn add_material(&mut self, path: &str) -> Result<u32, WeIngestError> {
        let path = normalize_we_path(path);
        if let Some(handle) = self.material_by_path.get(&path) {
            return Ok(*handle);
        }
        let handle = self.materials.len() as u32;
        self.material_by_path.insert(path.clone(), handle);
        let resource = self.add_required_resource(&path, SceneResourceKind::MaterialJson)?;
        let payload = self.resources[resource as usize].payload.clone();
        let material_json = parse_json_bytes(&path, &payload)?;
        let pass_start = self.material_passes.len() as u32;
        for pass in material_json
            .get("passes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            self.add_material_pass(handle, &path, pass)?;
        }
        let pass_count = self.material_passes.len() as u32 - pass_start;
        if pass_count == 0 {
            self.unsupported.push(WeIrUnsupported {
                object: None,
                pass_index: None,
                feature: format!("material-has-no-passes:{path}"),
                expected_subsystem: "convert/we_ingest material parser".to_owned(),
                containment: "material-record-kept-without-passes".to_owned(),
            });
        }
        self.materials.push(WeIrMaterial {
            handle,
            resource,
            pass_start,
            pass_count,
        });
        Ok(handle)
    }

    fn add_image_plane_mesh(
        &mut self,
        object: u32,
        material: Option<u32>,
        width: f32,
        height: f32,
    ) {
        let half_width = width * 0.5;
        let half_height = height * 0.5;
        let vertex_start = self.mesh_vertices.len() as u32;
        let index_start = self.mesh_indices.len() as u32;
        self.mesh_vertices.extend([
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: -half_width,
                    y: -half_height,
                    z: 0.0,
                },
                uv: [0.0, 1.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: half_width,
                    y: -half_height,
                    z: 0.0,
                },
                uv: [1.0, 1.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: half_width,
                    y: half_height,
                    z: 0.0,
                },
                uv: [1.0, 0.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
            WeIrMeshVertex {
                position: SceneVec3 {
                    x: -half_width,
                    y: half_height,
                    z: 0.0,
                },
                uv: [0.0, 0.0],
                blend_indices: [0; 4],
                blend_weights: [0.0; 4],
            },
        ]);
        self.mesh_indices.extend([0, 1, 2, 0, 2, 3]);
        self.meshes.push(WeIrMesh {
            object,
            material,
            vertex_start,
            vertex_count: 4,
            index_start,
            index_count: 6,
            width,
            height,
            bounds_min: SceneVec3 {
                x: -half_width,
                y: -half_height,
                z: 0.0,
            },
            bounds_max: SceneVec3 {
                x: half_width,
                y: half_height,
                z: 0.0,
            },
        });
    }

    fn add_material_pass(
        &mut self,
        material: u32,
        material_path: &str,
        pass: &Value,
    ) -> Result<(), WeIngestError> {
        let texture_start = self.material_textures.len() as u32;
        for (slot, texture) in pass
            .get("textures")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if let Some(path) = bound_string(Some(texture)) {
                let resource = self.add_texture(&path, Some(material_path))?;
                self.material_textures.push(WeIrMaterialTexture {
                    slot: slot as u32,
                    resource,
                    path,
                });
            } else {
                self.material_textures.push(WeIrMaterialTexture {
                    slot: slot as u32,
                    resource: None,
                    path: String::new(),
                });
            }
        }
        let texture_count = self.material_textures.len() as u32 - texture_start;

        let constant_start = self.material_constants.len() as u32;
        if let Some(constants) = pass.get("constantshadervalues").and_then(Value::as_object) {
            for (name, value) in constants {
                self.material_constants.push(WeIrMaterialConstant {
                    name: name.clone(),
                    value_json: compact_json(value),
                });
            }
        }
        let constant_count = self.material_constants.len() as u32 - constant_start;

        self.material_passes.push(WeIrMaterialPass {
            material,
            shader_key: bound_string(pass.get("shader")).unwrap_or_default(),
            target: bound_string(pass.get("target")).unwrap_or_default(),
            texture_start,
            texture_count,
            constant_start,
            constant_count,
            pipeline_blend: pipeline_blend_from_we(pass.get("blending").and_then(Value::as_str)),
            depth_test: depth_test_from_we(pass.get("depthtest").and_then(Value::as_str)),
            depth_write: pass
                .get("depthwrite")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("enabled")),
            cull_mode: cull_mode_from_we(pass.get("cullmode").and_then(Value::as_str)),
            alpha_writing: pass
                .get("alphawriting")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            clear_target: pass.get("clear").and_then(Value::as_bool).unwrap_or(false),
        });
        Ok(())
    }

    fn add_texture(
        &mut self,
        path: &str,
        material_path: Option<&str>,
    ) -> Result<Option<u32>, WeIngestError> {
        let original = normalize_we_path(path);
        if is_runtime_render_target(&original) {
            return Ok(None);
        }
        for candidate in texture_candidates(&original, material_path) {
            if let Some(resource) = self.texture_by_path.get(&candidate).copied() {
                return Ok(Some(resource));
            }
            let Some(asset) = self.source.read_optional_asset(&candidate)? else {
                continue;
            };
            let kind = if candidate.ends_with(".tex") {
                SceneResourceKind::TextureTex
            } else {
                SceneResourceKind::Raw
            };
            let resource = self.add_existing_resource(&candidate, kind, asset.source, asset.bytes);
            if candidate.ends_with(".tex") {
                match decode_tex_upload(&self.resources[resource as usize].payload).and_then(
                    |upload| {
                        let alpha_coverage_rows = texture_alpha_coverage_rows(&upload);
                        transcode_texture_upload(&candidate, upload)
                            .map(|upload| (upload, alpha_coverage_rows))
                    },
                ) {
                    Ok((upload, alpha_coverage_rows)) => self.textures.push(WeIrTexture {
                        resource,
                        format: upload.format,
                        source_runtime_format: upload.metadata.runtime_format,
                        payload_format: upload.metadata.payload_format,
                        sampler_flags: upload.metadata.sampler_flags,
                        width: upload.metadata.width,
                        height: upload.metadata.height,
                        storage_width: upload.metadata.storage_width,
                        storage_height: upload.metadata.storage_height,
                        texv_tag: upload.metadata.texv_tag,
                        texb_tag: upload.metadata.texb_tag,
                        mips: upload
                            .mips
                            .into_iter()
                            .map(|mip| WeIrTextureMip {
                                width: mip.width,
                                height: mip.height,
                                payload_offset: mip.payload_offset,
                                payload_len: mip.payload_len,
                            })
                            .collect(),
                        upload_payload: upload.payload,
                        alpha_coverage_rows,
                    }),
                    Err(source) => {
                        self.unsupported.push(WeIrUnsupported {
                            object: None,
                            pass_index: None,
                            feature: format!("tex-metadata-parse-failed:{candidate}:{source}"),
                            expected_subsystem: "convert/we_ingest tex parser".to_owned(),
                            containment: "texture-resource-kept-as-raw-payload".to_owned(),
                        });
                    }
                }
            }
            self.texture_by_path.insert(candidate.clone(), resource);
            return Ok(Some(resource));
        }
        self.unsupported.push(WeIrUnsupported {
            object: None,
            pass_index: None,
            feature: format!("missing-texture:{original}"),
            expected_subsystem: "convert/we_ingest texture resolver".to_owned(),
            containment: "texture-slot-kept-without-resource".to_owned(),
        });
        Ok(None)
    }

    fn add_effect(&mut self, path: &str) -> Result<u32, WeIngestError> {
        let path = normalize_we_path(path);
        if let Some(handle) = self.effect_by_path.get(&path) {
            return Ok(*handle);
        }
        let handle = self.effects.len() as u32;
        self.effect_by_path.insert(path.clone(), handle);
        let resource = self.add_required_resource(&path, SceneResourceKind::EffectJson)?;
        let payload = self.resources[resource as usize].payload.clone();
        let effect_json = parse_json_bytes(&path, &payload)?;

        let fbo_start = self.effect_fbos.len() as u32;
        for fbo in effect_json
            .get("fbos")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let name = bound_string(fbo.get("name")).unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let format =
                bound_string(fbo.get("format")).unwrap_or_else(|| "rgba_backbuffer".to_owned());
            let scale = value_f32(fbo.get("scale")).unwrap_or(1.0);
            self.effect_fbos.push(WeIrEffectFbo {
                name: name.clone(),
                format: format.clone(),
                scale,
            });
            self.image_targets.push(WeIrImageTarget {
                role: image_target_role(&name),
                name,
                format,
                width_divisor_milli: scale_divisor_to_milli(scale),
                height_divisor_milli: scale_divisor_to_milli(scale),
            });
        }
        let fbo_count = self.effect_fbos.len() as u32 - fbo_start;

        let pass_start = self.effect_passes.len() as u32;
        for (pass_index, pass) in effect_json
            .get("passes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            self.add_effect_pass(handle, pass_index as u32, pass)?;
        }
        let pass_count = self.effect_passes.len() as u32 - pass_start;

        self.effects.push(WeIrEffect {
            handle,
            resource,
            replacement_key: bound_string(effect_json.get("replacementkey")).unwrap_or_default(),
            pass_start,
            pass_count,
            fbo_start,
            fbo_count,
        });
        Ok(handle)
    }

    fn add_effect_pass(
        &mut self,
        effect: u32,
        pass_index: u32,
        pass: &Value,
    ) -> Result<(), WeIngestError> {
        let binding_start = self.effect_bindings.len() as u32;
        self.push_effect_pass_bindings(pass);
        let binding_count = self.effect_bindings.len() as u32 - binding_start;

        let combo_start = self.effect_combos.len() as u32;
        if let Some(combos) = pass.get("combos").and_then(Value::as_object) {
            for (name, value) in combos {
                if let Some(value) = value_i64(Some(value)) {
                    self.effect_combos.push(WeIrEffectCombo {
                        name: name.clone(),
                        value,
                    });
                }
            }
        }
        let combo_count = self.effect_combos.len() as u32 - combo_start;

        let material = bound_string(pass.get("material"))
            .map(|path| self.add_material(&path))
            .transpose()?;
        self.effect_passes.push(WeIrEffectPass {
            effect,
            pass_index,
            material,
            command: bound_string(pass.get("command")).unwrap_or_default(),
            source: bound_string(pass.get("source")).unwrap_or_default(),
            target: bound_string(pass.get("target")).unwrap_or_default(),
            binding_start,
            binding_count,
            combo_start,
            combo_count,
        });
        Ok(())
    }

    fn push_effect_pass_bindings(&mut self, pass: &Value) {
        if let Some(bindings) = pass.get("bind").and_then(Value::as_array) {
            for binding in bindings {
                let slot = value_u32(binding.get("index"))
                    .or_else(|| value_u32(binding.get("slot")))
                    .unwrap_or(0);
                let target = bound_string(binding.get("target"))
                    .or_else(|| bound_string(binding.get("source")))
                    .or_else(|| bound_string(binding.get("name")))
                    .unwrap_or_default();
                if !target.is_empty() {
                    self.effect_bindings
                        .push(WeIrEffectBinding { slot, target });
                }
            }
        }
        if let Some(textures) = pass.get("textures").and_then(Value::as_array) {
            for (slot, texture) in textures.iter().enumerate() {
                if let Some(path) = bound_string(Some(texture)) {
                    self.effect_bindings.push(WeIrEffectBinding {
                        slot: slot as u32,
                        target: path,
                    });
                } else if slot == 0 {
                    self.effect_bindings.push(WeIrEffectBinding {
                        slot: slot as u32,
                        target: "previous".to_owned(),
                    });
                }
            }
        }
    }

    fn add_render_graph_for_object(
        &mut self,
        object: u32,
        material: u32,
        effect_instances: &[(u32, Value)],
        color_blend_mode: i32,
        utility_layer: Option<WeIrUtilityLayerKind>,
        object_is_puppet: bool,
    ) -> Result<u32, WeIngestError> {
        let graph_index = self.render_graphs.len() as u32;
        let base_material_handle = material;
        let material = &self.materials[base_material_handle as usize];
        let base_pass = self
            .material_passes
            .get(material.pass_start as usize)
            .cloned();
        let base_texture_slots = base_pass
            .as_ref()
            .map(|pass| {
                self.material_textures
                    .iter()
                    .skip(pass.texture_start as usize)
                    .take(pass.texture_count as usize)
                    .map(|texture| texture.slot)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let base_pass_constants =
            material_pass_constant_names(&self.material_constants, base_pass.as_ref());
        let effects_in_authored_texture_space = puppet_material::image_effects_use_authored_texture(
            base_pass.as_ref().map_or("", |pass| &pass.shader_key),
        );
        let mut effect_passes = Vec::new();
        for (effect_handle, instance) in effect_instances {
            self.push_effect_contracts_for_instance(
                object,
                *effect_handle,
                instance,
                &mut effect_passes,
            )?;
        }
        let waterwaves_uv_field_material_index =
            waterwaves_displacement::create_waterwaves_uv_field_material(
                self,
                effects_in_authored_texture_space,
                &effect_passes,
            );
        let final_scene_blend = scene_blend_from_color_blend_mode(color_blend_mode);
        let foliage_ripple = foliage_ripple::create(
            self,
            base_material_handle,
            &effect_passes,
            final_scene_blend,
        );
        let ripple_flow_materials = ripple_flow::create(
            self,
            base_material_handle,
            &effect_passes,
            final_scene_blend,
        );
        let final_effect = final_effect::create(
            self,
            base_material_handle,
            &effect_passes,
            final_scene_blend,
            effects_in_authored_texture_space,
            object_is_puppet,
        );
        let mut graph = we_image_graph(&WeImageGraphContract {
            object_index: object as usize,
            base_material_index: Some(base_material_handle as usize),
            base_shader: base_pass.as_ref().and_then(|pass| {
                if pass.shader_key.is_empty() {
                    None
                } else {
                    Some(pass.shader_key.clone())
                }
            }),
            base_material_blending: base_pass
                .as_ref()
                .map(|pass| pipeline_blend_string(pass.pipeline_blend)),
            base_texture_slots,
            base_pass_constants,
            framebuffer_snapshot: utility_layer
                .filter(|layer| layer.samples_scene_color())
                .map(
                    |layer| crate::engine::render_graph::WeFramebufferSnapshotContract {
                        target_name: FULL_FRAMEBUFFER_TARGET.to_owned(),
                        texture_slot: 0,
                        composite_to_object_mesh: matches!(
                            layer,
                            WeIrUtilityLayerKind::FramebufferComposite
                        ),
                    },
                ),
            final_scene_blend,
            effects_in_authored_texture_space,
            puppet_skinning_after_effects: object_is_puppet && effects_in_authored_texture_space,
            waterwaves_uv_field_material_index,
            foliage_ripple_material_index: foliage_ripple,
            ripple_flow_material_indices: ripple_flow_materials,
            final_effect_material: final_effect,
            effect_passes,
        });
        puppet_clipping::apply_token_one_graph(self, object, base_material_handle, &mut graph);
        if utility_layer.is_some_and(WeIrUtilityLayerKind::samples_scene_color)
            && !self.image_targets.iter().any(|target| {
                target.name == FULL_FRAMEBUFFER_TARGET
                    && target.role == WeIrImageTargetRole::FirstClassEffectTarget
            })
        {
            self.image_targets.push(WeIrImageTarget {
                name: FULL_FRAMEBUFFER_TARGET.to_owned(),
                format: "rgba_backbuffer".to_owned(),
                role: WeIrImageTargetRole::FirstClassEffectTarget,
                width_divisor_milli: 1_000,
                height_divisor_milli: 1_000,
            });
        }
        self.render_graphs.push(graph);
        Ok(graph_index)
    }

    fn push_effect_contracts_for_instance(
        &mut self,
        object: u32,
        effect_handle: u32,
        instance: &Value,
        out: &mut Vec<WeEffectPassContract>,
    ) -> Result<(), WeIngestError> {
        let Some(effect) = self.effects.get(effect_handle as usize).cloned() else {
            return Ok(());
        };
        let effect_file = self
            .resources
            .get(effect.resource as usize)
            .map(|resource| resource.path.clone())
            .unwrap_or_default();
        for local_index in 0..effect.pass_count {
            let pass_index = effect.pass_start + local_index;
            let Some(effect_pass) = self.effect_passes.get(pass_index as usize).cloned() else {
                continue;
            };
            let base_material = effect_pass
                .material
                .and_then(|material| self.materials.get(material as usize))
                .cloned();
            let material_pass = base_material
                .as_ref()
                .and_then(|material| self.material_passes.get(material.pass_start as usize))
                .cloned();
            let base_textures = material_pass
                .as_ref()
                .map(|pass| {
                    self.material_textures
                        .iter()
                        .skip(pass.texture_start as usize)
                        .take(pass.texture_count as usize)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut binds = BTreeMap::new();
            for binding in self
                .effect_bindings
                .iter()
                .skip(effect_pass.binding_start as usize)
                .take(effect_pass.binding_count as usize)
            {
                binds.insert(binding.slot, binding.target.clone());
            }
            material_texture_bindings(&base_textures, &mut binds);
            let instance_pass = instance
                .get("passes")
                .and_then(Value::as_array)
                .and_then(|passes| passes.get(local_index as usize));
            if let Some(instance_pass) = instance_pass {
                push_instance_texture_overrides(instance_pass, &mut binds);
            }
            if material_pass.is_some() {
                binds.entry(0).or_insert_with(|| "previous".to_owned());
            }
            let mut combos = BTreeMap::new();
            for combo in self
                .effect_combos
                .iter()
                .skip(effect_pass.combo_start as usize)
                .take(effect_pass.combo_count as usize)
            {
                combos.insert(combo.name.clone(), combo.value);
            }
            if let Some(instance_pass) = instance_pass {
                push_instance_combo_overrides(instance_pass, &mut combos);
            }
            apply_builtin_effect_texture_defaults(&effect_file, &combos, &mut binds);
            let base_shader = material_pass
                .as_ref()
                .map(|pass| pass.shader_key.clone())
                .filter(|shader| !shader.is_empty());
            let combo_defaults = base_shader
                .as_deref()
                .map(|shader| self.shader_combo_defaults(shader))
                .transpose()?
                .unwrap_or_default();
            let shader = base_shader
                .as_deref()
                .map(|shader| effect_shader_variant_key(shader, &binds, &combos, &combo_defaults));
            let (material_index, pass_constants) =
                match (base_material, material_pass.clone(), shader.as_deref()) {
                    (Some(material), Some(pass), Some(shader)) => {
                        let material = self.add_effect_material_instance(
                            material,
                            pass,
                            base_textures,
                            instance_pass,
                            &binds,
                            shader,
                        )?;
                        let pass = self.materials.get(material as usize).and_then(|material| {
                            self.material_passes.get(material.pass_start as usize)
                        });
                        (
                            Some(material as usize),
                            material_pass_constant_names(&self.material_constants, pass),
                        )
                    }
                    _ => (
                        effect_pass.material.map(|material| material as usize),
                        Vec::new(),
                    ),
                };
            out.push(WeEffectPassContract {
                object_index: object as usize,
                material_index,
                effect_file: effect_file.clone(),
                pass_index: local_index,
                command: non_empty_string(&effect_pass.command),
                shader,
                source: non_empty_string(&effect_pass.source),
                target: if effect_pass.target.is_empty() {
                    None
                } else {
                    Some(effect_pass.target.clone())
                },
                binds,
                pass_constants,
                material_blending: material_pass
                    .as_ref()
                    .map(|pass| pipeline_blend_string(pass.pipeline_blend)),
                depthtest: material_pass.as_ref().map(|pass| match pass.depth_test {
                    SceneDepthTest::Enabled => "enabled".to_owned(),
                    SceneDepthTest::Disabled => "disabled".to_owned(),
                }),
                depthwrite: material_pass.as_ref().map(|pass| {
                    if pass.depth_write {
                        "enabled".to_owned()
                    } else {
                        "disabled".to_owned()
                    }
                }),
                cullmode: material_pass.as_ref().map(|pass| match pass.cull_mode {
                    SceneCullMode::Normal => "normal".to_owned(),
                    SceneCullMode::None => "nocull".to_owned(),
                }),
                combos,
            });
        }
        Ok(())
    }

    fn add_effect_material_instance(
        &mut self,
        base_material: WeIrMaterial,
        base_pass: WeIrMaterialPass,
        base_textures: Vec<WeIrMaterialTexture>,
        instance_pass: Option<&Value>,
        resolved_bindings: &BTreeMap<u32, String>,
        shader_key: &str,
    ) -> Result<u32, WeIngestError> {
        let material_path = self
            .resources
            .get(base_material.resource as usize)
            .map(|resource| resource.path.clone());
        let mut textures = base_textures
            .into_iter()
            .map(|texture| (texture.slot, texture))
            .collect::<BTreeMap<_, _>>();
        for (slot, path) in file_texture_bindings(resolved_bindings) {
            let resource = self.add_texture(&path, material_path.as_deref())?;
            textures.insert(
                slot,
                WeIrMaterialTexture {
                    slot,
                    resource,
                    path,
                },
            );
        }
        let base_constants = self
            .material_constants
            .iter()
            .skip(base_pass.constant_start as usize)
            .take(base_pass.constant_count as usize)
            .cloned()
            .collect::<Vec<_>>();
        let constants = merged_material_constants(&base_constants, instance_pass);
        let handle = self.materials.len() as u32;
        let texture_start = self.material_textures.len() as u32;
        self.material_textures.extend(textures.into_values());
        let constant_start = self.material_constants.len() as u32;
        self.material_constants.extend(constants);
        let mut pass = base_pass;
        pass.material = handle;
        pass.shader_key = shader_key.to_owned();
        pass.texture_start = texture_start;
        pass.texture_count = self.material_textures.len() as u32 - texture_start;
        pass.constant_start = constant_start;
        pass.constant_count = self.material_constants.len() as u32 - constant_start;
        let pass_start = self.material_passes.len() as u32;
        self.material_passes.push(pass);
        self.materials.push(WeIrMaterial {
            handle,
            resource: base_material.resource,
            pass_start,
            pass_count: 1,
        });
        Ok(handle)
    }

    fn shader_combo_defaults(
        &mut self,
        shader_key: &str,
    ) -> Result<BTreeMap<String, i64>, WeIngestError> {
        let shader_key = shader_key.split("__").next().unwrap_or(shader_key);
        if let Some(defaults) = self.shader_combo_defaults_by_shader.get(shader_key) {
            return Ok(defaults.clone());
        }
        let mut defaults = BTreeMap::new();
        for extension in ["vert", "frag"] {
            let path = format!("shaders/{shader_key}.{extension}");
            let Some(asset) = self.source.read_optional_asset(&path)? else {
                continue;
            };
            let source = String::from_utf8_lossy(&asset.bytes);
            for definition in parse_shader_combo_definitions(shader_key, &source) {
                defaults
                    .entry(definition.name.clone())
                    .or_insert(definition.default_value);
                if !self.shader_combo_definitions.iter().any(|existing| {
                    existing
                        .shader_key
                        .eq_ignore_ascii_case(&definition.shader_key)
                        && existing.name.eq_ignore_ascii_case(&definition.name)
                }) {
                    self.shader_combo_definitions.push(definition);
                }
            }
        }
        self.shader_combo_defaults_by_shader
            .insert(shader_key.to_owned(), defaults.clone());
        Ok(defaults)
    }

    fn build_shader_contracts(&mut self) {
        self.shader_contracts = build_shader_contract_records(
            &self.render_graphs,
            &self.material_passes,
            &self.material_textures,
            &self.material_constants,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_image_target_role_and_scale_follow_we_fbo_semantics() {
        assert_eq!(
            image_target_role("fbo_velocity"),
            WeIrImageTargetRole::NamedFbo
        );
        assert_eq!(
            image_target_role("_rt_QuarterCompoBuffer1"),
            WeIrImageTargetRole::FirstClassEffectTarget
        );
        assert_eq!(
            image_target_role("_tmp_GilderFramebufferCaustics"),
            WeIrImageTargetRole::Temporary
        );
        assert_eq!(scale_divisor_to_milli(4.0), 4_000);
        assert_eq!(scale_divisor_to_milli(1.0), 1_000);
    }

    #[test]
    fn ingests_minimal_loose_scene_project() {
        let root =
            std::env::temp_dir().join(format!("gilder-we-ingest-test-{}", std::process::id()));
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
            r#"{"general":{"orthogonalprojection":{"width":1920,"height":1080}},"objects":[{"id":7,"name":"layer","image":"models/layer.json","origin":"1 2 0","animationlayers":[{"animation":475,"index":2,"additive":true,"autosort":true}]}]}"#,
        )
        .expect("scene");
        fs::write(
            root.join("models/layer.json"),
            r#"{"width":64,"height":64,"material":"materials/layer.json"}"#,
        )
        .expect("model");
        fs::write(
            root.join("materials/layer.json"),
            r#"{"passes":[{"shader":"genericimage4","blending":"translucent","textures":[null],"constantshadervalues":{"tint":[0.2,0.4,0.6,1.0]}}]}"#,
        )
        .expect("material");

        let ir = ingest_wallpaper_engine_project(&root).expect("ir");
        assert_eq!(ir.project.title, "Demo");
        assert_eq!(ir.scene.logical_width, 1920);
        assert_eq!(ir.objects.len(), 1);
        assert_eq!(ir.object_animation_layers.len(), 1);
        assert_eq!(ir.object_animation_layers[0].animation_id, 475);
        assert_eq!(ir.object_animation_layers[0].layer_index, 2);
        assert!(ir.object_animation_layers[0].additive);
        assert!(ir.object_animation_layers[0].autosort);
        assert_eq!(ir.materials.len(), 1);
        assert_eq!(ir.meshes.len(), 1);
        assert_eq!(ir.mesh_vertices.len(), 4);
        assert_eq!(ir.mesh_indices, [0, 1, 2, 0, 2, 3]);
        assert_eq!(ir.meshes[0].width, 64.0);
        assert_eq!(ir.meshes[0].height, 64.0);
        assert_eq!(ir.render_graphs.len(), 1);
        assert!(ir.render_graphs[0].passes[0].bindings.contains(
            &crate::engine::render_graph::TextureBindingRole::PassConstant {
                name: "tint".to_owned()
            }
        ));
        assert_eq!(ir.shader_contracts.len(), 1);
        assert_eq!(ir.shader_contracts[0].texture_slot_mask, 1);
        assert_eq!(ir.shader_contracts[0].resource_heap_count, 3);
        assert_eq!(ir.shader_contracts[0].sampler_heap_count, 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingests_json_puppet_descriptor_into_mdl_ir_records() {
        let root =
            std::env::temp_dir().join(format!("gilder-we-mdl-ingest-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("models")).expect("models");
        fs::create_dir_all(root.join("materials")).expect("materials");
        fs::write(
            root.join("project.json"),
            r#"{"type":"scene","file":"scene.json","title":"MDL Demo"}"#,
        )
        .expect("project");
        fs::write(
            root.join("scene.json"),
            r#"{"objects":[{"id":9,"name":"puppet","image":"models/puppet.json","color":"0.1 0.2 0.3","alpha":0.4}]}"#,
        )
        .expect("scene");
        fs::write(
            root.join("models/puppet.json"),
            r#"{"material":"materials/puppet.json","puppet":"models/puppet.mdl"}"#,
        )
        .expect("model");
        fs::write(root.join("models/puppet.mdl"), test_mdlv0023()).expect("mdl");
        fs::write(
            root.join("materials/puppet.json"),
            r#"{"passes":[{"shader":"genericimage4","textures":[null]}]}"#,
        )
        .expect("material");

        let ir = ingest_wallpaper_engine_project(&root).expect("ir");

        assert_eq!(ir.objects[0].kind, SceneAbiObjectKind::Puppet);
        assert_eq!(ir.objects[0].material, Some(0));
        assert_eq!(
            ir.objects[0].color,
            SceneVec3 {
                x: 0.1,
                y: 0.2,
                z: 0.3
            }
        );
        assert_eq!(ir.objects[0].alpha, 0.4);
        assert_eq!(ir.materials.len(), 1);
        assert_eq!(ir.meshes.len(), 1);
        assert_eq!(ir.meshes[0].vertex_count, 3);
        assert_eq!(ir.meshes[0].index_count, 3);
        assert_eq!(ir.mesh_indices, [0, 1, 2]);
        assert_eq!(ir.mesh_vertices[2].position.x, 1.0);
        assert_eq!(ir.mesh_vertices[2].uv, [1.0, 1.0]);
        assert_eq!(ir.puppets.len(), 1);
        assert_eq!(ir.puppets[0].mesh_count, 1);
        assert_eq!(ir.puppets[0].attachment_count, 1);
        assert_eq!(ir.puppet_attachments[0].bone_index, 0);
        assert_eq!(ir.puppet_attachments[0].name, "eye");
        assert_eq!(ir.puppet_animation_clips.len(), 1);
        assert_eq!(ir.puppet_animation_clips[0].clip_id, 475);
        assert_eq!(ir.puppet_animation_tracks.len(), 1);
        assert_eq!(ir.puppet_animation_tracks[0].bone_index, 0);
        assert_eq!(ir.puppet_animation_transform_samples.len(), 2);
        assert_eq!(ir.puppet_animation_transform_samples[1].translation.x, 4.0);
        assert_eq!(ir.render_graphs.len(), 1);
        assert_eq!(ir.shader_contracts.len(), 1);
        assert!(ir.unsupported.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    fn test_mdlv0023() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MDLV0023\0");
        push_u32(&mut bytes, 0x0180_0009);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 1);
        bytes.extend_from_slice(b"materials/puppet.json\0");
        push_u32(&mut bytes, 0);
        for value in [0.0_f32, 0.0, 0.0, 1.0, 1.0, 0.0] {
            push_f32(&mut bytes, value);
        }
        push_u32(&mut bytes, 0x0180_000f);
        let mut vertices = Vec::new();
        push_mdl_vertex(&mut vertices, [0.0, 0.0, 0.0], [0.0, 1.0]);
        push_mdl_vertex(&mut vertices, [1.0, 0.0, 0.0], [1.0, 1.0]);
        push_mdl_vertex(&mut vertices, [1.0, 1.0, 0.0], [1.0, 1.0]);
        push_u32(&mut bytes, vertices.len() as u32);
        bytes.extend_from_slice(&vertices);
        push_u32(&mut bytes, 6);
        for index in [0_u16, 1, 2] {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        bytes.extend_from_slice(b"MDLS0004");
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 1);
        bytes.extend_from_slice(b"eye-bone\0");
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        push_u32(&mut bytes, u32::MAX);
        push_u32(&mut bytes, 64);
        for index in 0..16 {
            let value = if index == 0 || index == 5 || index == 10 || index == 15 {
                1.0
            } else {
                0.0
            };
            push_f32(&mut bytes, value);
        }
        bytes.extend_from_slice(b"{}\0");
        bytes.extend_from_slice(b"MDLA0006");
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 475);
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(b"blink\0");
        bytes.extend_from_slice(b"loop\0");
        push_f32(&mut bytes, 30.0);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 1);
        push_u32(&mut bytes, 0);
        push_u32(&mut bytes, 72);
        push_transform_sample(
            &mut bytes,
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        );
        push_transform_sample(
            &mut bytes,
            [4.0, 5.0, 6.0],
            [0.0, 0.0, 1.0],
            [2.0, 2.0, 2.0],
        );
        bytes.extend_from_slice(b"MDAT0001\0");
        push_u32(&mut bytes, 0);
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(b"eye\0");
        for index in 0..16 {
            let value = if index == 0 || index == 5 || index == 10 || index == 15 {
                1.0
            } else {
                0.0
            };
            push_f32(&mut bytes, value);
        }
        bytes
    }

    fn push_mdl_vertex(out: &mut Vec<u8>, position: [f32; 3], uv: [f32; 2]) {
        for value in position {
            push_f32(out, value);
        }
        out.resize(out.len() + 60, 0);
        push_f32(out, uv[0]);
        push_f32(out, uv[1]);
    }

    fn push_transform_sample(
        out: &mut Vec<u8>,
        translation: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
    ) {
        for value in translation.into_iter().chain(rotation).chain(scale) {
            push_f32(out, value);
        }
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_f32(out: &mut Vec<u8>, value: f32) {
        out.extend_from_slice(&value.to_le_bytes());
    }
}
