//! Scene texture descriptor binding plans for mesh draws.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use std::collections::BTreeSet;

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneGraph, SceneGraphResourceRole, SceneObjectId, SceneResourceId, SceneTextureFormat,
    SceneTextureResidency,
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
    pub resource: SceneResourceId,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<SceneTextureFormat>,
    pub mip_count: Option<u32>,
    pub payload_bytes: Option<u64>,
    pub shader_mapping: String,
}

impl NativeVulkanSceneTextureDescriptorFramePlan {
    pub(in crate::renderer::native_vulkan) fn from_graph<TextureResidency>(
        graph: &SceneGraph,
        mut texture_residency: TextureResidency,
    ) -> Result<Self, String>
    where
        TextureResidency: FnMut(SceneResourceId) -> Option<SceneTextureResidency>,
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

                let _ = draw.shader_texture_slot_mask()?;
                let mut used_slots = BTreeSet::new();
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
                    bindings.push(NativeVulkanSceneTextureDescriptorBinding {
                        draw_index,
                        object: draw.object,
                        slot: resource.slot,
                        role: resource.role,
                        resource: resource.resource,
                        width: texture.width,
                        height: texture.height,
                        format: texture.format,
                        mip_count: texture.mip_count,
                        payload_bytes: texture.payload_bytes,
                        shader_mapping: scene_shader_texture_mapping(resource.slot),
                    });
                }
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
        assert_eq!(plan.bindings[0].resource, SceneResourceId(3));
        assert_eq!(
            plan.bindings[0].format,
            Some(SceneTextureFormat::R8G8B8A8Unorm)
        );
        assert_eq!(plan.bindings[0].mip_count, Some(10));
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
