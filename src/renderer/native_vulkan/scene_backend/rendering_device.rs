//! Vulkan implementation of the scene RenderingDevice boundary.
//!
//! References:
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device.h`

use crate::engine::scene_engine::{
    RenderingDevice, RenderingDeviceCommand, SceneFramePlan, SceneGeometryId, SceneGraph,
    SceneGraphDraw, SceneGraphPass, ScenePuppetId, SceneResource, SceneResourceResidencyPlan,
};

use super::draw_command::NativeVulkanSceneMeshDrawCommandPlan;
use super::pass_command::NativeVulkanSceneMeshPassCommandPlan;
use super::resource_buffers::{
    NativeVulkanSceneGpuBufferCatalog, NativeVulkanSceneGpuBufferSyncAction,
    NativeVulkanSceneMeshDrawBufferRecords, NativeVulkanScenePuppetStorageBufferRecords,
};
use super::resource_storage::NativeVulkanSceneResourceStorage;
use super::resource_upload::NativeVulkanSceneGpuUploadPlan;

#[derive(Debug, Default)]
pub struct NativeVulkanRenderingDevice {
    commands: Vec<RenderingDeviceCommand>,
    resource_storage: NativeVulkanSceneResourceStorage,
    gpu_buffer_catalog: NativeVulkanSceneGpuBufferCatalog,
}

impl NativeVulkanRenderingDevice {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_commands(self) -> Vec<RenderingDeviceCommand> {
        self.commands
    }

    pub fn resource_storage(&self) -> &NativeVulkanSceneResourceStorage {
        &self.resource_storage
    }

    pub fn gpu_buffer_actions(&self) -> &[NativeVulkanSceneGpuBufferSyncAction] {
        self.gpu_buffer_catalog.last_actions()
    }

    pub fn mesh_draw_buffer_records(
        &self,
        geometry: SceneGeometryId,
    ) -> Result<NativeVulkanSceneMeshDrawBufferRecords, String> {
        self.gpu_buffer_catalog.mesh_draw_buffer_records(geometry)
    }

    pub fn puppet_storage_buffer_records(
        &self,
        puppet: ScenePuppetId,
    ) -> NativeVulkanScenePuppetStorageBufferRecords {
        self.gpu_buffer_catalog
            .puppet_storage_buffer_records(puppet)
    }

    pub fn mesh_draw_command_plan(
        &self,
        draw: &SceneGraphDraw,
    ) -> Result<NativeVulkanSceneMeshDrawCommandPlan, String> {
        let geometry = draw
            .geometry
            .ok_or_else(|| "scene mesh draw command requires geometry handle".to_owned())?;
        let records = self.mesh_draw_buffer_records(geometry)?;
        NativeVulkanSceneMeshDrawCommandPlan::from_record_bindings(draw, &records)
    }

    pub fn mesh_pass_command_plan<'a>(
        &self,
        pass: &'a SceneGraphPass,
    ) -> Result<NativeVulkanSceneMeshPassCommandPlan<'a>, String> {
        NativeVulkanSceneMeshPassCommandPlan::from_record_bindings(pass, |geometry| {
            self.mesh_draw_buffer_records(geometry)
        })
    }

    pub fn sync_scene_gpu_uploads(
        &mut self,
        resources: &[SceneResource],
    ) -> Result<&[NativeVulkanSceneGpuBufferSyncAction], String> {
        let upload_plan = NativeVulkanSceneGpuUploadPlan::from_resident_resources(
            &self.resource_storage,
            resources,
        )
        .map_err(|err| err.to_string())?;
        self.gpu_buffer_catalog
            .sync_upload_plan(&upload_plan)
            .map_err(|err| err.to_string())
    }

    fn record_residency_plan(&mut self, residency: &SceneResourceResidencyPlan) {
        self.commands
            .extend(self.resource_storage.sync_residency_plan(residency));
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
    use super::super::resource_buffers::NativeVulkanSceneGpuBufferSyncAction;
    use super::*;
    use crate::core::scene::SceneMeshVertex;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneFramePlan, SceneGeometryId, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneGraphTarget, SceneMaterialKey, SceneMeshResidency,
        SceneObjectId, SceneResidentResource, SceneResource, SceneResourceResidencyPlan,
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
                    vertex_bytes: 80,
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

    #[test]
    fn frame_recording_reuses_unchanged_mesh_residency() {
        let frame = SceneFramePlan {
            residency: SceneResourceResidencyPlan {
                resources: vec![SceneResidentResource::MeshGeometry(SceneMeshResidency {
                    id: SceneGeometryId(2),
                    source_record: 12,
                    vertex_count: 4,
                    index_count: 6,
                    vertex_bytes: 80,
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
        device.record_scene_frame(&frame);

        assert!(
            device
                .resource_storage()
                .mesh_geometry(SceneGeometryId(2))
                .is_some()
        );
        assert!(!device.commands().iter().any(|command| matches!(
            command,
            RenderingDeviceCommand::EnsureMeshGeometryResident { .. }
        )));
        assert!(matches!(
            device.commands()[2],
            RenderingDeviceCommand::DrawIndexed {
                geometry: Some(SceneGeometryId(2)),
                index_count: 6,
                ..
            }
        ));
    }

    #[test]
    fn device_syncs_scene_gpu_uploads_after_residency() {
        let resources = vec![SceneResource::MeshGeometry {
            id: SceneGeometryId(2),
            source_record: 12,
            vertices: vec![SceneMeshVertex::default(); 2],
            indices: vec![0, 1, 0],
        }];
        let residency = SceneResourceResidencyPlan::from_resources(&resources);
        let mut device = NativeVulkanRenderingDevice::new();
        device.record_resource_residency(&residency);

        let first = device.sync_scene_gpu_uploads(&resources).unwrap().to_vec();
        let second = device.sync_scene_gpu_uploads(&resources).unwrap().to_vec();

        assert!(matches!(
            first.as_slice(),
            [
                NativeVulkanSceneGpuBufferSyncAction::Create { .. },
                NativeVulkanSceneGpuBufferSyncAction::Create { .. }
            ]
        ));
        assert!(matches!(
            second.as_slice(),
            [
                NativeVulkanSceneGpuBufferSyncAction::Reuse { .. },
                NativeVulkanSceneGpuBufferSyncAction::Reuse { .. }
            ]
        ));

        let mesh_records = device
            .mesh_draw_buffer_records(SceneGeometryId(2))
            .expect("mesh draw records after GPU upload sync");
        assert_eq!(mesh_records.vertex.bytes, 40);
        assert_eq!(mesh_records.index.bytes, 12);

        let draw = SceneGraphDraw {
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
            index_count: 3,
        };
        let draw_plan = device
            .mesh_draw_command_plan(&draw)
            .expect("mesh draw command plan after GPU upload sync");
        assert_eq!(
            draw_plan.command_order,
            vec![
                "cmd_bind_vertex_buffers",
                "cmd_bind_index_buffer",
                "cmd_draw_indexed"
            ]
        );

        let pass = SceneGraphPass {
            name: "mesh-main".to_owned(),
            input: None,
            output: SceneGraphTarget::Swapchain,
            draws: vec![
                draw.clone(),
                SceneGraphDraw {
                    object: SceneObjectId(5),
                    ..draw
                },
            ],
        };
        let pass_plan = device
            .mesh_pass_command_plan(&pass)
            .expect("mesh pass command plan after GPU upload sync");
        assert_eq!(pass_plan.pipeline_bind_count, 1);
        assert_eq!(pass_plan.indexed_draw_count, 2);
    }
}
