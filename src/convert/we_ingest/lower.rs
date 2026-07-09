//! Lower Wallpaper Engine ingest IR into the new Gilder scene binary ABI.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `references/godot/servers/rendering/storage/*`

use std::collections::BTreeMap;

use crate::engine::render_graph::{
    CullMode, DepthTestMode, PipelineBlendMode, RenderPassRole, RenderTargetRole,
    TextureBindingRole,
};
use crate::engine::scene::*;

use super::ir::*;

pub fn lower_ir_to_scene_binary(ir: &WeSceneIr) -> Result<SceneBinaryDocument, WeLowerError> {
    let mut strings = StringInterner::default();
    let mut resource_payload = Vec::new();
    let mut resource_payload_ranges = BTreeMap::<u32, (u64, u64)>::new();

    for resource in &ir.resources {
        let offset = resource_payload.len() as u64;
        resource_payload.extend_from_slice(&resource.payload);
        let len = resource.payload.len() as u64;
        resource_payload_ranges.insert(resource.handle, (offset, len));
    }

    let project = SceneProjectRecord {
        title: strings.id(&ir.project.title),
        wallpaper_type: strings.id(&ir.project.wallpaper_type),
        scene_file: strings.id(&ir.project.scene_file),
        preview: strings.optional_id(&ir.project.preview),
        properties_json: strings.optional_id(&ir.project.properties_json),
        logical_width: ir.scene.logical_width,
        logical_height: ir.scene.logical_height,
        clear_color: ir.scene.clear_color,
        ambient_color: ir.scene.ambient_color,
        skylight_color: ir.scene.skylight_color,
        camera_eye: ir.scene.camera_eye,
        camera_center: ir.scene.camera_center,
        camera_up: ir.scene.camera_up,
    };

    let resources = ir
        .resources
        .iter()
        .map(|resource| {
            let (payload_offset, payload_len) = resource_payload_ranges
                .get(&resource.handle)
                .copied()
                .ok_or(WeLowerError::MissingResourcePayload(resource.handle))?;
            Ok(SceneResourceRecord {
                id: SceneResourceId(resource.handle),
                kind: resource.kind,
                path: strings.id(&resource.path),
                source: strings.id(resource.source.as_str()),
                payload_offset,
                payload_len,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let textures = ir
        .textures
        .iter()
        .map(|texture| {
            let (payload_offset, payload_len) = resource_payload_ranges
                .get(&texture.resource)
                .copied()
                .unwrap_or((0, 0));
            SceneTextureRecord {
                resource: SceneResourceId(texture.resource),
                format: texture.format,
                width: texture.width,
                height: texture.height,
                storage_width: texture.storage_width,
                storage_height: texture.storage_height,
                mip_count: texture.mip_count,
                texv_tag: strings.id(&texture.texv_tag),
                texb_tag: strings.id(&texture.texb_tag),
                payload_offset,
                payload_len,
            }
        })
        .collect();

    let mut object_effects_by_object = BTreeMap::<u32, (u32, u32)>::new();
    for object in &ir.objects {
        let start = ir
            .object_effects
            .iter()
            .position(|effect| effect.object == object.handle)
            .map(|index| index as u32)
            .unwrap_or(u32::MAX);
        let count = ir
            .object_effects
            .iter()
            .filter(|effect| effect.object == object.handle)
            .count() as u32;
        object_effects_by_object.insert(
            object.handle,
            if count == 0 {
                (u32::MAX, 0)
            } else {
                (start, count)
            },
        );
    }

    let objects = ir
        .objects
        .iter()
        .map(|object| {
            let (effect_start, effect_count) = object_effects_by_object
                .get(&object.handle)
                .copied()
                .unwrap_or((u32::MAX, 0));
            SceneObjectRecord {
                id: SceneObjectHandle(object.handle),
                we_id: object.we_id,
                name: strings.optional_id(&object.name),
                kind: object.kind,
                resource: object
                    .resource
                    .map(SceneResourceId)
                    .unwrap_or(SceneResourceId::NONE),
                material: object
                    .material
                    .map(SceneMaterialHandle)
                    .unwrap_or(SceneMaterialHandle(INVALID_MATERIAL_ID)),
                parent_we_id: object.parent_we_id.unwrap_or(INVALID_OBJECT_ID),
                attachment: strings.optional_id(&object.attachment),
                origin: object.origin,
                angles: object.angles,
                scale: object.scale,
                visible: object.visible,
                color_blend_mode: object.color_blend_mode,
                sort_order: object.sort_order,
                effect_start,
                effect_count,
                render_graph: object.render_graph.unwrap_or(u32::MAX),
            }
        })
        .collect();

    let object_effects = ir
        .object_effects
        .iter()
        .map(|effect| SceneObjectEffectRecord {
            object: SceneObjectHandle(effect.object),
            effect: SceneEffectHandle(effect.effect),
            instance_id: effect.instance_id,
            visible: effect.visible,
        })
        .collect();
    let object_animation_layers = ir
        .object_animation_layers
        .iter()
        .map(|layer| SceneObjectAnimationLayerRecord {
            object: SceneObjectHandle(layer.object),
            animation_id: layer.animation_id,
            layer_index: layer.layer_index,
            additive: layer.additive,
            autosort: layer.autosort,
        })
        .collect();
    let puppet_animation_clips = ir
        .puppet_animation_clips
        .iter()
        .map(|clip| ScenePuppetAnimationClipRecord {
            puppet: clip.puppet,
            clip_id: clip.clip_id,
            flags: clip.flags,
            name: strings.optional_id(&clip.name),
            playback: strings.optional_id(&clip.playback),
            fps: clip.fps,
            frame_count: clip.frame_count,
            frame_metadata: clip.frame_metadata,
            track_start: clip.track_start,
            track_count: clip.track_count,
        })
        .collect();
    let puppet_animation_tracks = ir
        .puppet_animation_tracks
        .iter()
        .map(|track| ScenePuppetAnimationTrackRecord {
            clip: track.clip,
            bone_index: track.bone_index,
            track_flags: track.track_flags,
            sample_start: track.sample_start,
            sample_count: track.sample_count,
        })
        .collect();
    let puppet_animation_transform_samples = ir
        .puppet_animation_transform_samples
        .iter()
        .map(|sample| ScenePuppetAnimationTransformSampleRecord {
            translation: sample.translation,
            rotation: sample.rotation,
            scale: sample.scale,
        })
        .collect();

    let materials = ir
        .materials
        .iter()
        .map(|material| SceneMaterialRecord {
            id: SceneMaterialHandle(material.handle),
            resource: SceneResourceId(material.resource),
            pass_start: material.pass_start,
            pass_count: material.pass_count,
        })
        .collect();
    let material_passes = ir
        .material_passes
        .iter()
        .map(|pass| SceneMaterialPassRecord {
            material: SceneMaterialHandle(pass.material),
            shader_key: strings.optional_id(&pass.shader_key),
            target: strings.optional_id(&pass.target),
            texture_start: pass.texture_start,
            texture_count: pass.texture_count,
            constant_start: pass.constant_start,
            constant_count: pass.constant_count,
            pipeline_blend: pass.pipeline_blend,
            depth_test: pass.depth_test,
            depth_write: pass.depth_write,
            cull_mode: pass.cull_mode,
            alpha_writing: strings.optional_id(&pass.alpha_writing),
            clear_target: pass.clear_target,
        })
        .collect();
    let material_textures = ir
        .material_textures
        .iter()
        .map(|texture| SceneMaterialTextureRecord {
            slot: texture.slot,
            resource: texture
                .resource
                .map(SceneResourceId)
                .unwrap_or(SceneResourceId::NONE),
            path: strings.optional_id(&texture.path),
        })
        .collect();
    let material_constants = ir
        .material_constants
        .iter()
        .map(|constant| SceneMaterialConstantRecord {
            name: strings.id(&constant.name),
            value_json: strings.id(&constant.value_json),
        })
        .collect();

    let meshes = ir
        .meshes
        .iter()
        .map(|mesh| SceneMeshRecord {
            object: SceneObjectHandle(mesh.object),
            material: mesh
                .material
                .map(SceneMaterialHandle)
                .unwrap_or(SceneMaterialHandle(INVALID_MATERIAL_ID)),
            vertex_start: mesh.vertex_start,
            vertex_count: mesh.vertex_count,
            index_start: mesh.index_start,
            index_count: mesh.index_count,
            width: mesh.width,
            height: mesh.height,
            bounds_min: mesh.bounds_min,
            bounds_max: mesh.bounds_max,
        })
        .collect();
    let mesh_vertices = ir
        .mesh_vertices
        .iter()
        .map(|vertex| SceneMeshVertexRecord {
            position: vertex.position,
            uv: vertex.uv,
        })
        .collect();
    let mesh_indices = ir.mesh_indices.clone();
    let puppets = ir
        .puppets
        .iter()
        .map(|puppet| ScenePuppetRecord {
            object: SceneObjectHandle(puppet.object),
            resource: SceneResourceId(puppet.resource),
            mesh_start: puppet.mesh_start,
            mesh_count: puppet.mesh_count,
            bone_start: puppet.bone_start,
            bone_count: puppet.bone_count,
            attachment_start: puppet.attachment_start,
            attachment_count: puppet.attachment_count,
        })
        .collect();
    let puppet_bones = ir
        .puppet_bones
        .iter()
        .map(|bone| ScenePuppetBoneRecord {
            puppet: bone.puppet,
            bone_index: bone.bone_index,
            flags: bone.flags,
            parent_index: bone.parent_index,
            local_matrix: bone.local_matrix,
            info: strings.optional_id(&bone.info),
        })
        .collect();
    let puppet_attachments = ir
        .puppet_attachments
        .iter()
        .map(|attachment| ScenePuppetAttachmentRecord {
            puppet: attachment.puppet,
            bone_index: attachment.bone_index,
            name: strings.id(&attachment.name),
            local_matrix: attachment.local_matrix,
        })
        .collect();

    let effects = ir
        .effects
        .iter()
        .map(|effect| SceneEffectRecord {
            id: SceneEffectHandle(effect.handle),
            resource: SceneResourceId(effect.resource),
            replacement_key: strings.optional_id(&effect.replacement_key),
            pass_start: effect.pass_start,
            pass_count: effect.pass_count,
            fbo_start: effect.fbo_start,
            fbo_count: effect.fbo_count,
        })
        .collect();
    let effect_passes = ir
        .effect_passes
        .iter()
        .map(|pass| SceneEffectPassRecord {
            effect: SceneEffectHandle(pass.effect),
            pass_index: pass.pass_index,
            material: pass
                .material
                .map(SceneMaterialHandle)
                .unwrap_or(SceneMaterialHandle(INVALID_MATERIAL_ID)),
            command: strings.optional_id(&pass.command),
            source: strings.optional_id(&pass.source),
            target: strings.optional_id(&pass.target),
            binding_start: pass.binding_start,
            binding_count: pass.binding_count,
            combo_start: pass.combo_start,
            combo_count: pass.combo_count,
        })
        .collect();
    let effect_bindings = ir
        .effect_bindings
        .iter()
        .map(|binding| SceneEffectBindingRecord {
            slot: binding.slot,
            target: strings.optional_id(&binding.target),
        })
        .collect();
    let effect_combos = ir
        .effect_combos
        .iter()
        .map(|combo| SceneEffectComboRecord {
            name: strings.id(&combo.name),
            value: combo.value,
        })
        .collect();
    let effect_fbos = ir
        .effect_fbos
        .iter()
        .map(|fbo| SceneEffectFboRecord {
            name: strings.id(&fbo.name),
            format: strings.optional_id(&fbo.format),
            scale: fbo.scale,
        })
        .collect();

    let image_targets = ir
        .image_targets
        .iter()
        .map(|target| SceneImageTargetRecord {
            name: strings.id(&target.name),
            role: SceneRenderTargetKind::FirstClassEffectTarget,
            format: strings.optional_id(&target.format),
            scale_x_milli: target.scale_x_milli,
            scale_y_milli: target.scale_y_milli,
        })
        .collect();

    let (render_graphs, render_passes, render_bindings, unsupported) =
        lower_render_graphs(ir, &mut strings)?;
    let (shader_contracts, shader_constant_names) = lower_shader_contracts(ir, &mut strings);

    Ok(SceneBinaryDocument {
        feature_flags: SCENE_DEFAULT_FEATURE_FLAGS,
        strings: strings.finish(),
        project,
        resources,
        resource_payload,
        textures,
        objects,
        object_effects,
        object_animation_layers,
        puppet_animation_clips,
        puppet_animation_tracks,
        puppet_animation_transform_samples,
        materials,
        material_passes,
        material_textures,
        material_constants,
        meshes,
        mesh_vertices,
        mesh_indices,
        puppets,
        puppet_bones,
        puppet_attachments,
        effects,
        effect_passes,
        effect_bindings,
        effect_combos,
        effect_fbos,
        render_graphs,
        render_passes,
        render_bindings,
        unsupported,
        image_targets,
        shader_contracts,
        shader_constant_names,
    })
}

#[derive(Debug)]
pub enum WeLowerError {
    MissingResourcePayload(u32),
}

impl std::fmt::Display for WeLowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingResourcePayload(handle) => {
                write!(f, "IR resource {handle} has no payload range")
            }
        }
    }
}

impl std::error::Error for WeLowerError {}

fn lower_render_graphs(
    ir: &WeSceneIr,
    strings: &mut StringInterner,
) -> Result<
    (
        Vec<SceneRenderGraphRecord>,
        Vec<SceneRenderPassRecord>,
        Vec<SceneRenderBindingRecord>,
        Vec<SceneUnsupportedRecord>,
    ),
    WeLowerError,
> {
    let mut graphs = Vec::new();
    let mut passes = Vec::new();
    let mut bindings = Vec::new();
    let mut unsupported = Vec::new();

    for (graph_index, graph) in ir.render_graphs.iter().enumerate() {
        let pass_start = passes.len() as u32;
        let unsupported_start = unsupported.len() as u32;
        let object_handle = graph
            .passes
            .iter()
            .find_map(|pass| pass.object_index)
            .map(|index| index as u32)
            .unwrap_or(graph_index as u32);
        for pass in &graph.passes {
            let binding_start = bindings.len() as u32;
            for binding in &pass.bindings {
                bindings.push(lower_binding(binding, strings));
            }
            passes.push(SceneRenderPassRecord {
                id: pass.id,
                role: lower_pass_role(pass.role),
                object: SceneObjectHandle(
                    pass.object_index
                        .map(|index| index as u32)
                        .unwrap_or(INVALID_OBJECT_ID),
                ),
                material: SceneMaterialHandle(
                    pass.material_index
                        .map(|index| index as u32)
                        .unwrap_or(INVALID_MATERIAL_ID),
                ),
                pass_index: pass.pass_index,
                shader_key: strings.optional_id(pass.shader.as_deref().unwrap_or_default()),
                target: lower_render_target(pass.target),
                target_name: strings.optional_id(pass.target_name.as_deref().unwrap_or_default()),
                binding_start,
                binding_count: pass.bindings.len() as u32,
                pipeline_blend: lower_pipeline_blend(pass.state.pipeline_blend),
                depth_test: lower_depth_test(pass.state.depth_test),
                depth_write: pass.state.depth_write,
                cull_mode: lower_cull_mode(pass.state.cull_mode),
            });
        }
        for boundary in &graph.unsupported {
            unsupported.push(SceneUnsupportedRecord {
                object: SceneObjectHandle(
                    boundary
                        .object_index
                        .map(|index| index as u32)
                        .unwrap_or(INVALID_OBJECT_ID),
                ),
                pass_index: boundary.pass_index.unwrap_or(u32::MAX),
                feature: strings.id(&boundary.feature),
                expected_subsystem: strings.id(&boundary.expected_subsystem),
                containment: strings.id(&boundary.containment),
            });
        }
        graphs.push(SceneRenderGraphRecord {
            object: SceneObjectHandle(object_handle),
            pass_start,
            pass_count: graph.passes.len() as u32,
            unsupported_start,
            unsupported_count: graph.unsupported.len() as u32,
        });
    }

    let unsupported_start = unsupported.len();
    for entry in &ir.unsupported {
        unsupported.push(SceneUnsupportedRecord {
            object: SceneObjectHandle(entry.object.unwrap_or(INVALID_OBJECT_ID)),
            pass_index: entry.pass_index.unwrap_or(u32::MAX),
            feature: strings.id(&entry.feature),
            expected_subsystem: strings.id(&entry.expected_subsystem),
            containment: strings.id(&entry.containment),
        });
    }
    if unsupported.len() != unsupported_start && graphs.is_empty() {
        graphs.push(SceneRenderGraphRecord {
            object: SceneObjectHandle(INVALID_OBJECT_ID),
            pass_start: 0,
            pass_count: 0,
            unsupported_start: unsupported_start as u32,
            unsupported_count: (unsupported.len() - unsupported_start) as u32,
        });
    }

    Ok((graphs, passes, bindings, unsupported))
}

fn lower_shader_contracts(
    ir: &WeSceneIr,
    strings: &mut StringInterner,
) -> (Vec<SceneShaderContractRecord>, Vec<SceneStringId>) {
    let mut constants = Vec::new();
    let mut contracts = Vec::new();
    for contract in &ir.shader_contracts {
        let constant_start = constants.len() as u32;
        constants.extend(contract.constants.iter().map(|name| strings.id(name)));
        contracts.push(SceneShaderContractRecord {
            shader_key: strings.id(&contract.shader_key),
            pipeline_key: strings.id(&contract.pipeline_key),
            texture_slot_mask: contract.texture_slot_mask,
            constant_start,
            constant_count: contract.constants.len() as u32,
            resource_heap_count: contract.resource_heap_count,
            sampler_heap_count: contract.sampler_heap_count,
        });
    }
    (contracts, constants)
}

fn lower_binding(
    binding: &TextureBindingRole,
    strings: &mut StringInterner,
) -> SceneRenderBindingRecord {
    match binding {
        TextureBindingRole::SourceTexture => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::SourceTexture,
            slot: 0,
            target: SceneRenderTargetKind::SceneColor,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::TextureSlot { slot } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::TextureSlot,
            slot: *slot,
            target: SceneRenderTargetKind::SceneColor,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::AlphaTextureSlot { slot } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::AlphaTextureSlot,
            slot: *slot,
            target: SceneRenderTargetKind::SceneColor,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::PreviousGraphTarget => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::PreviousGraphTarget,
            slot: 0,
            target: SceneRenderTargetKind::ImageLocalMain,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::GraphTarget { role, name } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::GraphTarget,
            slot: 0,
            target: lower_render_target(*role),
            name: strings.optional_id(name.as_deref().unwrap_or_default()),
        },
        TextureBindingRole::NamedFboBind { name } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::NamedFboBind,
            slot: 0,
            target: SceneRenderTargetKind::NamedFbo,
            name: strings.id(name),
        },
        TextureBindingRole::EffectTarget { name } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::EffectTarget,
            slot: 0,
            target: SceneRenderTargetKind::FirstClassEffectTarget,
            name: strings.id(name),
        },
        TextureBindingRole::VideoFrame { media_instance } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::VideoFrame,
            slot: *media_instance,
            target: SceneRenderTargetKind::VideoExternalImage,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::AudioUniform => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::AudioUniform,
            slot: 0,
            target: SceneRenderTargetKind::Temporary,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::SystemUniform => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::SystemUniform,
            slot: 0,
            target: SceneRenderTargetKind::Temporary,
            name: SceneStringId::NONE,
        },
        TextureBindingRole::PassConstant { name } => SceneRenderBindingRecord {
            kind: SceneRenderBindingKind::PassConstant,
            slot: 0,
            target: SceneRenderTargetKind::Temporary,
            name: strings.id(name),
        },
    }
}

