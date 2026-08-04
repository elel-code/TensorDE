//! Wallpaper Engine project ingest into scene IR.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/scene-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/scene-pkg-format.md`

mod animation_layer;
mod asset_source;
mod authored_shader;
mod builtin_effect_texture;
mod caustics_specialization;
mod effect_instance;
mod effect_target;
mod error;
mod final_effect;
mod foliage_ripple;
mod image_layer_composite;
mod image_plane;
mod json_value;
mod material_graph;
mod material_instance;
mod mdl_model;
mod media_state;
mod object_visual;
mod particle;
mod pipeline_state;
mod project_ingest;
mod puppet_clipping;
mod puppet_material;
mod puppet_model;
mod script_program;
mod shader_combo;
mod shader_contract;
mod shader_texture_default;
mod text_font_binding;
mod text_layer;
mod texture_resolver;
mod transform_animation;
mod user_property_binding;
mod utility_layer;
mod waterwaves_displacement;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[cfg(test)]
use std::fs;

use serde_json::{Map, Value};

use crate::engine::render_graph::{
    WeEffectPassContract, WeImageGraphContract, we_image_graph,
    we_image_graph_generated_scene_snapshot_slot, we_image_graph_requires_generated_scene_snapshot,
};
use crate::engine::scene::abi::{
    SceneCullMode, SceneDepthTest, SceneObjectKind as SceneAbiObjectKind, SceneResourceKind,
    SceneScriptTarget, SceneVec3,
};

use super::ir::*;
use super::tex::{
    block_compression::transcode_texture_upload, decode_tex_upload, texture_alpha_coverage_rows,
};
use animation_layer::animation_layer_initial_progress;
use asset_source::WeAssetSource;
pub(super) use authored_shader::compile_authored_shader_programs;
use builtin_effect_texture::apply_builtin_effect_texture_defaults;
use effect_target::{image_target_role, scale_divisor_to_milli};
pub use error::WeIngestError;
use image_plane::image_plane_extent;
use json_value::{
    bound_bool, bound_string, compact_json, finite_f32, infer_project_type, non_empty_string,
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
pub use project_ingest::ingest_wallpaper_engine_project;
use script_program::{
    SceneEffectVisibilityMutationPolicy, effect_script_programs, material_scalar_script_programs,
    object_script_programs, project_property_defaults, scene_effect_visibility_mutation_policy,
};
use shader_combo::parse_shader_combo_definitions;
use shader_contract::{build_shader_contract_records, material_shader_program_base};
use shader_texture_default::{
    ShaderTextureDefault, apply_shader_texture_defaults, parse_shader_texture_defaults,
};
use text_font_binding::text_font_overrides;
use text_layer::{
    DynamicTextAtlasEntry, DynamicTextAtlasKey, ingest_text_layer,
    retained_text_effect_is_supported, retained_text_effect_requires_dependency_composite,
    text_layer_value,
};
use texture_resolver::texture_candidates;
use transform_animation::ingest_object_transform_tracks;
use utility_layer::{FULL_FRAMEBUFFER_TARGET, is_runtime_render_target, utility_layer_kind};

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
        orthogonal_projection_auto: bound_bool(projection.get("auto")).unwrap_or(false),
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
    dynamic_texts: Vec<WeIrDynamicText>,
    dynamic_text_glyphs: Vec<WeIrDynamicTextGlyph>,
    dynamic_text_atlases: BTreeMap<DynamicTextAtlasKey, DynamicTextAtlasEntry>,
    user_property_bindings: Vec<WeIrUserPropertyBinding>,
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
    shader_texture_defaults_by_shader: BTreeMap<String, Vec<ShaderTextureDefault>>,
    effect_fbos: Vec<WeIrEffectFbo>,
    render_graphs: Vec<crate::engine::render_graph::RenderGraph>,
    image_targets: Vec<WeIrImageTarget>,
    shader_contracts: Vec<WeIrShaderContract>,
    text_font_overrides: BTreeMap<String, String>,
    unsupported: Vec<WeIrUnsupported>,
    effect_visibility_mutation_policy: SceneEffectVisibilityMutationPolicy,
}

