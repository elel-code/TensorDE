//! Native Vulkan scene resource residency storage.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::collections::{BTreeMap, BTreeSet};

use crate::engine::scene_engine::{
    RenderingDeviceCommand, SceneBufferResidency, SceneGeometryId, SceneMeshResidency,
    ScenePuppetId, ScenePuppetRigResidency, SceneResidentResource, SceneResourceId,
    SceneResourceResidencyPlan, SceneTextureResidency,
};

#[derive(Debug, Default)]
pub struct NativeVulkanSceneResourceStorage {
    textures: BTreeMap<SceneResourceId, SceneTextureResidency>,
    buffers: BTreeMap<SceneResourceId, SceneBufferResidency>,
    mesh_geometries: BTreeMap<SceneGeometryId, SceneMeshResidency>,
    puppet_rigs: BTreeMap<ScenePuppetId, ScenePuppetRigResidency>,
}

impl NativeVulkanSceneResourceStorage {
    pub fn sync_residency_plan(
        &mut self,
        residency: &SceneResourceResidencyPlan,
    ) -> Vec<RenderingDeviceCommand> {
        let active = NativeVulkanSceneActiveResources::from_residency_plan(residency);
        let mut commands = self.release_stale_resources(&active);
        commands.extend(self.ensure_active_resources(residency));
        commands
    }

    pub fn texture(&self, id: SceneResourceId) -> Option<&SceneTextureResidency> {
        self.textures.get(&id)
    }

    pub fn buffer(&self, id: SceneResourceId) -> Option<&SceneBufferResidency> {
        self.buffers.get(&id)
    }

    pub fn mesh_geometry(&self, id: SceneGeometryId) -> Option<&SceneMeshResidency> {
        self.mesh_geometries.get(&id)
    }

    pub fn puppet_rig(&self, id: ScenePuppetId) -> Option<&ScenePuppetRigResidency> {
        self.puppet_rigs.get(&id)
    }

    fn release_stale_resources(
        &mut self,
        active: &NativeVulkanSceneActiveResources,
    ) -> Vec<RenderingDeviceCommand> {
        let mut commands = Vec::new();
        self.textures.retain(|id, _| {
            let keep = active.textures.contains(id);
            if !keep {
                commands.push(RenderingDeviceCommand::ReleaseTextureResident { resource: *id });
            }
            keep
        });
        self.buffers.retain(|id, _| {
            let keep = active.buffers.contains(id);
            if !keep {
                commands.push(RenderingDeviceCommand::ReleaseBufferResident { resource: *id });
            }
            keep
        });
        self.mesh_geometries.retain(|id, _| {
            let keep = active.mesh_geometries.contains(id);
            if !keep {
                commands
                    .push(RenderingDeviceCommand::ReleaseMeshGeometryResident { geometry: *id });
            }
            keep
        });
        self.puppet_rigs.retain(|id, _| {
            let keep = active.puppet_rigs.contains(id);
            if !keep {
                commands.push(RenderingDeviceCommand::ReleasePuppetRigResident { puppet: *id });
            }
            keep
        });
        commands
    }

    fn ensure_active_resources(
        &mut self,
        residency: &SceneResourceResidencyPlan,
    ) -> Vec<RenderingDeviceCommand> {
        let mut commands = Vec::new();
        for resource in &residency.resources {
            match *resource {
                SceneResidentResource::Texture(texture) => {
                    if self.textures.get(&texture.id) != Some(&texture) {
                        self.textures.insert(texture.id, texture);
                        commands.push(RenderingDeviceCommand::EnsureTextureResident {
                            resource: texture.id,
                            width: texture.width,
                            height: texture.height,
                        });
                    }
                }
                SceneResidentResource::Buffer(buffer) => {
                    if self.buffers.get(&buffer.id) != Some(&buffer) {
                        self.buffers.insert(buffer.id, buffer);
                        commands.push(RenderingDeviceCommand::EnsureBufferResident {
                            resource: buffer.id,
                            bytes: buffer.bytes,
                        });
                    }
                }
                SceneResidentResource::MeshGeometry(mesh) => {
                    if self.mesh_geometries.get(&mesh.id) != Some(&mesh) {
                        self.mesh_geometries.insert(mesh.id, mesh);
                        commands.push(RenderingDeviceCommand::EnsureMeshGeometryResident {
                            geometry: mesh.id,
                            source_record: mesh.source_record,
                            vertex_count: mesh.vertex_count,
                            index_count: mesh.index_count,
                            vertex_bytes: mesh.vertex_bytes,
                            index_bytes: mesh.index_bytes,
                        });
                    }
                }
                SceneResidentResource::PuppetRig(puppet) => {
                    if self.puppet_rigs.get(&puppet.id) != Some(&puppet) {
                        self.puppet_rigs.insert(puppet.id, puppet);
                        commands.push(RenderingDeviceCommand::EnsurePuppetRigResident {
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
        commands
    }
}

#[derive(Debug, Default)]
struct NativeVulkanSceneActiveResources {
    textures: BTreeSet<SceneResourceId>,
    buffers: BTreeSet<SceneResourceId>,
    mesh_geometries: BTreeSet<SceneGeometryId>,
    puppet_rigs: BTreeSet<ScenePuppetId>,
}

impl NativeVulkanSceneActiveResources {
    fn from_residency_plan(residency: &SceneResourceResidencyPlan) -> Self {
        let mut active = Self::default();
        for resource in &residency.resources {
            match *resource {
                SceneResidentResource::Texture(texture) => {
                    active.textures.insert(texture.id);
                }
                SceneResidentResource::Buffer(buffer) => {
                    active.buffers.insert(buffer.id);
                }
                SceneResidentResource::MeshGeometry(mesh) => {
                    active.mesh_geometries.insert(mesh.id);
                }
                SceneResidentResource::PuppetRig(puppet) => {
                    active.puppet_rigs.insert(puppet.id);
                }
            }
        }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_skips_unchanged_mesh_residency_and_releases_stale_handles() {
        let mut storage = NativeVulkanSceneResourceStorage::default();
        let residency = SceneResourceResidencyPlan {
            resources: vec![SceneResidentResource::MeshGeometry(SceneMeshResidency {
                id: SceneGeometryId(2),
                source_record: 12,
                vertex_count: 4,
                index_count: 6,
                vertex_bytes: 160,
                index_bytes: 24,
            })],
        };

        let first = storage.sync_residency_plan(&residency);
        assert!(matches!(
            first.as_slice(),
            [RenderingDeviceCommand::EnsureMeshGeometryResident {
                geometry: SceneGeometryId(2),
                vertex_count: 4,
                index_count: 6,
                ..
            }]
        ));
        assert!(storage.mesh_geometry(SceneGeometryId(2)).is_some());

        let second = storage.sync_residency_plan(&residency);
        assert!(second.is_empty());

        let third = storage.sync_residency_plan(&SceneResourceResidencyPlan::default());
        assert!(matches!(
            third.as_slice(),
            [RenderingDeviceCommand::ReleaseMeshGeometryResident {
                geometry: SceneGeometryId(2)
            }]
        ));
        assert!(storage.mesh_geometry(SceneGeometryId(2)).is_none());
    }
}
