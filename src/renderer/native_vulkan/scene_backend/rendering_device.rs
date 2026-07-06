//! Vulkan implementation of the scene RenderingDevice boundary.
//!
//! References:
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`

use crate::engine::scene_engine::{
    RenderingDevice, RenderingDeviceCommand, SceneFramePlan, SceneGraph, SceneGraphDraw,
    SceneGraphPass, SceneResidentResource, SceneResourceResidencyPlan,
};

#[derive(Debug, Default)]
pub struct NativeVulkanRenderingDevice {
    commands: Vec<RenderingDeviceCommand>,
}

impl NativeVulkanRenderingDevice {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_commands(self) -> Vec<RenderingDeviceCommand> {
        self.commands
    }

    fn record_residency_plan(&mut self, residency: &SceneResourceResidencyPlan) {
        for resource in &residency.resources {
            match resource {
                SceneResidentResource::Texture(texture) => {
                    self.commands
                        .push(RenderingDeviceCommand::EnsureTextureResident {
                            resource: texture.id,
                            width: texture.width,
                            height: texture.height,
                        });
                }
                SceneResidentResource::Buffer(buffer) => {
                    self.commands
                        .push(RenderingDeviceCommand::EnsureBufferResident {
                            resource: buffer.id,
                            bytes: buffer.bytes,
                        });
                }
                SceneResidentResource::MeshGeometry(mesh) => {
                    self.commands
                        .push(RenderingDeviceCommand::EnsureMeshGeometryResident {
                            geometry: mesh.id,
                            source_record: mesh.source_record,
                            vertex_count: mesh.vertex_count,
                            index_count: mesh.index_count,
                            vertex_bytes: mesh.vertex_bytes,
                            index_bytes: mesh.index_bytes,
                        });
                }
                SceneResidentResource::PuppetRig(puppet) => {
                    self.commands
                        .push(RenderingDeviceCommand::EnsurePuppetRigResident {
                            puppet: puppet.id,
                            source_record: puppet.source_record,
                            bone_count: puppet.bone_count,
                            skin_vertex_count: puppet.skin_vertex_count,
                            attachment_count: puppet.attachment_count,
                            clip_count: puppet.clip_count,
                            clip_bone_count: puppet.clip_bone_count,
                            clip_frame_count: puppet.clip_frame_count,
                            clip_frame_bytes: puppet.clip_frame_bytes,
                            layer_count: puppet.layer_count,
                            clipping_record_count: puppet.clipping_record_count,
                            clipping_bone_count: puppet.clipping_bone_count,
                            clipping_frame_key_count: puppet.clipping_frame_key_count,
                        });
                }
            }
        }
    }

    fn record_graph(&mut self, graph: &SceneGraph) {
        for pass in &graph.passes {
            self.record_pass(pass);
        }
    }

    fn record_pass(&mut self, pass: &SceneGraphPass) {
        self.commands.push(RenderingDeviceCommand::BeginPass {
            name: pass.name.clone(),
        });
        for draw in &pass.draws {
            self.record_draw(draw);
        }
        self.commands.push(RenderingDeviceCommand::EndPass);
    }

    fn record_draw(&mut self, draw: &SceneGraphDraw) {
        self.commands.push(RenderingDeviceCommand::BindPipeline {
            name: format!(
                "{}::{:?}::{:?}",
                draw.material.shader, draw.material.blend, draw.pipeline
            ),
        });
        for resource in &draw.resources {
            self.commands.push(RenderingDeviceCommand::BindTexture {
                slot: resource.slot,
                resource: resource.resource,
            });
        }
        self.commands.push(RenderingDeviceCommand::DrawIndexed {
            object: draw.object,
            geometry: draw.geometry,
            puppet: draw.puppet,
            index_count: draw.index_count,
        });
    }
}

impl RenderingDevice for NativeVulkanRenderingDevice {
    fn record_scene_frame(&mut self, frame: &SceneFramePlan) {
        self.commands.clear();
        self.record_residency_plan(&frame.residency);
        self.record_graph(&frame.graph);
    }

    fn record_resource_residency(&mut self, residency: &SceneResourceResidencyPlan) {
        self.record_residency_plan(residency);
    }

    fn record_scene_graph(&mut self, graph: &SceneGraph) {
        self.commands.clear();
        self.record_graph(graph);
    }

    fn commands(&self) -> &[RenderingDeviceCommand] {
        &self.commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneFramePlan, SceneGeometryId, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneGraphTarget, SceneMaterialKey, SceneMeshResidency,
        SceneObjectId, SceneResidentResource, SceneResourceResidencyPlan,
    };

    #[test]
    fn device_command_keeps_geometry_handle() {
        let graph = SceneGraph {
            passes: vec![SceneGraphPass {
                name: "mesh".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws: vec![SceneGraphDraw {
                    object: SceneObjectId(4),
                    pipeline: SceneGraphPipelineClass::Mesh,
                    material: SceneMaterialKey {
                        shader: "we/genericimage4".to_owned(),
                        blend: SceneBlendContract::TranslucentAlpha,
                        writes_depth: false,
                        tests_depth: false,
                    },
                    geometry: Some(SceneGeometryId(2)),
                    puppet: None,
                    resources: Vec::new(),
                    index_count: 12,
                }],
            }],
        };
        let mut device = NativeVulkanRenderingDevice::new();
        device.record_scene_graph(&graph);

        assert!(matches!(
            device.commands()[2],
            RenderingDeviceCommand::DrawIndexed {
                object: SceneObjectId(4),
                geometry: Some(SceneGeometryId(2)),
                puppet: None,
                index_count: 12,
            }
        ));
    }

    #[test]
    fn frame_recording_ensures_mesh_residency_before_draw() {
        let frame = SceneFramePlan {
            residency: SceneResourceResidencyPlan {
                resources: vec![SceneResidentResource::MeshGeometry(SceneMeshResidency {
                    id: SceneGeometryId(2),
                    source_record: 12,
                    vertex_count: 4,
                    index_count: 6,
                    vertex_bytes: 160,
                    index_bytes: 24,
                })],
            },
            graph: SceneGraph {
                passes: vec![SceneGraphPass {
                    name: "mesh".to_owned(),
                    input: None,
                    output: SceneGraphTarget::Swapchain,
                    draws: vec![SceneGraphDraw {
                        object: SceneObjectId(4),
                        pipeline: SceneGraphPipelineClass::Mesh,
                        material: SceneMaterialKey {
                            shader: "we/genericimage4".to_owned(),
                            blend: SceneBlendContract::TranslucentAlpha,
                            writes_depth: false,
                            tests_depth: false,
                        },
                        geometry: Some(SceneGeometryId(2)),
                        puppet: None,
                        resources: Vec::new(),
                        index_count: 6,
                    }],
                }],
            },
        };
        let mut device = NativeVulkanRenderingDevice::new();
        device.record_scene_frame(&frame);

        assert!(matches!(
            device.commands()[0],
            RenderingDeviceCommand::EnsureMeshGeometryResident {
                geometry: SceneGeometryId(2),
                source_record: 12,
                vertex_count: 4,
                index_count: 6,
                ..
            }
        ));
        assert!(matches!(
            device.commands()[3],
            RenderingDeviceCommand::DrawIndexed {
                geometry: Some(SceneGeometryId(2)),
                index_count: 6,
                ..
            }
        ));
    }
}