fn lower_pass_role(role: RenderPassRole) -> SceneRenderPassKind {
    match role {
        RenderPassRole::Clear => SceneRenderPassKind::Clear,
        RenderPassRole::BaseMaterial => SceneRenderPassKind::BaseMaterial,
        RenderPassRole::EffectMaterial => SceneRenderPassKind::EffectMaterial,
        RenderPassRole::ColorBlendPassthrough => SceneRenderPassKind::ColorBlendPassthrough,
        RenderPassRole::CopyTarget => SceneRenderPassKind::CopyTarget,
        RenderPassRole::SwapTargetReferences => SceneRenderPassKind::SwapTargetReferences,
        RenderPassRole::VideoSample => SceneRenderPassKind::VideoSample,
        RenderPassRole::Particle => SceneRenderPassKind::Particle,
        RenderPassRole::TextPath => SceneRenderPassKind::TextPath,
        RenderPassRole::SceneComposite => SceneRenderPassKind::SceneComposite,
        RenderPassRole::DebugEvidence => SceneRenderPassKind::DebugEvidence,
        RenderPassRole::Unsupported => SceneRenderPassKind::Unsupported,
    }
}

fn lower_render_target(target: RenderTargetRole) -> SceneRenderTargetKind {
    match target {
        RenderTargetRole::SceneColor => SceneRenderTargetKind::SceneColor,
        RenderTargetRole::Swapchain => SceneRenderTargetKind::Swapchain,
        RenderTargetRole::ImageLocalMain => SceneRenderTargetKind::ImageLocalMain,
        RenderTargetRole::ImageLocalSub => SceneRenderTargetKind::ImageLocalSub,
        RenderTargetRole::NamedFbo => SceneRenderTargetKind::NamedFbo,
        RenderTargetRole::FirstClassEffectTarget => SceneRenderTargetKind::FirstClassEffectTarget,
        RenderTargetRole::VideoExternalImage => SceneRenderTargetKind::VideoExternalImage,
        RenderTargetRole::Temporary => SceneRenderTargetKind::Temporary,
    }
}

