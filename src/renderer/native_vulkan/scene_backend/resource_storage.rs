//! Native Vulkan scene resource residency storage.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `references/godot/servers/rendering/storage/`
//! - `references/godot/servers/rendering/rendering_device.h`

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::engine::scene_engine::{
    RenderingDeviceCommand, SceneBufferResidency, SceneGeometryId,
    SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency, SceneMeshResidency, SceneObjectId,
    ScenePuppetId, ScenePuppetRigResidency, SceneResidentResource, SceneResourceId,
    SceneResourceResidencyPlan, SceneTextureResidency,
};

#[derive(Debug, Default)]
pub struct NativeVulkanSceneResourceStorage {
    textures: BTreeMap<SceneResourceId, SceneTextureResidency>,
    buffers: BTreeMap<SceneResourceId, SceneBufferResidency>,
    mesh_geometries: BTreeMap<SceneGeometryId, SceneMeshResidency>,
    layer_alpha_mask_rt_method8_mdlv_geometries: BTreeMap<
        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
        SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency,
    >,
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

    pub fn texture_residencies(&self) -> impl Iterator<Item = &SceneTextureResidency> {
        self.textures.values()
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

    pub fn layer_alpha_mask_rt_method8_mdlv_geometry(
        &self,
        key: NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry,
    ) -> Option<&SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency> {
        self.layer_alpha_mask_rt_method8_mdlv_geometries.get(&key)
    }

    pub fn gpu_buffer_requirements(&self) -> Vec<NativeVulkanSceneGpuBufferRequirement> {
        let mut requirements = Vec::new();
        for mesh in self.mesh_geometries.values() {
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::MeshGeometry(mesh.id),
                NativeVulkanSceneGpuBufferRole::MeshVertex,
                mesh.vertex_bytes,
            );
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::MeshGeometry(mesh.id),
                NativeVulkanSceneGpuBufferRole::MeshIndex,
                mesh.index_bytes,
            );
        }
        for puppet in self.puppet_rigs.values() {
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet.id),
                NativeVulkanSceneGpuBufferRole::PuppetBone,
                puppet.bone_bytes,
            );
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet.id),
                NativeVulkanSceneGpuBufferRole::PuppetSkinVertex,
                puppet.skin_vertex_bytes,
            );
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet.id),
                NativeVulkanSceneGpuBufferRole::PuppetClipFrame,
                puppet.clip_frame_bytes,
            );
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet.id),
                NativeVulkanSceneGpuBufferRole::PuppetClippingRecord,
                puppet.clipping_record_bytes,
            );
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet.id),
                NativeVulkanSceneGpuBufferRole::PuppetClippingBoneIndex,
                puppet.clipping_bone_bytes,
            );
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet.id),
                NativeVulkanSceneGpuBufferRole::PuppetClippingFrameKey,
                puppet.clipping_frame_key_bytes,
            );
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::PuppetRig(puppet.id),
                NativeVulkanSceneGpuBufferRole::PuppetActiveSource,
                puppet.active_source_bytes,
            );
        }
        for geometry in self.layer_alpha_mask_rt_method8_mdlv_geometries.values() {
            let key = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
                object: geometry.object,
                entry_owner_index: geometry.entry_owner_index,
            };
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(key),
                NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex,
                geometry.vertex_bytes,
            );
            push_gpu_buffer_requirement(
                &mut requirements,
                NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(key),
                NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex,
                geometry.index_bytes,
            );
        }
        requirements
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
        self.layer_alpha_mask_rt_method8_mdlv_geometries
            .retain(|key, _| {
                let keep = active
                    .layer_alpha_mask_rt_method8_mdlv_geometries
                    .contains(key);
                if !keep {
                    commands.push(
                        RenderingDeviceCommand::ReleaseLayerAlphaMaskRtMethod8MdlvGeometryResident {
                            object: key.object,
                            entry_owner_index: key.entry_owner_index,
                        },
                    );
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
                SceneResidentResource::LayerAlphaMaskRtMethod8MdlvGeometry(geometry) => {
                    let key = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
                        object: geometry.object,
                        entry_owner_index: geometry.entry_owner_index,
                    };
                    if self.layer_alpha_mask_rt_method8_mdlv_geometries.get(&key) != Some(&geometry)
                    {
                        self.layer_alpha_mask_rt_method8_mdlv_geometries
                            .insert(key, geometry);
                        commands.push(
                            RenderingDeviceCommand::EnsureLayerAlphaMaskRtMethod8MdlvGeometryResident {
                                object: geometry.object,
                                entry_owner_index: geometry.entry_owner_index,
                                layout_key: geometry.layout_key,
                                vertex_stride_bytes: geometry.vertex_stride_bytes,
                                vertex_count: geometry.vertex_count,
                                index_count: geometry.index_count,
                                vertex_bytes: geometry.vertex_bytes,
                                index_bytes: geometry.index_bytes,
                            },
                        );
                    }
                }
                SceneResidentResource::PuppetRig(puppet) => {
                    if self.puppet_rigs.get(&puppet.id) != Some(&puppet) {
                        self.puppet_rigs.insert(puppet.id, puppet);
                        commands.push(RenderingDeviceCommand::EnsurePuppetRigResident {
                            puppet: puppet.id,
                            source_record: puppet.source_record,
                            bone_count: puppet.bone_count,
                            bone_bytes: puppet.bone_bytes,
                            skin_vertex_count: puppet.skin_vertex_count,
                            skin_vertex_bytes: puppet.skin_vertex_bytes,
                            attachment_count: puppet.attachment_count,
                            clip_count: puppet.clip_count,
                            clip_bone_count: puppet.clip_bone_count,
                            clip_frame_count: puppet.clip_frame_count,
                            clip_frame_bytes: puppet.clip_frame_bytes,
                            layer_count: puppet.layer_count,
                            clipping_record_count: puppet.clipping_record_count,
                            clipping_record_bytes: puppet.clipping_record_bytes,
                            clipping_bone_count: puppet.clipping_bone_count,
                            clipping_bone_bytes: puppet.clipping_bone_bytes,
                            clipping_frame_key_count: puppet.clipping_frame_key_count,
                            clipping_frame_key_bytes: puppet.clipping_frame_key_bytes,
                            active_source_count: puppet.active_source_count,
                            active_source_bytes: puppet.active_source_bytes,
                        });
                    }
                }
            }
        }
        commands
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum NativeVulkanSceneGpuBufferOwner {
    MeshGeometry(SceneGeometryId),
    PuppetRig(ScenePuppetId),
    RenderStateUtility(NativeVulkanSceneRenderStateUtilityGeometry),
    LayerAlphaMaskRtMethod8MdlvEntry(NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum NativeVulkanSceneGpuBufferRole {
    MeshVertex,
    MeshIndex,
    RenderStateFlatTextureVertex,
    LayerAlphaMaskRtMethod8MdlvVertex,
    LayerAlphaMaskRtMethod8MdlvIndex,
    PuppetBone,
    PuppetSkinVertex,
    PuppetClipFrame,
    PuppetClippingRecord,
    PuppetClippingBoneIndex,
    PuppetClippingFrameKey,
    PuppetActiveSource,
}

impl NativeVulkanSceneGpuBufferRole {
    pub fn usage(self) -> NativeVulkanSceneGpuBufferUsage {
        match self {
            Self::MeshVertex
            | Self::RenderStateFlatTextureVertex
            | Self::LayerAlphaMaskRtMethod8MdlvVertex => NativeVulkanSceneGpuBufferUsage::Vertex,
            Self::MeshIndex | Self::LayerAlphaMaskRtMethod8MdlvIndex => {
                NativeVulkanSceneGpuBufferUsage::Index
            }
            Self::PuppetBone
            | Self::PuppetSkinVertex
            | Self::PuppetClipFrame
            | Self::PuppetClippingRecord
            | Self::PuppetClippingBoneIndex
            | Self::PuppetClippingFrameKey
            | Self::PuppetActiveSource => NativeVulkanSceneGpuBufferUsage::Storage,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum NativeVulkanSceneRenderStateUtilityGeometry {
    LayerAlphaMaskCopyBackState48,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
    pub object: SceneObjectId,
    pub entry_owner_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct NativeVulkanSceneGpuBufferRequirement {
    pub owner: NativeVulkanSceneGpuBufferOwner,
    pub role: NativeVulkanSceneGpuBufferRole,
    pub bytes: u64,
    pub usage: NativeVulkanSceneGpuBufferUsage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum NativeVulkanSceneGpuBufferUsage {
    Vertex,
    Index,
    Storage,
}

fn push_gpu_buffer_requirement(
    requirements: &mut Vec<NativeVulkanSceneGpuBufferRequirement>,
    owner: NativeVulkanSceneGpuBufferOwner,
    role: NativeVulkanSceneGpuBufferRole,
    bytes: u64,
) {
    if bytes == 0 {
        return;
    }
    requirements.push(NativeVulkanSceneGpuBufferRequirement {
        owner,
        role,
        bytes,
        usage: role.usage(),
    });
}

#[derive(Debug, Default)]
struct NativeVulkanSceneActiveResources {
    textures: BTreeSet<SceneResourceId>,
    buffers: BTreeSet<SceneResourceId>,
    mesh_geometries: BTreeSet<SceneGeometryId>,
    layer_alpha_mask_rt_method8_mdlv_geometries:
        BTreeSet<NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry>,
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
                SceneResidentResource::LayerAlphaMaskRtMethod8MdlvGeometry(geometry) => {
                    active.layer_alpha_mask_rt_method8_mdlv_geometries.insert(
                        NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
                            object: geometry.object,
                            entry_owner_index: geometry.entry_owner_index,
                        },
                    );
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
                vertex_bytes: 80,
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

    #[test]
    fn storage_exposes_mesh_and_puppet_gpu_buffer_requirements() {
        let mut storage = NativeVulkanSceneResourceStorage::default();
        let mdlv_geometry = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
            object: SceneObjectId(1530),
            entry_owner_index: 0,
        };
        let residency = SceneResourceResidencyPlan {
            resources: vec![
                SceneResidentResource::MeshGeometry(SceneMeshResidency {
                    id: SceneGeometryId(2),
                    source_record: 12,
                    vertex_count: 4,
                    index_count: 6,
                    vertex_bytes: 80,
                    index_bytes: 24,
                }),
                SceneResidentResource::PuppetRig(ScenePuppetRigResidency {
                    id: ScenePuppetId(3),
                    source_record: 4,
                    bone_count: 2,
                    bone_bytes: 128,
                    skin_vertex_count: 4,
                    skin_vertex_bytes: 128,
                    attachment_count: 0,
                    clip_count: 1,
                    clip_bone_count: 2,
                    clip_frame_count: 10,
                    clip_frame_bytes: 480,
                    layer_count: 1,
                    clipping_record_count: 1,
                    clipping_record_bytes: 48,
                    clipping_bone_count: 2,
                    clipping_bone_bytes: 8,
                    clipping_frame_key_count: 3,
                    clipping_frame_key_bytes: 12,
                    active_source_count: 1,
                    active_source_bytes: 64,
                }),
                SceneResidentResource::LayerAlphaMaskRtMethod8MdlvGeometry(
                    SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency {
                        object: mdlv_geometry.object,
                        entry_owner_index: mdlv_geometry.entry_owner_index,
                        layout_key: 0x9,
                        vertex_stride_bytes: 20,
                        vertex_count: 4,
                        index_count: 6,
                        vertex_bytes: 80,
                        index_bytes: 12,
                        source_record_count: 0,
                        subdraw_count: 0,
                    },
                ),
            ],
        };
        storage.sync_residency_plan(&residency);

        let requirements = storage.gpu_buffer_requirements();
        assert_eq!(
            requirements,
            vec![
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(SceneGeometryId(2)),
                    role: NativeVulkanSceneGpuBufferRole::MeshVertex,
                    bytes: 80,
                    usage: NativeVulkanSceneGpuBufferUsage::Vertex,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::MeshGeometry(SceneGeometryId(2)),
                    role: NativeVulkanSceneGpuBufferRole::MeshIndex,
                    bytes: 24,
                    usage: NativeVulkanSceneGpuBufferUsage::Index,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(ScenePuppetId(3)),
                    role: NativeVulkanSceneGpuBufferRole::PuppetBone,
                    bytes: 128,
                    usage: NativeVulkanSceneGpuBufferUsage::Storage,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(ScenePuppetId(3)),
                    role: NativeVulkanSceneGpuBufferRole::PuppetSkinVertex,
                    bytes: 128,
                    usage: NativeVulkanSceneGpuBufferUsage::Storage,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(ScenePuppetId(3)),
                    role: NativeVulkanSceneGpuBufferRole::PuppetClipFrame,
                    bytes: 480,
                    usage: NativeVulkanSceneGpuBufferUsage::Storage,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(ScenePuppetId(3)),
                    role: NativeVulkanSceneGpuBufferRole::PuppetClippingRecord,
                    bytes: 48,
                    usage: NativeVulkanSceneGpuBufferUsage::Storage,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(ScenePuppetId(3)),
                    role: NativeVulkanSceneGpuBufferRole::PuppetClippingBoneIndex,
                    bytes: 8,
                    usage: NativeVulkanSceneGpuBufferUsage::Storage,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(ScenePuppetId(3)),
                    role: NativeVulkanSceneGpuBufferRole::PuppetClippingFrameKey,
                    bytes: 12,
                    usage: NativeVulkanSceneGpuBufferUsage::Storage,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::PuppetRig(ScenePuppetId(3)),
                    role: NativeVulkanSceneGpuBufferRole::PuppetActiveSource,
                    bytes: 64,
                    usage: NativeVulkanSceneGpuBufferUsage::Storage,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(
                        mdlv_geometry
                    ),
                    role: NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvVertex,
                    bytes: 80,
                    usage: NativeVulkanSceneGpuBufferUsage::Vertex,
                },
                NativeVulkanSceneGpuBufferRequirement {
                    owner: NativeVulkanSceneGpuBufferOwner::LayerAlphaMaskRtMethod8MdlvEntry(
                        mdlv_geometry
                    ),
                    role: NativeVulkanSceneGpuBufferRole::LayerAlphaMaskRtMethod8MdlvIndex,
                    bytes: 12,
                    usage: NativeVulkanSceneGpuBufferUsage::Index,
                },
            ]
        );
    }

    #[test]
    fn storage_tracks_layer_alpha_mask_rt_method8_geometry_without_mesh_owner() {
        let mut storage = NativeVulkanSceneResourceStorage::default();
        let geometry = NativeVulkanSceneLayerAlphaMaskRtMethod8MdlvEntryGeometry {
            object: SceneObjectId(1530),
            entry_owner_index: 0,
        };
        let residency = SceneResourceResidencyPlan {
            resources: vec![SceneResidentResource::LayerAlphaMaskRtMethod8MdlvGeometry(
                SceneLayerAlphaMaskRtMethod8MdlvGeometryResidency {
                    object: geometry.object,
                    entry_owner_index: geometry.entry_owner_index,
                    layout_key: 0x9,
                    vertex_stride_bytes: 20,
                    vertex_count: 4,
                    index_count: 6,
                    vertex_bytes: 80,
                    index_bytes: 12,
                    source_record_count: 0,
                    subdraw_count: 0,
                },
            )],
        };

        let first = storage.sync_residency_plan(&residency);
        assert!(matches!(
            first.as_slice(),
            [
                RenderingDeviceCommand::EnsureLayerAlphaMaskRtMethod8MdlvGeometryResident {
                    object: SceneObjectId(1530),
                    entry_owner_index: 0,
                    layout_key: 0x9,
                    vertex_stride_bytes: 20,
                    vertex_count: 4,
                    index_count: 6,
                    vertex_bytes: 80,
                    index_bytes: 12,
                }
            ]
        ));
        assert!(
            storage
                .layer_alpha_mask_rt_method8_mdlv_geometry(geometry)
                .is_some()
        );
        assert!(storage.mesh_geometry(SceneGeometryId(1530)).is_none());

        let third = storage.sync_residency_plan(&SceneResourceResidencyPlan::default());
        assert!(matches!(
            third.as_slice(),
            [
                RenderingDeviceCommand::ReleaseLayerAlphaMaskRtMethod8MdlvGeometryResident {
                    object: SceneObjectId(1530),
                    entry_owner_index: 0,
                }
            ]
        ));
    }
}
