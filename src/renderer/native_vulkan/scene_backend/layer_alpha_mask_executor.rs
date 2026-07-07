//! WE layer alpha-mask token executor planning for Vulkan.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use std::collections::BTreeSet;

use serde::Serialize;
use vulkanalia::vk;

mod copy_back;
mod copy_back_command;
mod copy_back_geometry;
mod copy_back_pipeline;
mod copy_back_runtime;
mod copy_back_target_graph;
mod resource_binds;

use crate::engine::scene_engine::{
    SceneBlendContract, SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorBlendKey,
    SceneLayerCompositorCommand, SceneLayerCompositorCondition, SceneLayerCompositorEntry,
    SceneLayerCompositorOperation, SceneLayerCompositorPlan, SceneLayerCompositorTarget,
    SceneMaterialRenderState, SceneObject, SceneObjectGeometry, SceneObjectId, ScenePuppetId,
    SceneResource, SceneResourceId,
};

use super::frame_resources::NativeVulkanSceneFrameResources;
use super::pipeline::{NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineVertexLayout};
use super::resource_heap::texture_set::scene_shader_texture_mapping;
pub(in crate::renderer::native_vulkan) use copy_back::ALPHA_MASK_FLATTEXTURE_SHADER;
pub(in crate::renderer::native_vulkan) use copy_back_runtime::{
    NativeVulkanSceneLayerAlphaMaskCopyBackRuntimeCommandPlan,
    native_vulkan_plan_scene_layer_alpha_mask_copy_back_runtime_commands,
};
pub(in crate::renderer::native_vulkan) use resource_binds::{
    NativeVulkanSceneLayerAlphaMaskResourceBindRuntimePlan,
    native_vulkan_plan_scene_layer_alpha_mask_resource_binds,
};

const CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK: u32 = (1u32 << 0) | (1u32 << 1);
const CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT: u32 = 5;
const CLIPPINGTARGET_TEXTURE_SLOT_MASK: u32 = (1u32 << 0) | (1u32 << 8);
const FLATTEXTURE_COPY_BACK_TEXTURE_SLOT_MASK: u32 =
    1u32 << copy_back::ALPHA_MASK_FLATTEXTURE_TEXTURE_SLOT;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskRuntimePlan {
    pub tokenized_layer_count: usize,
    pub command_count: usize,
    pub required_target_count: usize,
    pub pipeline_warmup: NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan,
    pub target_scope_count: usize,
    pub alpha_mask_attachment_write_count: usize,
    pub alpha_mask_shader_sample_count: usize,
    pub token_program_dispatch_count: usize,
    pub draw_clipping_mask_count: usize,
    pub draw_style_copy_back_count: usize,
    pub generated_clipping_target_draw_count: usize,
    pub transfer_copy_count: usize,
    pub targets: Vec<NativeVulkanSceneLayerAlphaMaskTargetPlan>,
    pub commands: Vec<NativeVulkanSceneLayerAlphaMaskCommandPlan>,
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan {
    pub cache_key_count: usize,
    pub keys: Vec<NativeVulkanSceneLayerAlphaMaskPipelineKeyPlan>,
    pub command_order: [&'static str; 4],
    #[serde(skip)]
    cache_keys: Vec<NativeVulkanScenePipelineCacheKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskPipelineKeyPlan {
    pub shader: String,
    pub target_format: &'static str,
    pub pipeline_class: SceneGraphPipelineClass,
    pub texture_slot_mask: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTargetBinding {
    pub target: SceneGraphTarget,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTargetPlan {
    pub target: SceneGraphTarget,
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
    pub scale: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskCommandPlan {
    pub object: SceneObjectId,
    pub entry: SceneLayerCompositorEntry,
    pub operation: SceneLayerCompositorOperation,
    pub condition: SceneLayerCompositorCondition,
    pub source: Option<SceneLayerCompositorTarget>,
    pub target: SceneLayerCompositorTarget,
    pub source_graph_target: Option<SceneGraphTarget>,
    pub target_graph_target: Option<SceneGraphTarget>,
    pub access: NativeVulkanSceneLayerAlphaMaskAccess,
    pub copy_method: NativeVulkanSceneLayerAlphaMaskCopyMethod,
    pub blend_key: SceneLayerCompositorBlendKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskAccess {
    TokenProgram,
    AlphaMaskAttachmentWrite,
    AlphaMaskSampleAndAttachmentWrite,
    FullMaskSampleForGeneratedTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskCopyMethod {
    None,
    FlatTextureDrawDestColorBlendKey0x100,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskDescriptorPlan {
    pub tokenized_layer_count: usize,
    pub heap_bind_count: usize,
    pub clippingmaskimage4_heap_bind_count: usize,
    pub generated_clippingtarget_heap_bind_count: usize,
    pub flattexture_copy_back_heap_bind_count: usize,
    pub resident_texture_descriptor_count: usize,
    pub graph_target_descriptor_count: usize,
    pub entries: Vec<NativeVulkanSceneLayerAlphaMaskTextureBindPlan>,
    pub command_order: [&'static str; 7],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskTextureBindPlan {
    pub object: SceneObjectId,
    pub puppet: ScenePuppetId,
    pub shader: &'static str,
    pub role: NativeVulkanSceneLayerAlphaMaskTextureBindRole,
    pub slot_mask: u32,
    pub optional_morph_slot: Option<u32>,
    pub slots: Vec<NativeVulkanSceneLayerAlphaMaskSlotBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskTextureBindRole {
    ClippingMaskImage4 { clipping_record_index: u32 },
    GeneratedClippingTarget,
    FlatTextureCopyBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskSlotBinding {
    pub slot: u32,
    pub source: NativeVulkanSceneLayerAlphaMaskDescriptorSource,
    pub shader_mapping: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneLayerAlphaMaskDescriptorSource {
    ResidentTexture(SceneResourceId),
    GraphTarget(SceneGraphTarget),
}

impl NativeVulkanSceneLayerAlphaMaskRuntimePlan {
    pub(in crate::renderer::native_vulkan) fn from_layer_compositor<TargetBindingForGraph>(
        layer_compositor: &SceneLayerCompositorPlan,
        swapchain_extent: vk::Extent2D,
        mut target_binding: TargetBindingForGraph,
    ) -> Result<Self, String>
    where
        TargetBindingForGraph:
            FnMut(SceneGraphTarget) -> Result<NativeVulkanSceneLayerAlphaMaskTargetBinding, String>,
    {
        if layer_compositor.tokenized_layer_count == 0 {
            return Ok(Self::empty());
        }
        if swapchain_extent.width == 0 || swapchain_extent.height == 0 {
            return Err(
                "scene layer alpha-mask executor requires non-zero swapchain extent".into(),
            );
        }

        let full = validate_alpha_mask_target_binding(
            target_binding(SceneGraphTarget::FullAlphaMask)?,
            SceneGraphTarget::FullAlphaMask,
            swapchain_extent,
        )?;
        let intermediate = validate_alpha_mask_target_binding(
            target_binding(SceneGraphTarget::FullAlphaMaskIntermediate)?,
            SceneGraphTarget::FullAlphaMaskIntermediate,
            swapchain_extent,
        )?;
        if full.width != intermediate.width || full.height != intermediate.height {
            return Err(format!(
                "scene layer alpha-mask targets require matching extents: full {}x{}, intermediate {}x{}",
                full.width, full.height, intermediate.width, intermediate.height
            ));
        }

        let mut commands = Vec::new();
        for layer in layer_compositor
            .layers
            .iter()
            .filter(|layer| layer.uses_tokenized_subdraw)
        {
            for command in layer
                .commands
                .iter()
                .filter(|command| layer_alpha_mask_executor_command(command))
            {
                commands.push(layer_alpha_mask_command_plan(layer.object, *command)?);
            }
        }

        let token_program_dispatch_count = commands
            .iter()
            .filter(|command| {
                command.operation == SceneLayerCompositorOperation::TokenProgramDispatch
            })
            .count();
        let draw_clipping_mask_count = commands
            .iter()
            .filter(|command| command.operation == SceneLayerCompositorOperation::DrawClippingMask)
            .count();
        let draw_style_copy_back_count = commands
            .iter()
            .filter(|command| {
                command.copy_method
                    == NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100
            })
            .count();
        let generated_clipping_target_draw_count = commands
            .iter()
            .filter(|command| {
                command.operation == SceneLayerCompositorOperation::DrawGeneratedClippingTarget
            })
            .count();
        let alpha_mask_shader_sample_count = commands
            .iter()
            .filter(|command| command.source_graph_target.is_some())
            .count();
        let alpha_mask_attachment_write_count = commands
            .iter()
            .filter(|command| {
                matches!(
                    command.access,
                    NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskAttachmentWrite
                        | NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskSampleAndAttachmentWrite
                )
            })
            .count();
        let pipeline_warmup =
            NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan::from_targets(&[full, intermediate])?;

        Ok(Self {
            tokenized_layer_count: layer_compositor.tokenized_layer_count,
            command_count: commands.len(),
            required_target_count: 2,
            pipeline_warmup,
            target_scope_count: alpha_mask_attachment_write_count,
            alpha_mask_attachment_write_count,
            alpha_mask_shader_sample_count,
            token_program_dispatch_count,
            draw_clipping_mask_count,
            draw_style_copy_back_count,
            generated_clipping_target_draw_count,
            transfer_copy_count: 0,
            targets: vec![full, intermediate],
            commands,
            command_order: [
                "read_we_vtable_52_53_token_program",
                "validate_full_alpha_mask_targets_r8_half_extent",
                "derive_clippingmaskimage4_pipeline_warmup_key",
                "lower_clippingmaskimage4_to_alpha_mask_attachment_writes",
                "lower_flattexture_copy_back_to_draw_blend_key_0x100",
                "preserve_generated_clippingtarget_full_mask_sample",
                "track_alpha_mask_usage_like_godot_rendering_device_graph",
            ],
        })
    }

    fn empty() -> Self {
        Self {
            tokenized_layer_count: 0,
            command_count: 0,
            required_target_count: 0,
            pipeline_warmup: NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan::empty(),
            target_scope_count: 0,
            alpha_mask_attachment_write_count: 0,
            alpha_mask_shader_sample_count: 0,
            token_program_dispatch_count: 0,
            draw_clipping_mask_count: 0,
            draw_style_copy_back_count: 0,
            generated_clipping_target_draw_count: 0,
            transfer_copy_count: 0,
            targets: Vec::new(),
            commands: Vec::new(),
            command_order: [
                "read_we_vtable_52_53_token_program",
                "validate_full_alpha_mask_targets_r8_half_extent",
                "derive_clippingmaskimage4_pipeline_warmup_key",
                "lower_clippingmaskimage4_to_alpha_mask_attachment_writes",
                "lower_flattexture_copy_back_to_draw_blend_key_0x100",
                "preserve_generated_clippingtarget_full_mask_sample",
                "track_alpha_mask_usage_like_godot_rendering_device_graph",
            ],
        }
    }
}

impl NativeVulkanSceneLayerAlphaMaskDescriptorPlan {
    pub(in crate::renderer::native_vulkan) fn from_scene(
        resources: &[SceneResource],
        objects: &[SceneObject],
        layer_compositor: &SceneLayerCompositorPlan,
    ) -> Result<Self, String> {
        if layer_compositor.tokenized_layer_count == 0 {
            return Ok(Self::empty());
        }

        let resident_textures = resident_texture_ids(resources);
        let mut entries = Vec::new();
        for layer in layer_compositor
            .layers
            .iter()
            .filter(|layer| layer.uses_tokenized_subdraw)
        {
            let object = objects
                .iter()
                .find(|object| object.id == layer.object)
                .ok_or_else(|| {
                    format!(
                        "scene alpha-mask descriptor plan missing object {:?}",
                        layer.object
                    )
                })?;
            let source_texture = object.source.ok_or_else(|| {
                format!(
                    "scene alpha-mask descriptor plan object {:?} requires source texture for clippingmaskimage4 g_Texture0",
                    object.id
                )
            })?;
            validate_alpha_mask_resident_texture(source_texture, &resident_textures, "g_Texture0")?;
            let puppet = match object.geometry {
                SceneObjectGeometry::Puppet { puppet, .. } => puppet,
                _ => {
                    return Err(format!(
                        "scene alpha-mask descriptor plan object {:?} is tokenized but has no puppet clipping geometry",
                        object.id
                    ));
                }
            };
            let clipping = resources
                .iter()
                .find_map(|resource| match resource {
                    SceneResource::PuppetRig { id, clipping, .. } if *id == puppet => {
                        Some(clipping)
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    format!(
                        "scene alpha-mask descriptor plan object {:?} references missing puppet {:?}",
                        object.id, puppet
                    )
                })?;
            if clipping.records.is_empty() {
                return Err(format!(
                    "scene alpha-mask descriptor plan object {:?} has tokenized puppet {:?} without clipping records",
                    object.id, puppet
                ));
            }

            for (record_index, record) in clipping.records.iter().enumerate() {
                let mask_texture = record.mask_texture_index.map(SceneResourceId).ok_or_else(|| {
                    format!(
                        "scene alpha-mask descriptor plan object {:?} clipping record {} has no resolved mask texture id for clippingmaskimage4 g_Texture1",
                        object.id, record_index
                    )
                })?;
                validate_alpha_mask_resident_texture(
                    mask_texture,
                    &resident_textures,
                    "g_Texture1",
                )?;
                entries.push(NativeVulkanSceneLayerAlphaMaskTextureBindPlan {
                    object: object.id,
                    puppet,
                    shader: "we/clippingmaskimage4",
                    role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
                        clipping_record_index: record_index.min(u32::MAX as usize) as u32,
                    },
                    slot_mask: CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
                    optional_morph_slot: Some(CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT),
                    slots: vec![
                        alpha_mask_slot(
                            0,
                            NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                                source_texture,
                            ),
                        ),
                        alpha_mask_slot(
                            1,
                            NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                                mask_texture,
                            ),
                        ),
                    ],
                });
            }

            entries.push(NativeVulkanSceneLayerAlphaMaskTextureBindPlan {
                object: object.id,
                puppet,
                shader: "we/genericimage4",
                role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget,
                slot_mask: CLIPPINGTARGET_TEXTURE_SLOT_MASK,
                optional_morph_slot: None,
                slots: vec![
                    alpha_mask_slot(
                        0,
                        NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(
                            source_texture,
                        ),
                    ),
                    alpha_mask_slot(
                        8,
                        NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                            SceneGraphTarget::FullAlphaMask,
                        ),
                    ),
                ],
            });

            if layer.commands.iter().any(|command| {
                command.operation == SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            }) {
                entries.push(NativeVulkanSceneLayerAlphaMaskTextureBindPlan {
                    object: object.id,
                    puppet,
                    shader: copy_back::ALPHA_MASK_FLATTEXTURE_SHADER,
                    role: NativeVulkanSceneLayerAlphaMaskTextureBindRole::FlatTextureCopyBack,
                    slot_mask: FLATTEXTURE_COPY_BACK_TEXTURE_SLOT_MASK,
                    optional_morph_slot: None,
                    slots: vec![alpha_mask_slot(
                        copy_back::ALPHA_MASK_FLATTEXTURE_TEXTURE_SLOT,
                        NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                            SceneGraphTarget::FullAlphaMaskIntermediate,
                        ),
                    )],
                });
            }
        }

        let clippingmaskimage4_heap_bind_count = entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.role,
                    NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 { .. }
                )
            })
            .count();
        let generated_clippingtarget_heap_bind_count = entries
            .iter()
            .filter(|entry| {
                entry.role
                    == NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget
            })
            .count();
        let flattexture_copy_back_heap_bind_count = entries
            .iter()
            .filter(|entry| {
                entry.role == NativeVulkanSceneLayerAlphaMaskTextureBindRole::FlatTextureCopyBack
            })
            .count();
        let resident_texture_descriptor_count = entries
            .iter()
            .flat_map(|entry| &entry.slots)
            .filter(|slot| {
                matches!(
                    slot.source,
                    NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(_)
                )
            })
            .count();
        let graph_target_descriptor_count = entries
            .iter()
            .flat_map(|entry| &entry.slots)
            .filter(|slot| {
                matches!(
                    slot.source,
                    NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(_)
                )
            })
            .count();

        Ok(Self {
            tokenized_layer_count: layer_compositor.tokenized_layer_count,
            heap_bind_count: entries.len(),
            clippingmaskimage4_heap_bind_count,
            generated_clippingtarget_heap_bind_count,
            flattexture_copy_back_heap_bind_count,
            resident_texture_descriptor_count,
            graph_target_descriptor_count,
            entries,
            command_order: [
                "resolve_tokenized_layer_object_source_texture",
                "resolve_puppet_clipping_record_mask_texture",
                "bind_clippingmaskimage4_slots_0_1",
                "preserve_clippingmaskimage4_optional_morph_slot_5",
                "bind_generated_clippingtarget_slots_0_8",
                "bind_flattexture_copy_back_slot_0_to_intermediate_mask",
                "keep_alpha_mask_descriptors_separate_from_genericimage4_material_heap",
            ],
        })
    }

    fn empty() -> Self {
        Self {
            tokenized_layer_count: 0,
            heap_bind_count: 0,
            clippingmaskimage4_heap_bind_count: 0,
            generated_clippingtarget_heap_bind_count: 0,
            flattexture_copy_back_heap_bind_count: 0,
            resident_texture_descriptor_count: 0,
            graph_target_descriptor_count: 0,
            entries: Vec::new(),
            command_order: [
                "resolve_tokenized_layer_object_source_texture",
                "resolve_puppet_clipping_record_mask_texture",
                "bind_clippingmaskimage4_slots_0_1",
                "preserve_clippingmaskimage4_optional_morph_slot_5",
                "bind_generated_clippingtarget_slots_0_8",
                "bind_flattexture_copy_back_slot_0_to_intermediate_mask",
                "keep_alpha_mask_descriptors_separate_from_genericimage4_material_heap",
            ],
        }
    }
}

impl NativeVulkanSceneLayerAlphaMaskPipelineWarmupPlan {
    fn from_targets(targets: &[NativeVulkanSceneLayerAlphaMaskTargetPlan]) -> Result<Self, String> {
        if targets.is_empty() {
            return Ok(Self::empty());
        }
        if !targets.iter().any(|target| {
            target.target == SceneGraphTarget::FullAlphaMask
                || target.target == SceneGraphTarget::FullAlphaMaskIntermediate
        }) {
            return Err(
                "scene layer alpha-mask pipeline warmup requires an alpha-mask target".to_owned(),
            );
        }
        let cache_key = NativeVulkanScenePipelineCacheKey {
            shader: "we/clippingmaskimage4".to_owned(),
            blend: SceneBlendContract::TranslucentAlpha,
            render_state: SceneMaterialRenderState::translucent_2d(),
            pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
            target_format: vk::Format::R8_UNORM,
            texture_slot_mask: CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK,
        };
        Ok(Self {
            cache_key_count: 1,
            keys: vec![NativeVulkanSceneLayerAlphaMaskPipelineKeyPlan {
                shader: cache_key.shader.clone(),
                target_format: "R8_UNORM",
                pipeline_class: cache_key.pipeline_class,
                texture_slot_mask: cache_key.texture_slot_mask,
            }],
            cache_keys: vec![cache_key],
            command_order: [
                "select_clippingmaskimage4_shader",
                "select_puppet_skinning_mesh_vertex_layout",
                "select_r8_unorm_alpha_mask_target_format",
                "include_required_we_slots_0_1_for_mask_generator",
            ],
        })
    }

    fn empty() -> Self {
        Self {
            cache_key_count: 0,
            keys: Vec::new(),
            cache_keys: Vec::new(),
            command_order: [
                "select_clippingmaskimage4_shader",
                "select_puppet_skinning_mesh_vertex_layout",
                "select_r8_unorm_alpha_mask_target_format",
                "include_required_we_slots_0_1_for_mask_generator",
            ],
        }
    }

    pub(in crate::renderer::native_vulkan) fn cache_keys(
        &self,
    ) -> &[NativeVulkanScenePipelineCacheKey] {
        &self.cache_keys
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_runtime_frame(
    frame_resources: &NativeVulkanSceneFrameResources,
    layer_compositor: &SceneLayerCompositorPlan,
    swapchain_extent: vk::Extent2D,
) -> Result<NativeVulkanSceneLayerAlphaMaskRuntimePlan, String> {
    NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
        layer_compositor,
        swapchain_extent,
        |target| {
            let binding = frame_resources.offscreen_target_binding(target)?;
            Ok(NativeVulkanSceneLayerAlphaMaskTargetBinding {
                target: binding.target,
                format: binding.format,
                width: binding.width,
                height: binding.height,
            })
        },
    )
}

fn validate_alpha_mask_target_binding(
    binding: NativeVulkanSceneLayerAlphaMaskTargetBinding,
    target: SceneGraphTarget,
    swapchain_extent: vk::Extent2D,
) -> Result<NativeVulkanSceneLayerAlphaMaskTargetPlan, String> {
    if binding.target != target {
        return Err(format!(
            "scene layer alpha-mask executor requested {target:?} but resolver returned {:?}",
            binding.target
        ));
    }
    if binding.format != vk::Format::R8_UNORM {
        return Err(format!(
            "scene layer alpha-mask target {target:?} must be R8_UNORM, got {:?}",
            binding.format
        ));
    }
    let expected_width = swapchain_extent.width.saturating_add(1) / 2;
    let expected_height = swapchain_extent.height.saturating_add(1) / 2;
    if binding.width != expected_width || binding.height != expected_height {
        return Err(format!(
            "scene layer alpha-mask target {target:?} must be half-resolution {}x{}, got {}x{}",
            expected_width, expected_height, binding.width, binding.height
        ));
    }
    Ok(NativeVulkanSceneLayerAlphaMaskTargetPlan {
        target,
        format: "R8_UNORM",
        width: binding.width,
        height: binding.height,
        scale: 2,
    })
}

fn layer_alpha_mask_executor_command(command: &&SceneLayerCompositorCommand) -> bool {
    matches!(
        command.operation,
        SceneLayerCompositorOperation::TokenProgramDispatch
            | SceneLayerCompositorOperation::DrawClippingMask
            | SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            | SceneLayerCompositorOperation::DrawGeneratedClippingTarget
    )
}

fn layer_alpha_mask_command_plan(
    object: SceneObjectId,
    command: SceneLayerCompositorCommand,
) -> Result<NativeVulkanSceneLayerAlphaMaskCommandPlan, String> {
    let source_graph_target = command.source.and_then(layer_alpha_mask_graph_target);
    let target_graph_target = layer_alpha_mask_graph_target(command.target);
    let (access, copy_method) = match command.operation {
        SceneLayerCompositorOperation::TokenProgramDispatch => (
            NativeVulkanSceneLayerAlphaMaskAccess::TokenProgram,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
        ),
        SceneLayerCompositorOperation::DrawClippingMask => (
            NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskAttachmentWrite,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
        ),
        SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask => {
            if command.source != Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate)
                || command.target != SceneLayerCompositorTarget::FullAlphaMask
                || command.blend_key != SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100
            {
                return Err(format!(
                    "scene layer alpha-mask copy-back must be intermediate -> full with blend-key 0x100, got {:?}",
                    command
                ));
            }
            (
                NativeVulkanSceneLayerAlphaMaskAccess::AlphaMaskSampleAndAttachmentWrite,
                NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100,
            )
        }
        SceneLayerCompositorOperation::DrawGeneratedClippingTarget => {
            if command.source != Some(SceneLayerCompositorTarget::FullAlphaMask) {
                return Err(format!(
                    "scene layer generated CLIPPINGTARGET draw must sample full alpha mask, got {:?}",
                    command.source
                ));
            }
            (
                NativeVulkanSceneLayerAlphaMaskAccess::FullMaskSampleForGeneratedTarget,
                NativeVulkanSceneLayerAlphaMaskCopyMethod::None,
            )
        }
        _ => {
            return Err(format!(
                "scene layer alpha-mask executor received unsupported operation {:?}",
                command.operation
            ));
        }
    };

    Ok(NativeVulkanSceneLayerAlphaMaskCommandPlan {
        object,
        entry: command.entry,
        operation: command.operation,
        condition: command.condition,
        source: command.source,
        target: command.target,
        source_graph_target,
        target_graph_target,
        access,
        copy_method,
        blend_key: command.blend_key,
    })
}

fn layer_alpha_mask_graph_target(target: SceneLayerCompositorTarget) -> Option<SceneGraphTarget> {
    match target {
        SceneLayerCompositorTarget::FullAlphaMask => Some(SceneGraphTarget::FullAlphaMask),
        SceneLayerCompositorTarget::FullAlphaMaskIntermediate => {
            Some(SceneGraphTarget::FullAlphaMaskIntermediate)
        }
        _ => None,
    }
}

fn resident_texture_ids(resources: &[SceneResource]) -> BTreeSet<SceneResourceId> {
    resources
        .iter()
        .filter_map(|resource| match resource {
            SceneResource::Texture { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}

fn validate_alpha_mask_resident_texture(
    texture: SceneResourceId,
    resident_textures: &BTreeSet<SceneResourceId>,
    slot: &'static str,
) -> Result<(), String> {
    resident_textures.contains(&texture).then_some(()).ok_or_else(|| {
        format!("scene alpha-mask descriptor plan {slot} references non-resident texture {texture:?}")
    })
}

fn alpha_mask_slot(
    slot: u32,
    source: NativeVulkanSceneLayerAlphaMaskDescriptorSource,
) -> NativeVulkanSceneLayerAlphaMaskSlotBinding {
    NativeVulkanSceneLayerAlphaMaskSlotBinding {
        slot,
        source,
        shader_mapping: scene_shader_texture_mapping(slot),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scene::SceneMeshPuppetClippingRecord;
    use crate::engine::scene_engine::{
        SceneGeometryId, SceneLayerCompositorLayer, SceneLayerCompositorRoute,
        SceneMaterialContract, ScenePuppetClippingProgram, SceneTextureFormat,
    };
    #[test]
    fn alpha_mask_executor_keeps_empty_layer_plan_targetless() {
        let plan = NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
            &SceneLayerCompositorPlan::empty(),
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            |_| panic!("empty alpha-mask plan must not resolve targets"),
        )
        .expect("empty alpha-mask executor plan");

        assert_eq!(plan.tokenized_layer_count, 0);
        assert_eq!(plan.command_count, 0);
        assert_eq!(plan.required_target_count, 0);
        assert_eq!(plan.pipeline_warmup.cache_key_count, 0);
        assert_eq!(plan.transfer_copy_count, 0);
    }

    #[test]
    fn alpha_mask_executor_lowers_token_commands_to_draw_style_copy_back() {
        let layer_compositor = tokenized_layer_compositor();

        let plan = NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
            &layer_compositor,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            alpha_mask_binding,
        )
        .expect("alpha-mask executor plan");

        assert_eq!(plan.tokenized_layer_count, 1);
        assert_eq!(plan.required_target_count, 2);
        assert_eq!(plan.pipeline_warmup.cache_key_count, 1);
        assert_eq!(plan.command_count, 5);
        assert_eq!(plan.token_program_dispatch_count, 1);
        assert_eq!(plan.draw_clipping_mask_count, 2);
        assert_eq!(plan.draw_style_copy_back_count, 1);
        assert_eq!(plan.generated_clipping_target_draw_count, 1);
        assert_eq!(plan.transfer_copy_count, 0);
        assert_eq!(plan.targets[0].width, 1920);
        assert_eq!(plan.targets[0].height, 1080);
        assert_eq!(plan.pipeline_warmup.keys[0].shader, "we/clippingmaskimage4");
        assert_eq!(plan.pipeline_warmup.keys[0].target_format, "R8_UNORM");
        assert_eq!(
            plan.pipeline_warmup.keys[0].pipeline_class,
            SceneGraphPipelineClass::PuppetSkinning
        );
        assert_eq!(
            plan.pipeline_warmup.keys[0].texture_slot_mask,
            CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK
        );
        assert_eq!(
            plan.pipeline_warmup.cache_keys()[0].target_format,
            vk::Format::R8_UNORM
        );

        let copy_back = plan
            .commands
            .iter()
            .find(|command| {
                command.operation == SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            })
            .expect("copy-back command");
        assert_eq!(
            copy_back.copy_method,
            NativeVulkanSceneLayerAlphaMaskCopyMethod::FlatTextureDrawDestColorBlendKey0x100
        );
        assert_eq!(
            copy_back.source_graph_target,
            Some(SceneGraphTarget::FullAlphaMaskIntermediate)
        );
        assert_eq!(
            copy_back.target_graph_target,
            Some(SceneGraphTarget::FullAlphaMask)
        );
        assert_eq!(
            copy_back.blend_key,
            SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100
        );
    }

    #[test]
    fn alpha_mask_executor_rejects_non_r8_targets() {
        let err = NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
            &tokenized_layer_compositor(),
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            |target| {
                let mut binding = alpha_mask_binding(target)?;
                if target == SceneGraphTarget::FullAlphaMask {
                    binding.format = vk::Format::R16G16B16A16_SFLOAT;
                }
                Ok(binding)
            },
        )
        .expect_err("alpha-mask target format must be R8");

        assert!(err.contains("must be R8_UNORM"));
    }

    #[test]
    fn alpha_mask_executor_rejects_transfer_copy_shaped_copy_back() {
        let mut layer_compositor = tokenized_layer_compositor();
        let copy_back = layer_compositor.layers[0]
            .commands
            .iter_mut()
            .find(|command| {
                command.operation == SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask
            })
            .expect("copy-back command");
        copy_back.blend_key = SceneLayerCompositorBlendKey::Inherit;

        let err = NativeVulkanSceneLayerAlphaMaskRuntimePlan::from_layer_compositor(
            &layer_compositor,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
            alpha_mask_binding,
        )
        .expect_err("copy-back without blend-key 0x100 must fail");

        assert!(err.contains("blend-key 0x100"));
    }

    #[test]
    fn alpha_mask_descriptor_plan_binds_mask_generator_and_generated_target_slots() {
        let layer_compositor = tokenized_layer_compositor();
        let objects = vec![SceneObject {
            id: SceneObjectId(77),
            geometry: SceneObjectGeometry::Puppet {
                geometry: SceneGeometryId(3),
                puppet: ScenePuppetId(5),
                vertex_count: 4,
                index_count: 6,
            },
            material: SceneMaterialContract::we_translucent("we/genericimage4"),
            source: Some(SceneResourceId(9)),
        }];
        let mut clipping = ScenePuppetClippingProgram::from_source_records(
            vec![SceneMeshPuppetClippingRecord {
                source_name: None,
                mask: "masks/clipping_mask_eye".to_owned(),
                mask_resource: Some("assets/clipping-mask.gtex".to_owned()),
                duration_frames: 1680,
                flags: 1,
                bones: vec![42, 43],
                frame_keys: vec![0, 1, 2],
            }],
            Vec::new(),
        );
        clipping.resolve_mask_texture_indices(|path| {
            (path == "assets/clipping-mask.gtex").then_some(SceneResourceId(12))
        });
        let resources = vec![
            texture_resource(SceneResourceId(9)),
            texture_resource(SceneResourceId(12)),
            SceneResource::PuppetRig {
                id: ScenePuppetId(5),
                source_record: 4,
                skin: None,
                clips: Vec::new(),
                layers: Vec::new(),
                clipping,
            },
        ];

        let plan = NativeVulkanSceneLayerAlphaMaskDescriptorPlan::from_scene(
            &resources,
            &objects,
            &layer_compositor,
        )
        .expect("alpha-mask descriptor plan");

        assert_eq!(plan.heap_bind_count, 3);
        assert_eq!(plan.clippingmaskimage4_heap_bind_count, 1);
        assert_eq!(plan.generated_clippingtarget_heap_bind_count, 1);
        assert_eq!(plan.flattexture_copy_back_heap_bind_count, 1);
        assert_eq!(plan.resident_texture_descriptor_count, 3);
        assert_eq!(plan.graph_target_descriptor_count, 2);
        let mask_generator = &plan.entries[0];
        assert_eq!(mask_generator.shader, "we/clippingmaskimage4");
        assert_eq!(
            mask_generator.role,
            NativeVulkanSceneLayerAlphaMaskTextureBindRole::ClippingMaskImage4 {
                clipping_record_index: 0
            }
        );
        assert_eq!(
            mask_generator.slot_mask,
            CLIPPINGMASKIMAGE4_REQUIRED_TEXTURE_SLOT_MASK
        );
        assert_eq!(
            mask_generator.optional_morph_slot,
            Some(CLIPPINGMASKIMAGE4_MORPH_TEXTURE_SLOT)
        );
        assert_eq!(
            mask_generator.slots[0].source,
            NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(SceneResourceId(9))
        );
        assert_eq!(
            mask_generator.slots[1].source,
            NativeVulkanSceneLayerAlphaMaskDescriptorSource::ResidentTexture(SceneResourceId(12))
        );
        let generated_target = &plan.entries[1];
        assert_eq!(generated_target.shader, "we/genericimage4");
        assert_eq!(generated_target.slot_mask, CLIPPINGTARGET_TEXTURE_SLOT_MASK);
        assert_eq!(
            generated_target.slots[1].source,
            NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                SceneGraphTarget::FullAlphaMask
            )
        );
        assert_eq!(
            generated_target.slots[1].shader_mapping,
            "set0.binding8.g_Texture8"
        );
        let copy_back = &plan.entries[2];
        assert_eq!(copy_back.shader, "util/minimalalpha");
        assert_eq!(
            copy_back.role,
            NativeVulkanSceneLayerAlphaMaskTextureBindRole::FlatTextureCopyBack
        );
        assert_eq!(copy_back.slot_mask, FLATTEXTURE_COPY_BACK_TEXTURE_SLOT_MASK);
        assert_eq!(
            copy_back.slots[0].source,
            NativeVulkanSceneLayerAlphaMaskDescriptorSource::GraphTarget(
                SceneGraphTarget::FullAlphaMaskIntermediate
            )
        );
        assert_eq!(
            copy_back.slots[0].shader_mapping,
            "set0.binding0.g_Texture0"
        );
    }

    #[test]
    fn alpha_mask_descriptor_plan_rejects_unresolved_mask_texture() {
        let layer_compositor = tokenized_layer_compositor();
        let objects = vec![SceneObject {
            id: SceneObjectId(77),
            geometry: SceneObjectGeometry::Puppet {
                geometry: SceneGeometryId(3),
                puppet: ScenePuppetId(5),
                vertex_count: 4,
                index_count: 6,
            },
            material: SceneMaterialContract::we_translucent("we/genericimage4"),
            source: Some(SceneResourceId(9)),
        }];
        let clipping = ScenePuppetClippingProgram::from_source_records(
            vec![SceneMeshPuppetClippingRecord {
                source_name: None,
                mask: "masks/clipping_mask_eye".to_owned(),
                mask_resource: Some("assets/clipping-mask.gtex".to_owned()),
                duration_frames: 1680,
                flags: 1,
                bones: vec![42, 43],
                frame_keys: vec![0, 1, 2],
            }],
            Vec::new(),
        );
        let resources = vec![
            texture_resource(SceneResourceId(9)),
            SceneResource::PuppetRig {
                id: ScenePuppetId(5),
                source_record: 4,
                skin: None,
                clips: Vec::new(),
                layers: Vec::new(),
                clipping,
            },
        ];

        let err = NativeVulkanSceneLayerAlphaMaskDescriptorPlan::from_scene(
            &resources,
            &objects,
            &layer_compositor,
        )
        .expect_err("mask texture id must be resolved");

        assert!(err.contains("no resolved mask texture id"));
    }

    fn tokenized_layer_compositor() -> SceneLayerCompositorPlan {
        let commands = vec![
            command(
                SceneLayerCompositorEntry::TokenizedCompositeEntry52,
                SceneLayerCompositorOperation::TokenProgramDispatch,
                SceneLayerCompositorCondition::Always,
                None,
                SceneLayerCompositorTarget::LayerTarget490,
                SceneLayerCompositorBlendKey::Inherit,
            ),
            command(
                SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
                SceneLayerCompositorOperation::DrawClippingMask,
                SceneLayerCompositorCondition::Token1OrToken2FirstPair,
                None,
                SceneLayerCompositorTarget::FullAlphaMask,
                SceneLayerCompositorBlendKey::Inherit,
            ),
            command(
                SceneLayerCompositorEntry::AlphaMaskHelper20d6a0,
                SceneLayerCompositorOperation::DrawClippingMask,
                SceneLayerCompositorCondition::Token2IntermediatePairOrFinalMask,
                None,
                SceneLayerCompositorTarget::FullAlphaMaskIntermediate,
                SceneLayerCompositorBlendKey::Inherit,
            ),
            command(
                SceneLayerCompositorEntry::FlatTextureCopyBack20d9ed,
                SceneLayerCompositorOperation::CopyIntermediateToFullAlphaMask,
                SceneLayerCompositorCondition::Token2AfterIntermediateMask,
                Some(SceneLayerCompositorTarget::FullAlphaMaskIntermediate),
                SceneLayerCompositorTarget::FullAlphaMask,
                SceneLayerCompositorBlendKey::DestColorCopyBackBit0x100,
            ),
            command(
                SceneLayerCompositorEntry::TokenizedCompositeWithMaterialEntry53,
                SceneLayerCompositorOperation::DrawGeneratedClippingTarget,
                SceneLayerCompositorCondition::TokenizedGeneratedMaterial,
                Some(SceneLayerCompositorTarget::FullAlphaMask),
                SceneLayerCompositorTarget::LayerTarget490,
                SceneLayerCompositorBlendKey::SubdrawBlendByteToGeneratedMaterial1f0,
            ),
        ];
        SceneLayerCompositorPlan {
            layer_count: 1,
            command_count: commands.len(),
            object_final_layer_count: 0,
            tokenized_layer_count: 1,
            layers: vec![SceneLayerCompositorLayer {
                object: SceneObjectId(77),
                route: SceneLayerCompositorRoute::DirectSwapchain,
                uses_tokenized_subdraw: true,
                commands,
            }],
            command_order: [
                "preserve_scene_object_order",
                "classify_object_final_routes",
                "model_vtable_32_normal_render_entry",
                "model_vtable_50_clear_prep_entry",
                "model_vtable_52_53_tokenized_subdraw_entries",
                "model_wrapper_0xd8_0x128_state_keys",
                "model_flattexture_intermediate_copy_back_bit_0x100",
                "model_vtable_51_full_layer_composite_entry",
                "lower_layer_routes_to_scene_graph_passes",
            ],
        }
    }

    fn command(
        entry: SceneLayerCompositorEntry,
        operation: SceneLayerCompositorOperation,
        condition: SceneLayerCompositorCondition,
        source: Option<SceneLayerCompositorTarget>,
        target: SceneLayerCompositorTarget,
        blend_key: SceneLayerCompositorBlendKey,
    ) -> SceneLayerCompositorCommand {
        SceneLayerCompositorCommand {
            entry,
            operation,
            condition,
            source,
            target,
            blend_key,
        }
    }

    fn alpha_mask_binding(
        target: SceneGraphTarget,
    ) -> Result<NativeVulkanSceneLayerAlphaMaskTargetBinding, String> {
        match target {
            SceneGraphTarget::FullAlphaMask | SceneGraphTarget::FullAlphaMaskIntermediate => {
                Ok(NativeVulkanSceneLayerAlphaMaskTargetBinding {
                    target,
                    format: vk::Format::R8_UNORM,
                    width: 1920,
                    height: 1080,
                })
            }
            target => Err(format!("unexpected alpha-mask target {target:?}")),
        }
    }

    fn texture_resource(id: SceneResourceId) -> SceneResource {
        SceneResource::Texture {
            id,
            source: std::path::PathBuf::from(format!("assets/texture-{}.gtex", id.0)),
            width: Some(64),
            height: Some(64),
            format: Some(SceneTextureFormat::R8G8B8A8Unorm),
            mip_count: Some(1),
            payload_bytes: Some(16_384),
        }
    }
}
