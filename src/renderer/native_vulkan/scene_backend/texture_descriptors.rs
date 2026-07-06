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
    SceneGraph, SceneGraphPipelineClass, SceneGraphResourceRole, SceneObjectId, SceneResourceId,
    SceneTextureFormat, SceneTextureResidency,
};

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
    pub object: SceneObjectId,
    pub slot: u32,
    pub role: SceneGraphResourceRole,
    pub resource: SceneResourceId,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<SceneTextureFormat>,
    pub mip_count: Option<u32>,
    pub payload_bytes: Option<u64>,
    pub shader_mapping: &'static str,
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
        let mut draw_count = 0usize;

        for pass in &graph.passes {
            for draw in &pass.draws {
                if draw.pipeline != SceneGraphPipelineClass::Mesh {
                    return Err(format!(
                        "scene texture descriptor plan requires Mesh pipeline, got {:?} for object {:?}",
                        draw.pipeline, draw.object
                    ));
                }

                draw_count += 1;
                let mut used_slots = BTreeSet::new();
                for resource in &draw.resources {
                    if resource.role != SceneGraphResourceRole::BaseColor {
                        return Err(format!(
                            "scene texture descriptor plan only supports BaseColor, got {:?} for object {:?}",
                            resource.role, draw.object
                        ));
                    }
                    if resource.slot != 0 {
                        return Err(format!(
                            "scene texture descriptor plan requires BaseColor at slot 0, got slot {} for object {:?}",
                            resource.slot, draw.object
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
                        object: draw.object,
                        slot: resource.slot,
                        role: resource.role,
                        resource: resource.resource,
                        width: texture.width,
                        height: texture.height,
                        format: texture.format,
                        mip_count: texture.mip_count,
                        payload_bytes: texture.payload_bytes,
                        shader_mapping: "set0.binding0.base_color",
                    });
                }
            }
        }

        Ok(Self {
            draw_count,
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
    fn texture_descriptor_plan_resolves_base_color_texture_binding() {
        let graph = mesh_graph(vec![mesh_draw(
            SceneObjectId(7),
            Some(SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::BaseColor,
                resource: SceneResourceId(3),
            }),
        )]);

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
        .expect("texture descriptor frame plan");

        assert_eq!(plan.draw_count, 1);
        assert_eq!(plan.binding_count, 1);
        assert_eq!(plan.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(plan.bindings[0].object, SceneObjectId(7));
        assert_eq!(plan.bindings[0].resource, SceneResourceId(3));
        assert_eq!(
            plan.bindings[0].format,
            Some(SceneTextureFormat::R8G8B8A8Unorm)
        );
        assert_eq!(plan.bindings[0].mip_count, Some(10));
        assert_eq!(plan.bindings[0].payload_bytes, Some(2_796_204));
        assert_eq!(plan.bindings[0].shader_mapping, "set0.binding0.base_color");
        assert_eq!(
            plan.command_order,
            [
                "resolve_resident_texture_descriptors",
                "bind_descriptor_heap_texture_mapping"
            ]
        );
    }

    #[test]
    fn texture_descriptor_plan_allows_mesh_draw_without_texture_binding() {
        let graph = mesh_graph(vec![mesh_draw(SceneObjectId(7), None)]);

        let plan = NativeVulkanSceneTextureDescriptorFramePlan::from_graph(&graph, |_| None)
            .expect("texture-free mesh descriptor frame plan");

        assert_eq!(plan.draw_count, 1);
        assert_eq!(plan.binding_count, 0);
        assert!(plan.bindings.is_empty());
    }

    #[test]
    fn texture_descriptor_plan_rejects_missing_resident_texture() {
        let graph = mesh_graph(vec![mesh_draw(
            SceneObjectId(7),
            Some(SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::BaseColor,
                resource: SceneResourceId(3),
            }),
        )]);

        let err = NativeVulkanSceneTextureDescriptorFramePlan::from_graph(&graph, |_| None)
            .expect_err("missing resident texture must fail");

        assert!(err.contains("missing resident scene texture"));
    }

    #[test]
    fn texture_descriptor_plan_rejects_nonzero_base_color_slot() {
        let graph = mesh_graph(vec![mesh_draw(
            SceneObjectId(7),
            Some(SceneGraphResourceBinding {
                slot: 1,
                role: SceneGraphResourceRole::BaseColor,
                resource: SceneResourceId(3),
            }),
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
        .expect_err("BaseColor slot mismatch must fail");

        assert!(err.contains("slot 0"));
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
        resource: Option<SceneGraphResourceBinding>,
    ) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                writes_depth: false,
                tests_depth: false,
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources: resource.into_iter().collect(),
            index_count: 6,
        }
    }
}