fn lower_pipeline_blend(blend: PipelineBlendMode) -> ScenePipelineBlend {
    match blend {
        PipelineBlendMode::Normal => ScenePipelineBlend::Normal,
        PipelineBlendMode::Translucent => ScenePipelineBlend::Translucent,
        PipelineBlendMode::Additive => ScenePipelineBlend::Additive,
        PipelineBlendMode::Disabled => ScenePipelineBlend::Disabled,
        PipelineBlendMode::AlphaToCoverage => ScenePipelineBlend::AlphaToCoverage,
    }
}

fn lower_depth_test(depth: DepthTestMode) -> SceneDepthTest {
    match depth {
        DepthTestMode::Disabled => SceneDepthTest::Disabled,
        DepthTestMode::Less
        | DepthTestMode::LessEqual
        | DepthTestMode::Equal
        | DepthTestMode::NotEqual
        | DepthTestMode::Greater
        | DepthTestMode::Never => SceneDepthTest::Enabled,
    }
}

fn lower_cull_mode(cull: CullMode) -> SceneCullMode {
    match cull {
        CullMode::None => SceneCullMode::None,
        CullMode::Front | CullMode::Back => SceneCullMode::Normal,
    }
}

#[derive(Default)]
struct StringInterner {
    ids: BTreeMap<String, SceneStringId>,
    strings: Vec<String>,
}

