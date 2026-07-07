//! Scene texture descriptor binding plans for mesh draws.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `reverse-engineered/docs/exe/texture-and-format.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use std::collections::BTreeSet;

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SCENE_WE_PASS_INPUT_TEXTURE_SLOT, SceneGraph, SceneGraphResourceRole, SceneGraphTarget,
    SceneObjectId, SceneResourceId, SceneTextureFormat, SceneTextureResidency,
};

use super::resource_heap::texture_set::scene_shader_texture_mapping;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureDescriptorFramePlan {
    pub draw_count: usize,
    pub binding_count: usize,
    pub bindings: Vec<NativeVulkanSceneTextureDescriptorBinding>,
    pub descriptor_model: &'static str,
    pub command_order: [&'static str; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureDescriptorBinding {
    pub draw_index: usize,
    pub object: SceneObjectId,
    pub slot: u32,
    pub role: SceneGraphResourceRole,
    pub source: NativeVulkanSceneTextureDescriptorSource,
    pub width: u32,
    pub height: u32,
    pub format: NativeVulkanSceneTextureDescriptorFormat,
    pub mip_count: u32,
    pub payload_bytes: Option<u64>,
    pub shader_mapping: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneTextureDescriptorSource {
    ResidentTexture(SceneResourceId),
    GraphTarget(SceneGraphTarget),
    PreviousFramebuffer {
        object: SceneObjectId,
        effect_pass_index: usize,
    },
    Scene {
        object: SceneObjectId,
        effect_pass_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneTextureDescriptorFormat {
    SceneTexture(SceneTextureFormat),
    VkFormat(NativeVulkanSceneTextureDescriptorVkFormat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneTextureDescriptorVkFormat {
    R16G16B16A16Sfloat,
    R16G16Sfloat,
    R8G8B8A8Unorm,
    B8G8R8A8Unorm,
    R16Sfloat,
    R8Unorm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTargetInputTextureDescriptor {
    pub target: SceneGraphTarget,
    pub width: u32,
    pub height: u32,
    pub format: NativeVulkanSceneTextureDescriptorVkFormat,
}

impl NativeVulkanSceneTextureDescriptorVkFormat {
    pub(in crate::renderer::native_vulkan) fn from_vk_format(
        format: vk::Format,
    ) -> Result<Self, String> {
        match format {
            vk::Format::R16G16B16A16_SFLOAT => Ok(Self::R16G16B16A16Sfloat),
            vk::Format::R16G16_SFLOAT => Ok(Self::R16G16Sfloat),
            vk::Format::R8G8B8A8_UNORM => Ok(Self::R8G8B8A8Unorm),
            vk::Format::B8G8R8A8_UNORM => Ok(Self::B8G8R8A8Unorm),
            vk::Format::R16_SFLOAT => Ok(Self::R16Sfloat),
            vk::Format::R8_UNORM => Ok(Self::R8Unorm),
            format => Err(format!(
                "scene texture descriptor cannot represent graph target vk::Format {format:?}"
            )),
        }
    }

    pub(in crate::renderer::native_vulkan) const fn to_vk_format(self) -> vk::Format {
        match self {
            Self::R16G16B16A16Sfloat => vk::Format::R16G16B16A16_SFLOAT,
            Self::R16G16Sfloat => vk::Format::R16G16_SFLOAT,
            Self::R8G8B8A8Unorm => vk::Format::R8G8B8A8_UNORM,
            Self::B8G8R8A8Unorm => vk::Format::B8G8R8A8_UNORM,
            Self::R16Sfloat => vk::Format::R16_SFLOAT,
            Self::R8Unorm => vk::Format::R8_UNORM,
        }
    }
}

impl NativeVulkanSceneTextureDescriptorFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_graph<TextureResidency>(
        graph: &SceneGraph,
        texture_residency: TextureResidency,
    ) -> Result<Self, String>
    where
        TextureResidency: FnMut(SceneResourceId) -> Option<SceneTextureResidency>,
    {
        Self::from_graph_with_target_inputs(graph, texture_residency, |target| {
            Err(format!(
                "scene texture descriptor plan requires retained graph target input {:?}",
                target
            ))
        })
    }

    pub(in crate::renderer::native_vulkan) fn from_graph_with_target_inputs<
        TextureResidency,
        TargetInput,
    >(
        graph: &SceneGraph,
        mut texture_residency: TextureResidency,
        mut target_input: TargetInput,
    ) -> Result<Self, String>
    where
        TextureResidency: FnMut(SceneResourceId) -> Option<SceneTextureResidency>,
        TargetInput: FnMut(
            SceneGraphTarget,
        )
            -> Result<NativeVulkanSceneTargetInputTextureDescriptor, String>,
    {
        let mut bindings = Vec::new();
        let mut draw_index = 0usize;

        for pass in &graph.passes {
            for draw in &pass.draws {
                if !draw.pipeline.is_indexed_mesh_graphics() {
                    return Err(format!(
                        "scene texture descriptor plan requires indexed mesh graphics pipeline, got {:?} for object {:?}",
                        draw.pipeline, draw.object
                    ));
                }

                let _ = draw.shader_texture_slot_mask_with_pass_input(pass.input)?;
                let mut used_slots = BTreeSet::new();
                let mut draw_bindings = Vec::new();
                for resource in &draw.resources {
                    let texture_index = resource.role.shader_texture_index();
                    if texture_index != resource.slot {
                        return Err(format!(
                            "scene texture descriptor plan slot {} does not match WE g_Texture{} role for object {:?}",
                            resource.slot, texture_index, draw.object
                        ));
                    }
                    if !used_slots.insert(resource.slot) {
                        return Err(format!(
                            "duplicate scene texture descriptor slot {} for object {:?}",
                            resource.slot, draw.object
                        ));
                    }

                    let texture = texture_residency(resource.resource).ok_or_else(|| {
                        format!(
                            "missing resident scene texture {:?} for object {:?}",
                            resource.resource, draw.object
                        )
                    })?;
                    let texture = resident_texture_descriptor(resource.resource, texture)?;
                    draw_bindings.push(NativeVulkanSceneTextureDescriptorBinding {
                        draw_index,
                        object: draw.object,
                        slot: resource.slot,
                        role: resource.role,
                        source: NativeVulkanSceneTextureDescriptorSource::ResidentTexture(
                            resource.resource,
                        ),
                        width: texture.width,
                        height: texture.height,
                        format: NativeVulkanSceneTextureDescriptorFormat::SceneTexture(
                            texture.format,
                        ),
                        mip_count: texture.mip_count,
                        payload_bytes: Some(texture.payload_bytes),
                        shader_mapping: scene_shader_texture_mapping(resource.slot),
                    });
                }
                if let Some(target) = pass.input {
                    if !used_slots.insert(SCENE_WE_PASS_INPUT_TEXTURE_SLOT) {
                        return Err(format!(
                            "scene texture descriptor plan pass input {:?} collides with WE g_Texture{} for object {:?}",
                            target, SCENE_WE_PASS_INPUT_TEXTURE_SLOT, draw.object
                        ));
                    }
                    let input = target_input(target)?;
                    if input.target != target {
                        return Err(format!(
                            "scene texture descriptor plan target resolver returned {:?} for requested {:?}",
                            input.target, target
                        ));
                    }
                    draw_bindings.push(NativeVulkanSceneTextureDescriptorBinding {
                        draw_index,
                        object: draw.object,
                        slot: SCENE_WE_PASS_INPUT_TEXTURE_SLOT,
                        role: SceneGraphResourceRole::shader_texture(
                            SCENE_WE_PASS_INPUT_TEXTURE_SLOT,
                        ),
                        source: NativeVulkanSceneTextureDescriptorSource::GraphTarget(target),
                        width: input.width,
                        height: input.height,
                        format: NativeVulkanSceneTextureDescriptorFormat::VkFormat(input.format),
                        mip_count: 1,
                        payload_bytes: None,
                        shader_mapping: scene_shader_texture_mapping(
                            SCENE_WE_PASS_INPUT_TEXTURE_SLOT,
                        ),
                    });
                }
                draw_bindings.sort_by_key(|binding| binding.slot);
                bindings.extend(draw_bindings);
                draw_index += 1;
            }
        }

        Ok(Self {
            draw_count: draw_index,
            binding_count: bindings.len(),
            bindings,
            descriptor_model: "VK_EXT_descriptor_heap",
            command_order: [
                "resolve_resident_texture_descriptors",
                "bind_descriptor_heap_texture_mapping",
            ],
        })
    }
}

struct ResidentTextureDescriptor {
    width: u32,
    height: u32,
    format: SceneTextureFormat,
    mip_count: u32,
    payload_bytes: u64,
}

fn resident_texture_descriptor(
    resource: SceneResourceId,
    texture: SceneTextureResidency,
) -> Result<ResidentTextureDescriptor, String> {
    let width = texture
        .width
        .ok_or_else(|| format!("scene texture descriptor {:?} missing width", resource))?;
    let height = texture
        .height
        .ok_or_else(|| format!("scene texture descriptor {:?} missing height", resource))?;
    let format = texture.format.ok_or_else(|| {
        format!(
            "scene texture descriptor {:?} missing native format",
            resource
        )
    })?;
    let mip_count = texture
        .mip_count
        .ok_or_else(|| format!("scene texture descriptor {:?} missing mip count", resource))?;
    let payload_bytes = texture.payload_bytes.ok_or_else(|| {
        format!(
            "scene texture descriptor {:?} missing payload byte count",
            resource
        )
    })?;
    if width == 0 || height == 0 || mip_count == 0 {
        return Err(format!(
            "scene texture descriptor {:?} has invalid metadata {width}x{height} mips={mip_count}",
            resource
        ));
    }
    Ok(ResidentTextureDescriptor {
        width,
        height,
        format,
        mip_count,
        payload_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphTarget, SceneMaterialKey,
    };

    #[test]
    fn texture_descriptor_plan_resolves_we_texture_bindings() {
        let graph = mesh_graph(vec![mesh_draw(
            SceneObjectId(7),
            vec![
                SceneGraphResourceBinding {
                    slot: 0,
                    role: SceneGraphResourceRole::shader_texture(0),
                    resource: SceneResourceId(3),
                },
                SceneGraphResourceBinding {
                    slot: 4,
                    role: SceneGraphResourceRole::shader_texture(4),
                    resource: SceneResourceId(5),
                },
            ],
        )]);

        let plan = NativeVulkanSceneTextureDescriptorFramePlan::from_graph(&graph, |resource| {
            matches!(resource, SceneResourceId(3) | SceneResourceId(5)).then_some(
                SceneTextureResidency {
                    id: resource,
                    width: Some(1024),
                    height: Some(512),
                    format: Some(SceneTextureFormat::R8G8B8A8Unorm),
                    mip_count: Some(10),
                    payload_bytes: Some(2_796_204),
                },
            )
        })
        .expect("texture descriptor frame plan");

        assert_eq!(plan.draw_count, 1);
        assert_eq!(plan.binding_count, 2);
        assert_eq!(plan.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(plan.bindings[0].draw_index, 0);
        assert_eq!(plan.bindings[0].object, SceneObjectId(7));
        assert_eq!(
            plan.bindings[0].source,
            NativeVulkanSceneTextureDescriptorSource::ResidentTexture(SceneResourceId(3))
        );
        assert_eq!(
            plan.bindings[0].format,
            NativeVulkanSceneTextureDescriptorFormat::SceneTexture(
                SceneTextureFormat::R8G8B8A8Unorm
            )
        );
        assert_eq!(plan.bindings[0].mip_count, 10);
        assert_eq!(plan.bindings[0].payload_bytes, Some(2_796_204));
        assert_eq!(plan.bindings[0].shader_mapping, "set0.binding0.g_Texture0");
        assert_eq!(plan.bindings[1].slot, 4);
        assert_eq!(plan.bindings[1].shader_mapping, "set0.binding4.g_Texture4");
        assert_eq!(
            plan.command_order,
            [
                "resolve_resident_texture_descriptors",
                "bind_descriptor_heap_texture_mapping"
            ]
        );
    }

    #[test]
    fn texture_descriptor_vk_format_maps_runtime_r8_targets() {
        let format =
            NativeVulkanSceneTextureDescriptorVkFormat::from_vk_format(vk::Format::R8_UNORM)
                .expect("R8 target descriptor format");

        assert_eq!(format, NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm);
        assert_eq!(format.to_vk_format(), vk::Format::R8_UNORM);
    }

    #[test]
    fn texture_descriptor_plan_rejects_genericimage4_without_required_texture0() {
        let graph = mesh_graph(vec![mesh_draw(SceneObjectId(7), Vec::new())]);

        let err = NativeVulkanSceneTextureDescriptorFramePlan::from_graph(&graph, |_| None)
            .expect_err("genericimage4 requires g_Texture0");

        assert!(err.contains("requires texture slots"));
    }

    #[test]
    fn texture_descriptor_plan_rejects_missing_resident_texture() {
        let graph = mesh_graph(vec![mesh_draw(
            SceneObjectId(7),
            vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(3),
            }],
        )]);

        let err = NativeVulkanSceneTextureDescriptorFramePlan::from_graph(&graph, |_| None)
            .expect_err("missing resident texture must fail");

        assert!(err.contains("missing resident scene texture"));
    }

    #[test]
    fn texture_descriptor_plan_rejects_slot_role_mismatch() {
        let graph = mesh_graph(vec![mesh_draw(
            SceneObjectId(7),
            vec![SceneGraphResourceBinding {
                slot: 1,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(3),
            }],
        )]);

        let err = NativeVulkanSceneTextureDescriptorFramePlan::from_graph(&graph, |_| {
            Some(SceneTextureResidency {
                id: SceneResourceId(3),
                width: None,
                height: None,
                format: None,
                mip_count: None,
                payload_bytes: None,
            })
        })
        .expect_err("WE texture slot mismatch must fail");

        assert!(err.contains("does not match WE g_Texture0"));
    }

    #[test]
    fn texture_descriptor_plan_accepts_puppet_skinning_draws() {
        let mut draw = mesh_draw(
            SceneObjectId(7),
            vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(3),
            }],
        );
        draw.pipeline = SceneGraphPipelineClass::PuppetSkinning;
        draw.puppet = Some(crate::engine::scene_engine::ScenePuppetId(9));
        let graph = mesh_graph(vec![draw]);

        let plan = NativeVulkanSceneTextureDescriptorFramePlan::from_graph(&graph, |resource| {
            (resource == SceneResourceId(3)).then_some(SceneTextureResidency {
                id: resource,
                width: Some(1024),
                height: Some(512),
                format: Some(SceneTextureFormat::R8G8B8A8Unorm),
                mip_count: Some(10),
                payload_bytes: Some(2_796_204),
            })
        })
        .expect("puppet draw texture descriptors");

        assert_eq!(plan.draw_count, 1);
        assert_eq!(plan.binding_count, 1);
        assert_eq!(plan.bindings[0].object, SceneObjectId(7));
    }

    #[test]
    fn texture_descriptor_plan_binds_pass_input_target_as_texture0() {
        let mut draw = mesh_draw(SceneObjectId(7), Vec::new());
        draw.resources.clear();
        let graph = SceneGraph {
            passes: vec![SceneGraphPass {
                name: "effect-resolve".to_owned(),
                input: Some(SceneGraphTarget::EffectTarget(0)),
                output: SceneGraphTarget::Swapchain,
                draws: vec![draw],
            }],
        };

        let plan = NativeVulkanSceneTextureDescriptorFramePlan::from_graph_with_target_inputs(
            &graph,
            |_| None,
            |target| {
                Ok(NativeVulkanSceneTargetInputTextureDescriptor {
                    target,
                    width: 3840,
                    height: 2160,
                    format: NativeVulkanSceneTextureDescriptorVkFormat::R16G16B16A16Sfloat,
                })
            },
        )
        .expect("target input descriptor plan");

        assert_eq!(plan.draw_count, 1);
        assert_eq!(plan.binding_count, 1);
        assert_eq!(plan.bindings[0].slot, 0);
        assert_eq!(
            plan.bindings[0].source,
            NativeVulkanSceneTextureDescriptorSource::GraphTarget(SceneGraphTarget::EffectTarget(
                0
            ))
        );
        assert_eq!(plan.bindings[0].width, 3840);
        assert_eq!(plan.bindings[0].height, 2160);
        assert_eq!(
            plan.bindings[0].format,
            NativeVulkanSceneTextureDescriptorFormat::VkFormat(
                NativeVulkanSceneTextureDescriptorVkFormat::R16G16B16A16Sfloat
            )
        );
        assert_eq!(plan.bindings[0].mip_count, 1);
        assert_eq!(plan.bindings[0].payload_bytes, None);
    }

    #[test]
    fn texture_descriptor_plan_rejects_pass_input_texture0_collision() {
        let graph = SceneGraph {
            passes: vec![SceneGraphPass {
                name: "effect-resolve".to_owned(),
                input: Some(SceneGraphTarget::EffectTarget(0)),
                output: SceneGraphTarget::Swapchain,
                draws: vec![mesh_draw(
                    SceneObjectId(7),
                    vec![SceneGraphResourceBinding {
                        slot: 0,
                        role: SceneGraphResourceRole::shader_texture(0),
                        resource: SceneResourceId(3),
                    }],
                )],
            }],
        };

        let err = NativeVulkanSceneTextureDescriptorFramePlan::from_graph_with_target_inputs(
            &graph,
            |resource| {
                Some(SceneTextureResidency {
                    id: resource,
                    width: Some(1024),
                    height: Some(512),
                    format: Some(SceneTextureFormat::R8G8B8A8Unorm),
                    mip_count: Some(10),
                    payload_bytes: Some(2_796_204),
                })
            },
            |target| {
                Ok(NativeVulkanSceneTargetInputTextureDescriptor {
                    target,
                    width: 3840,
                    height: 2160,
                    format: NativeVulkanSceneTextureDescriptorVkFormat::R16G16B16A16Sfloat,
                })
            },
        )
        .expect_err("target input collides with draw texture0");

        assert!(err.contains("collides with WE g_Texture0"));
    }

    fn mesh_graph(draws: Vec<SceneGraphDraw>) -> SceneGraph {
        SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws,
            }],
        }
    }

    fn mesh_draw(
        object: SceneObjectId,
        resources: Vec<SceneGraphResourceBinding>,
    ) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources,
            index_count: 6,
        }
    }
}