impl WeIrBuilder {
    fn new(
        project_root: PathBuf,
        source: WeAssetSource,
        project: WeProjectIr,
        scene: WeSceneRootIr,
        project_property_defaults: Map<String, Value>,
        text_font_overrides: BTreeMap<String, String>,
        effect_visibility_mutation_policy: SceneEffectVisibilityMutationPolicy,
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
            dynamic_texts: Vec::new(),
            dynamic_text_glyphs: Vec::new(),
            dynamic_text_atlases: BTreeMap::new(),
            user_property_bindings: Vec::new(),
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
            shader_texture_defaults_by_shader: BTreeMap::new(),
            effect_fbos: Vec::new(),
            render_graphs: Vec::new(),
            image_targets: Vec::new(),
            shader_contracts: Vec::new(),
            text_font_overrides,
            unsupported: Vec::new(),
            effect_visibility_mutation_policy,
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
            dynamic_texts: self.dynamic_texts,
            dynamic_text_glyphs: self.dynamic_text_glyphs,
            user_property_bindings: self.user_property_bindings,
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
            shader_programs: Vec::new(),
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
        let sound_paths = value
            .get("sound")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|path| bound_string(Some(path)))
            .collect::<Vec<_>>();
        let text_value = text_layer_value(value);
        let object_programs = object_script_programs(
            handle,
            value,
            text_value.as_deref(),
            &self.project_property_defaults,
        )
        .map_err(|source| WeIngestError::Script {
            object: handle,
            message: source.to_string(),
        })?;
        let dynamic_text_programs = object_programs
            .iter()
            .filter(|program| {
                program.target == SceneScriptTarget::Text && program.updates_target_value
            })
            .cloned()
            .collect::<Vec<_>>();
        self.script_programs.extend(object_programs);
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
        let mut kind = if bound_string(value.get("camera")).is_some() {
            SceneAbiObjectKind::Camera
        } else {
            SceneAbiObjectKind::Unsupported
        };

        if let Some(sound_path) = sound_paths.first() {
            resource = Some(self.add_required_resource(sound_path, SceneResourceKind::Audio)?);
            if sound_paths.len() > 1 {
                self.unsupported.push(WeIrUnsupported {
                    object: Some(handle),
                    pass_index: None,
                    feature: "sound-layer-multiple-authored-resources".to_owned(),
                    expected_subsystem: "scene sound-layer playlist storage".to_owned(),
                    containment: "first-authored-sound-resource-retained".to_owned(),
                });
            }
        } else if !particle_path.is_empty() {
            kind = SceneAbiObjectKind::ParticleEmitter;
            let (particle_resource, particle_material) =
                self.add_particle_system(handle, &particle_path, value)?;
            resource = Some(particle_resource);
            material = Some(particle_material);
        } else if let Some(text) = text_value.as_deref() {
            kind = SceneAbiObjectKind::Text;
            let selected_font = self.text_font_overrides.get(&name).cloned();
            match ingest_text_layer(
                self,
                handle,
                value,
                text,
                selected_font.as_deref(),
                &dynamic_text_programs,
            )? {
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

        let effect_instances = self.add_object_effect_instances(handle, value)?;

        let color_blend_mode = value_i32(value.get("colorBlendMode")).unwrap_or(0);
        let retained_text_effect_instances;
        let retained_text_requires_dependency_composite;
        let render_effect_instances = if kind == SceneAbiObjectKind::Text {
            retained_text_requires_dependency_composite = effect_instances.iter().any(|instance| {
                retained_text_effect_requires_dependency_composite(self, instance.effect)
            });
            retained_text_effect_instances = effect_instances
                .iter()
                .filter(|instance| retained_text_effect_is_supported(self, instance.effect))
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
            material.map(|_| self.add_particle_render_graph(handle, color_blend_mode))
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
        let (visible, user_property_binding) = user_property_binding::object_visibility(
            handle,
            value.get("visible"),
            &self.project_property_defaults,
        )
        .map_err(|message| WeIngestError::Script {
            object: handle,
            message,
        })?;
        // A media-controlled group is explicitly contained as hidden until the media-session
        // subsystem can provide its runtime gate. Publishing its authored user-property binding
        // would let that property bypass the containment and make the group visible again.
        if !media_controlled_group_hidden && let Some(binding) = user_property_binding {
            self.user_property_bindings.push(binding);
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
            camera_zoom: value_f32(value.get("zoom"))
                .filter(|zoom| zoom.is_finite() && *zoom > 0.0)
                .unwrap_or(1.0),
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
            visible: visible && !media_controlled_group_hidden,
            color_blend_mode,
            sort_order: value_i32(value.get("sortorder")).unwrap_or(index as i32),
            parallax_depth: parse_vec3(value.get("parallaxDepth"))
                .filter(|depth| depth.x.is_finite() && depth.y.is_finite())
                .map(|depth| [depth.x, depth.y])
                .unwrap_or([0.0; 2]),
            utility_layer,
            render_source_extent_domain: if utility_layer
                .is_some_and(WeIrUtilityLayerKind::samples_scene_color)
            {
                WeIrRenderSourceExtentDomain::PhysicalSurface
            } else {
                WeIrRenderSourceExtentDomain::OwnerAuthored
            },
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
}

#[cfg(test)]
mod camera_tests;
#[cfg(test)]
mod tests;