impl StringInterner {
    fn id(&mut self, value: &str) -> SceneStringId {
        if let Some(id) = self.ids.get(value) {
            return *id;
        }
        let id = SceneStringId(self.strings.len() as u32);
        self.strings.push(value.to_owned());
        self.ids.insert(value.to_owned(), id);
        id
    }

    fn optional_id(&mut self, value: &str) -> SceneStringId {
        if value.is_empty() {
            SceneStringId::NONE
        } else {
            self.id(value)
        }
    }

    fn finish(self) -> Vec<String> {
        self.strings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::SceneResourceKind;

    #[test]
    fn lower_ir_uses_payload_chunk_and_string_handles() {
        let ir = WeSceneIr {
            project_root: ".".into(),
            project: WeProjectIr {
                title: "demo".to_owned(),
                wallpaper_type: "scene".to_owned(),
                scene_file: "scene.json".to_owned(),
                preview: String::new(),
                properties_json: "{}".to_owned(),
            },
            scene: WeSceneRootIr {
                logical_width: 1920,
                logical_height: 1080,
                clear_color: [0.0, 0.0, 0.0, 1.0],
                ambient_color: [0.3, 0.3, 0.3, 1.0],
                skylight_color: [0.3, 0.3, 0.3, 1.0],
                camera_eye: SceneVec3::default(),
                camera_center: SceneVec3 {
                    x: 0.0,
                    y: 0.0,
                    z: -1.0,
                },
                camera_up: SceneVec3 {
                    x: 0.0,
                    y: 1.0,
                    z: 0.0,
                },
            },
            resources: vec![WeIrResource {
                handle: 0,
                kind: SceneResourceKind::SceneJson,
                path: "scene.json".to_owned(),
                source: WeIrResourceSource::LooseFile,
                payload: b"{}".to_vec(),
            }],
            textures: Vec::new(),
            objects: Vec::new(),
            object_effects: Vec::new(),
            object_animation_layers: Vec::new(),
            puppet_animation_clips: Vec::new(),
            puppet_animation_tracks: Vec::new(),
            puppet_animation_transform_samples: Vec::new(),
            materials: Vec::new(),
            material_passes: Vec::new(),
            material_textures: Vec::new(),
            material_constants: Vec::new(),
            meshes: vec![WeIrMesh {
                object: 0,
                material: None,
                vertex_start: 0,
                vertex_count: 4,
                index_start: 0,
                index_count: 6,
                width: 64.0,
                height: 32.0,
                bounds_min: SceneVec3 {
                    x: -32.0,
                    y: -16.0,
                    z: 0.0,
                },
                bounds_max: SceneVec3 {
                    x: 32.0,
                    y: 16.0,
                    z: 0.0,
                },
            }],
            mesh_vertices: vec![
                WeIrMeshVertex {
                    position: SceneVec3 {
                        x: -32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [0.0, 1.0],
                },
                WeIrMeshVertex {
                    position: SceneVec3 {
                        x: 32.0,
                        y: -16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 1.0],
                },
                WeIrMeshVertex {
                    position: SceneVec3 {
                        x: 32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                    uv: [1.0, 0.0],
                },
                WeIrMeshVertex {
                    position: SceneVec3 {
                        x: -32.0,
                        y: 16.0,
                        z: 0.0,
                    },
                    uv: [0.0, 0.0],
                },
            ],
            mesh_indices: vec![0, 1, 2, 0, 2, 3],
            puppets: Vec::new(),
            puppet_bones: Vec::new(),
            puppet_attachments: Vec::new(),
            effects: Vec::new(),
            effect_passes: Vec::new(),
            effect_bindings: Vec::new(),
            effect_combos: Vec::new(),
            effect_fbos: Vec::new(),
            render_graphs: Vec::new(),
            image_targets: Vec::new(),
            shader_contracts: Vec::new(),
            unsupported: Vec::new(),
        };

        let binary = lower_ir_to_scene_binary(&ir).expect("lower");
        assert_eq!(binary.resource_payload, b"{}".to_vec());
        assert_eq!(binary.resources[0].payload_len, 2);
        assert!(binary.strings.iter().any(|value| value == "scene.json"));
        assert_eq!(binary.meshes.len(), 1);
        assert_eq!(binary.mesh_vertices.len(), 4);
        assert_eq!(binary.mesh_indices, [0, 1, 2, 0, 2, 3]);
        assert_eq!(binary.meshes[0].width, 64.0);
    }
}
