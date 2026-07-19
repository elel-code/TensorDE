//! Wallpaper Engine project ingest into scene IR.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/scene-pkg-format.md`

mod animation_layer;
mod asset_source;
mod builtin_effect_texture;
mod effect_target;
mod final_effect;
mod foliage_ripple;
mod image_layer_composite;
mod image_plane;
mod json_value;
mod material_graph;
mod material_instance;
mod media_state;
mod object_visual;
mod particle;
mod pipeline_state;
mod puppet_clipping;
mod puppet_material;
mod puppet_model;
mod ripple_flow;
mod script_program;
mod shader_combo;
mod shader_contract;
mod text_font_binding;
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

use serde_json::{Map, Value};

use crate::engine::render_graph::{
    WeEffectPassContract, WeImageGraphContract, we_image_graph,
    we_image_graph_requires_generated_scene_snapshot,
};
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
use script_program::{effect_script_programs, object_script_programs, project_property_defaults};
use shader_combo::parse_shader_combo_definitions;
use shader_contract::build_shader_contract_records;
use text_font_binding::text_font_overrides;
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
    let font_overrides = text_font_overrides(&scene_json, &project_json).map_err(|message| {
        WeIngestError::Script {
            object: u32::MAX,
            message,
        }
    })?;
    let mut builder = WeIrBuilder::new(
        project_root,
        source,
        project,
        scene,
        project_property_defaults(&project_json),
        font_overrides,
    );
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
    Script {
        object: u32,
        message: String,
    },
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
            Self::Script { object, message } => {
                write!(f, "invalid SceneScript on object {object}: {message}")
            }
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
        camera_parallax_enabled: bound_bool(general.get("cameraparallax")).unwrap_or(false),
        camera_parallax_amount: finite_f32(general.get("cameraparallaxamount"), 1.0),
        camera_parallax_delay: finite_f32(general.get("cameraparallaxdelay"), 0.0),
        camera_parallax_mouse_influence: finite_f32(
            general.get("cameraparallaxmouseinfluence"),
            1.0,
        ),
    }
}

struct WeIrBuilder {
    project_root: PathBuf,
    source: WeAssetSource,
    project: WeProjectIr,
    scene: WeSceneRootIr,
    project_property_defaults: Map<String, Value>,
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
    script_programs: Vec<WeIrScriptProgram>,
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
    particles: Vec<WeIrParticleSystem>,
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
    text_font_overrides: BTreeMap<String, String>,
    unsupported: Vec<WeIrUnsupported>,
}

impl WeIrBuilder {
    fn new(
        project_root: PathBuf,
        source: WeAssetSource,
        project: WeProjectIr,
        scene: WeSceneRootIr,
        project_property_defaults: Map<String, Value>,
        text_font_overrides: BTreeMap<String, String>,
    ) -> Self {
        Self {
            project_root,
            source,
            project,
            scene,
            project_property_defaults,
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
            script_programs: Vec::new(),
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
            particles: Vec::new(),
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
            text_font_overrides,
            unsupported: Vec::new(),
        }
    }

    fn finish(mut self) -> Result<WeSceneIr, WeIngestError> {
        self.materialize_image_layer_composite_targets();
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
            script_programs: self.script_programs,
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
            particles: self.particles,
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
        let particle_path = bound_string(value.get("particle")).unwrap_or_default();
        let text_value = text_layer_value(value);
        self.script_programs.extend(
            object_script_programs(
                handle,
                value,
                text_value.as_deref(),
                &self.project_property_defaults,
            )
            .map_err(|source| WeIngestError::Script {
                object: handle,
                message: source.to_string(),
            })?,
        );
        self.script_programs.extend(
            effect_script_programs(handle, value, &self.project_property_defaults).map_err(
                |source| WeIngestError::Script {
                    object: handle,
                    message: source.to_string(),
                },
            )?,
        );
        let utility_layer = utility_layer_kind(&image_path);
        let mut resource = None;
        let mut material = None;
        let mut kind = SceneAbiObjectKind::Unsupported;

        if !particle_path.is_empty() {
            kind = SceneAbiObjectKind::ParticleEmitter;
            let (particle_resource, particle_material) =
                self.add_particle_system(handle, &particle_path, value)?;
            resource = Some(particle_resource);
            material = Some(particle_material);
        } else if let Some(text) = text_value.as_deref() {
            kind = SceneAbiObjectKind::Text;
            let selected_font = self.text_font_overrides.get(&name).cloned();
            match ingest_text_layer(self, handle, value, text, selected_font.as_deref())? {
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
        let render_graph = if kind == SceneAbiObjectKind::ParticleEmitter {
            material
                .map(|material| self.add_particle_render_graph(handle, material, color_blend_mode))
        } else if retained_text_requires_dependency_composite {
            self.unsupported.push(WeIrUnsupported {
                object: Some(handle),
                pass_index: None,
                feature: "text-clipping-mask-needs-dependency-composite".to_owned(),
                expected_subsystem: "scene RenderingDevice dependency composite".to_owned(),
                containment: "masked-text-hidden-instead-of-unmasked-solid-fallback".to_owned(),
            });
            None
        } else if let Some(material_handle) = material {
            let static_black_output = value
                .get("color")
                .filter(|color| color.is_string())
                .and_then(|color| parse_vec3(Some(color)))
                .is_some_and(|color| color == SceneVec3::default());
            let puppet_group_visual_required =
                kind == SceneAbiObjectKind::Puppet && self.puppet_group_visual_required(value);
            Some(self.add_render_graph_for_object(
                handle,
                material_handle,
                render_effect_instances,
                color_blend_mode,
                utility_layer,
                kind == SceneAbiObjectKind::Puppet,
                static_black_output,
                puppet_group_visual_required,
            )?)
        } else {
            None
        };
        self.add_object_animation_layers(handle, value)?;
        ingest_object_transform_tracks(
            handle,
            value,
            &mut self.object_transform_tracks,
            &mut self.object_transform_channels,
            &mut self.object_transform_keyframes,
            &mut self.unsupported,
        );

        let media_controlled_group_hidden = kind == SceneAbiObjectKind::Unsupported
            && media_state::group_starts_hidden_without_media_session(value).map_err(
                |message| WeIngestError::Script {
                    object: handle,
                    message,
                },
            )?;
        if media_controlled_group_hidden {
            self.unsupported.push(WeIrUnsupported {
                object: Some(handle),
                pass_index: None,
                feature: "media-playback-controlled-group-awaits-session-state".to_owned(),
                expected_subsystem: "scene media-session semantic binding".to_owned(),
                containment: "media-dependent-group-hidden-while-no-session-is-connected"
                    .to_owned(),
            });
        }
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
            visible: bound_bool(value.get("visible")).unwrap_or(true)
                && !media_controlled_group_hidden,
            color_blend_mode,
            sort_order: value_i32(value.get("sortorder")).unwrap_or(index as i32),
            parallax_depth: parse_vec3(value.get("parallaxDepth"))
                .filter(|depth| depth.x.is_finite() && depth.y.is_finite())
                .map(|depth| [depth.x, depth.y])
                .unwrap_or([0.0; 2]),
            utility_layer,
            render_graph,
        });
        Ok(())
    }

    fn add_object_animation_layers(
        &mut self,
        object: u32,
        value: &Value,
    ) -> Result<(), WeIngestError> {
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
                initial_progress: animation_layer_initial_progress(layer)
                    .map_err(|message| WeIngestError::Script { object, message })?,
            });
        }
        Ok(())
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
}

fn finite_f32(value: Option<&Value>, fallback: f32) -> f32 {
    value_f32(value)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests;
