//! Retained scene texture image store for native Vulkan.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `reverse-engineered/docs/exe/texture-and-format.md`
//! - `references/godot/servers/rendering/storage/texture_storage.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneResource, SceneResourceId, SceneTextureFormat, SceneTextureResidency,
};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaImage, NativeVulkanVulkanaliaImageMipUpload,
    native_vulkan_vulkanalia_create_sampled_image_with_staging_upload,
    native_vulkan_vulkanalia_destroy_image,
};

use super::resource_storage::NativeVulkanSceneResourceStorage;

const GTEX_HEADER_BYTES: usize = 32;
const GTEX_MAGIC: &[u8; 8] = b"GDTEX002";
const GTEX_FORMAT_BC1_RGBA_UNORM_BLOCK: u32 = 1;
const GTEX_FORMAT_BC3_UNORM_BLOCK: u32 = 3;
const GTEX_FORMAT_BC7_UNORM_BLOCK: u32 = 7;
const GTEX_FORMAT_R8_UNORM: u32 = 9;
const GTEX_FORMAT_R8G8B8A8_UNORM: u32 = 37;
const BC_BLOCK_TEXELS: u32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureUploadPlan {
    uploads: Vec<NativeVulkanSceneTextureUpload>,
}

impl NativeVulkanSceneTextureUploadPlan {
    pub(in crate::renderer::native_vulkan) fn from_resident_resources(
        storage: &NativeVulkanSceneResourceStorage,
        resources: &[SceneResource],
    ) -> Result<Self, String> {
        let active = storage
            .texture_residencies()
            .map(texture_requirement)
            .collect::<Result<BTreeSet<_>, _>>()?;
        let mut pending = active.clone();
        let mut uploads = Vec::new();

        for resource in resources {
            let SceneResource::Texture { id, source, .. } = resource else {
                continue;
            };
            let Some(residency) = storage.texture(*id) else {
                continue;
            };
            let requirement = texture_requirement(residency)?;
            if active.contains(&requirement) {
                pending.remove(&requirement);
                uploads.push(NativeVulkanSceneTextureUpload {
                    requirement,
                    source: source.clone(),
                });
            }
        }

        if let Some(requirement) = pending.into_iter().next() {
            return Err(format!(
                "missing resident scene texture source for {:?}",
                requirement.resource
            ));
        }

        Ok(Self { uploads })
    }

    pub(in crate::renderer::native_vulkan) fn uploads(&self) -> &[NativeVulkanSceneTextureUpload] {
        &self.uploads
    }

    pub(in crate::renderer::native_vulkan) fn into_uploads(
        self,
    ) -> Vec<NativeVulkanSceneTextureUpload> {
        self.uploads
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureUpload {
    pub requirement: NativeVulkanSceneTextureImageRequirement,
    pub source: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureImageRequirement {
    pub resource: SceneResourceId,
    pub width: u32,
    pub height: u32,
    pub format: SceneTextureFormat,
    pub mip_count: u32,
    pub payload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureImageRecord {
    pub requirement: NativeVulkanSceneTextureImageRequirement,
    pub source: PathBuf,
    pub vk_format: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) enum NativeVulkanSceneTextureImageSyncAction {
    Create {
        record: NativeVulkanSceneTextureImageRecord,
    },
    Reuse {
        record: NativeVulkanSceneTextureImageRecord,
    },
    Replace {
        old: NativeVulkanSceneTextureImageRecord,
        new: NativeVulkanSceneTextureImageRecord,
    },
    Release {
        record: NativeVulkanSceneTextureImageRecord,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureImageBinding {
    pub resource: SceneResourceId,
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureImageCatalog {
    records: BTreeMap<SceneResourceId, NativeVulkanSceneTextureImageRecord>,
    last_actions: Vec<NativeVulkanSceneTextureImageSyncAction>,
}

impl NativeVulkanSceneTextureImageCatalog {
    pub(in crate::renderer::native_vulkan) fn sync_upload_plan(
        &mut self,
        upload_plan: &NativeVulkanSceneTextureUploadPlan,
    ) -> Result<&[NativeVulkanSceneTextureImageSyncAction], String> {
        let upload_records = texture_upload_records(upload_plan.uploads())?;
        self.last_actions.clear();

        let active = upload_records.keys().copied().collect::<BTreeSet<_>>();
        let stale = self
            .records
            .keys()
            .copied()
            .filter(|resource| !active.contains(resource))
            .collect::<Vec<_>>();
        for resource in stale {
            if let Some(record) = self.records.remove(&resource) {
                self.last_actions
                    .push(NativeVulkanSceneTextureImageSyncAction::Release { record });
            }
        }

        for (resource, new_record) in upload_records {
            match self.records.get(&resource).cloned() {
                Some(old_record) if old_record == new_record => {
                    self.last_actions
                        .push(NativeVulkanSceneTextureImageSyncAction::Reuse {
                            record: old_record,
                        });
                }
                Some(old_record) => {
                    self.records.insert(resource, new_record.clone());
                    self.last_actions
                        .push(NativeVulkanSceneTextureImageSyncAction::Replace {
                            old: old_record,
                            new: new_record,
                        });
                }
                None => {
                    self.records.insert(resource, new_record.clone());
                    self.last_actions
                        .push(NativeVulkanSceneTextureImageSyncAction::Create {
                            record: new_record,
                        });
                }
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanSceneTextureImageSyncAction] {
        &self.last_actions
    }
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneTextureImageStore {
    images: BTreeMap<SceneResourceId, NativeVulkanSceneTextureImageSlot>,
    last_actions: Vec<NativeVulkanSceneTextureImageSyncAction>,
}

impl NativeVulkanSceneTextureImageStore {
    pub(in crate::renderer::native_vulkan) fn new() -> Self {
        Self {
            images: BTreeMap::new(),
            last_actions: Vec::new(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn sync_upload_plan(
        &mut self,
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        upload_plan: NativeVulkanSceneTextureUploadPlan,
    ) -> Result<&[NativeVulkanSceneTextureImageSyncAction], String> {
        let uploads = texture_upload_map(upload_plan.into_uploads())?;
        self.last_actions.clear();

        let active = uploads.keys().copied().collect::<BTreeSet<_>>();
        let stale = self
            .images
            .keys()
            .copied()
            .filter(|resource| !active.contains(resource))
            .collect::<Vec<_>>();
        for resource in stale {
            if let Some(slot) = self.images.remove(&resource) {
                native_vulkan_vulkanalia_destroy_image(device, slot.image);
                self.last_actions
                    .push(NativeVulkanSceneTextureImageSyncAction::Release {
                        record: slot.record,
                    });
            }
        }

        for (resource, upload) in uploads {
            let new_record = texture_upload_record(&upload)?;
            if let Some(old_slot) = self.images.get(&resource)
                && old_slot.record == new_record
            {
                self.last_actions
                    .push(NativeVulkanSceneTextureImageSyncAction::Reuse {
                        record: old_slot.record.clone(),
                    });
                continue;
            }

            let texture_payload = read_gtex_texture_payload(&upload)?;
            let image = native_vulkan_vulkanalia_create_sampled_image_with_staging_upload(
                device,
                memory_properties,
                command_pool,
                queue,
                "scene-texture-image",
                scene_texture_vk_format(upload.requirement.format),
                upload.requirement.width,
                upload.requirement.height,
                upload.requirement.mip_count,
                &texture_payload.payload,
                &texture_payload.mips,
            )?;

            match self.images.insert(
                resource,
                NativeVulkanSceneTextureImageSlot {
                    record: new_record.clone(),
                    image,
                },
            ) {
                Some(old_slot) => {
                    native_vulkan_vulkanalia_destroy_image(device, old_slot.image);
                    self.last_actions
                        .push(NativeVulkanSceneTextureImageSyncAction::Replace {
                            old: old_slot.record,
                            new: new_record,
                        });
                }
                None => {
                    self.last_actions
                        .push(NativeVulkanSceneTextureImageSyncAction::Create {
                            record: new_record,
                        });
                }
            }
        }

        Ok(&self.last_actions)
    }

    pub(in crate::renderer::native_vulkan) fn texture_binding(
        &self,
        resource: SceneResourceId,
    ) -> Result<NativeVulkanSceneTextureImageBinding, String> {
        let slot = self
            .images
            .get(&resource)
            .ok_or_else(|| format!("missing retained scene texture image for {resource:?}"))?;
        Ok(NativeVulkanSceneTextureImageBinding {
            resource,
            image: slot.image.image,
            view: slot.image.view,
            sampler: slot.image.sampler,
            format: scene_texture_vk_format(slot.record.requirement.format),
            width: slot.record.requirement.width,
            height: slot.record.requirement.height,
            mip_count: slot.record.requirement.mip_count,
        })
    }

    pub(in crate::renderer::native_vulkan) fn last_actions(
        &self,
    ) -> &[NativeVulkanSceneTextureImageSyncAction] {
        &self.last_actions
    }

    pub(in crate::renderer::native_vulkan) fn destroy_all(&mut self, device: &Device) {
        for (_, slot) in std::mem::take(&mut self.images) {
            native_vulkan_vulkanalia_destroy_image(device, slot.image);
        }
        self.last_actions.clear();
    }
}

impl Default for NativeVulkanSceneTextureImageStore {
    fn default() -> Self {
        Self::new()
    }
}

struct NativeVulkanSceneTextureImageSlot {
    record: NativeVulkanSceneTextureImageRecord,
    image: NativeVulkanVulkanaliaImage,
}

struct GtexTexturePayload {
    payload: Vec<u8>,
    mips: Vec<NativeVulkanVulkanaliaImageMipUpload>,
}

fn texture_requirement(
    texture: &SceneTextureResidency,
) -> Result<NativeVulkanSceneTextureImageRequirement, String> {
    let width = texture
        .width
        .ok_or_else(|| format!("scene texture {:?} is missing width", texture.id))?;
    let height = texture
        .height
        .ok_or_else(|| format!("scene texture {:?} is missing height", texture.id))?;
    let format = texture
        .format
        .ok_or_else(|| format!("scene texture {:?} is missing native format", texture.id))?;
    let mip_count = texture
        .mip_count
        .ok_or_else(|| format!("scene texture {:?} is missing mip count", texture.id))?;
    let payload_bytes = texture
        .payload_bytes
        .ok_or_else(|| format!("scene texture {:?} is missing payload bytes", texture.id))?;
    if width == 0 || height == 0 || mip_count == 0 {
        return Err(format!(
            "scene texture {:?} has invalid metadata {width}x{height} mips={mip_count}",
            texture.id
        ));
    }
    Ok(NativeVulkanSceneTextureImageRequirement {
        resource: texture.id,
        width,
        height,
        format,
        mip_count,
        payload_bytes,
    })
}

fn texture_upload_records(
    uploads: &[NativeVulkanSceneTextureUpload],
) -> Result<BTreeMap<SceneResourceId, NativeVulkanSceneTextureImageRecord>, String> {
    let mut records = BTreeMap::new();
    for upload in uploads {
        let record = texture_upload_record(upload)?;
        if records
            .insert(record.requirement.resource, record.clone())
            .is_some()
        {
            return Err(format!(
                "duplicate scene texture upload for {:?}",
                record.requirement.resource
            ));
        }
    }
    Ok(records)
}

fn texture_upload_map(
    uploads: Vec<NativeVulkanSceneTextureUpload>,
) -> Result<BTreeMap<SceneResourceId, NativeVulkanSceneTextureUpload>, String> {
    let mut by_resource = BTreeMap::new();
    for upload in uploads {
        if by_resource
            .insert(upload.requirement.resource, upload)
            .is_some()
        {
            return Err("duplicate scene texture upload resource".to_owned());
        }
    }
    Ok(by_resource)
}

fn texture_upload_record(
    upload: &NativeVulkanSceneTextureUpload,
) -> Result<NativeVulkanSceneTextureImageRecord, String> {
    Ok(NativeVulkanSceneTextureImageRecord {
        requirement: upload.requirement.clone(),
        source: upload.source.clone(),
        vk_format: scene_texture_vk_format_label(upload.requirement.format),
    })
}

fn read_gtex_texture_payload(
    upload: &NativeVulkanSceneTextureUpload,
) -> Result<GtexTexturePayload, String> {
    let bytes = fs::read(&upload.source)
        .map_err(|err| format!("read scene texture {}: {err}", upload.source.display()))?;
    if bytes.len() < GTEX_HEADER_BYTES {
        return Err(format!(
            "{} is shorter than native gtex header",
            upload.source.display()
        ));
    }
    validate_gtex_header(upload, &bytes[..GTEX_HEADER_BYTES])?;
    let payload = bytes[GTEX_HEADER_BYTES..].to_vec();
    if payload.len() as u64 != upload.requirement.payload_bytes {
        return Err(format!(
            "{} payload has {} bytes, expected {}",
            upload.source.display(),
            payload.len(),
            upload.requirement.payload_bytes
        ));
    }
    let mips = scene_texture_mip_uploads(&upload.requirement)?;
    Ok(GtexTexturePayload { payload, mips })
}

fn validate_gtex_header(
    upload: &NativeVulkanSceneTextureUpload,
    header: &[u8],
) -> Result<(), String> {
    if header.get(0..8) != Some(GTEX_MAGIC.as_slice()) {
        return Err(format!(
            "{} is not a native GDTEX002 texture",
            upload.source.display()
        ));
    }
    let width = read_u32(header, 8, "width")?;
    let height = read_u32(header, 12, "height")?;
    let format = read_u32(header, 16, "format")?;
    let mip_count = read_u32(header, 20, "mip count")?;
    let payload_bytes = read_u64(header, 24, "payload bytes")?;
    let expected_format = scene_texture_gtex_format(upload.requirement.format);
    if width != upload.requirement.width
        || height != upload.requirement.height
        || format != expected_format
        || mip_count != upload.requirement.mip_count
        || payload_bytes != upload.requirement.payload_bytes
    {
        return Err(format!(
            "{} gtex header does not match scene residency: header {width}x{height} fmt={format} mips={mip_count} payload={payload_bytes}, residency {}x{} fmt={} mips={} payload={}",
            upload.source.display(),
            upload.requirement.width,
            upload.requirement.height,
            expected_format,
            upload.requirement.mip_count,
            upload.requirement.payload_bytes
        ));
    }
    Ok(())
}

fn scene_texture_mip_uploads(
    requirement: &NativeVulkanSceneTextureImageRequirement,
) -> Result<Vec<NativeVulkanVulkanaliaImageMipUpload>, String> {
    let mut uploads = Vec::with_capacity(requirement.mip_count as usize);
    let mut offset = 0u64;
    for level in 0..requirement.mip_count {
        let width = mip_extent(requirement.width, level);
        let height = mip_extent(requirement.height, level);
        let byte_count = scene_texture_mip_bytes(requirement.format, width, height)?;
        uploads.push(NativeVulkanVulkanaliaImageMipUpload {
            buffer_offset: offset,
            byte_count,
            width,
            height,
        });
        offset = offset
            .checked_add(byte_count)
            .ok_or_else(|| "scene texture mip offset overflow".to_owned())?;
    }
    if offset != requirement.payload_bytes {
        return Err(format!(
            "scene texture {:?} mip bytes sum to {offset}, expected {}",
            requirement.resource, requirement.payload_bytes
        ));
    }
    Ok(uploads)
}

fn scene_texture_mip_bytes(
    format: SceneTextureFormat,
    width: u32,
    height: u32,
) -> Result<u64, String> {
    match format {
        SceneTextureFormat::R8Unorm => Ok(u64::from(width) * u64::from(height)),
        SceneTextureFormat::R8G8B8A8Unorm => Ok(u64::from(width) * u64::from(height) * 4),
        SceneTextureFormat::Bc1RgbaUnormBlock => bc_mip_bytes(width, height, 8),
        SceneTextureFormat::Bc3UnormBlock | SceneTextureFormat::Bc7UnormBlock => {
            bc_mip_bytes(width, height, 16)
        }
    }
}

fn bc_mip_bytes(width: u32, height: u32, block_bytes: u64) -> Result<u64, String> {
    let blocks_w = u64::from(width.div_ceil(BC_BLOCK_TEXELS));
    let blocks_h = u64::from(height.div_ceil(BC_BLOCK_TEXELS));
    blocks_w
        .checked_mul(blocks_h)
        .and_then(|blocks| blocks.checked_mul(block_bytes))
        .ok_or_else(|| "scene texture BC mip byte count overflow".to_owned())
}

fn mip_extent(base: u32, level: u32) -> u32 {
    base.checked_shr(level).unwrap_or(0).max(1)
}

fn scene_texture_vk_format(format: SceneTextureFormat) -> vk::Format {
    match format {
        SceneTextureFormat::Bc1RgbaUnormBlock => vk::Format::BC1_RGBA_UNORM_BLOCK,
        SceneTextureFormat::Bc3UnormBlock => vk::Format::BC3_UNORM_BLOCK,
        SceneTextureFormat::Bc7UnormBlock => vk::Format::BC7_UNORM_BLOCK,
        SceneTextureFormat::R8Unorm => vk::Format::R8_UNORM,
        SceneTextureFormat::R8G8B8A8Unorm => vk::Format::R8G8B8A8_UNORM,
    }
}

fn scene_texture_vk_format_label(format: SceneTextureFormat) -> &'static str {
    match format {
        SceneTextureFormat::Bc1RgbaUnormBlock => "BC1_RGBA_UNORM_BLOCK",
        SceneTextureFormat::Bc3UnormBlock => "BC3_UNORM_BLOCK",
        SceneTextureFormat::Bc7UnormBlock => "BC7_UNORM_BLOCK",
        SceneTextureFormat::R8Unorm => "R8_UNORM",
        SceneTextureFormat::R8G8B8A8Unorm => "R8G8B8A8_UNORM",
    }
}

fn scene_texture_gtex_format(format: SceneTextureFormat) -> u32 {
    match format {
        SceneTextureFormat::Bc1RgbaUnormBlock => GTEX_FORMAT_BC1_RGBA_UNORM_BLOCK,
        SceneTextureFormat::Bc3UnormBlock => GTEX_FORMAT_BC3_UNORM_BLOCK,
        SceneTextureFormat::Bc7UnormBlock => GTEX_FORMAT_BC7_UNORM_BLOCK,
        SceneTextureFormat::R8Unorm => GTEX_FORMAT_R8_UNORM,
        SceneTextureFormat::R8G8B8A8Unorm => GTEX_FORMAT_R8G8B8A8_UNORM,
    }
}

fn read_u32(bytes: &[u8], offset: usize, label: &'static str) -> Result<u32, String> {
    let data = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("native gtex header missing {label}"))?;
    Ok(u32::from_le_bytes(
        data.try_into().expect("u32 header bytes"),
    ))
}

fn read_u64(bytes: &[u8], offset: usize, label: &'static str) -> Result<u64, String> {
    let data = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| format!("native gtex header missing {label}"))?;
    Ok(u64::from_le_bytes(
        data.try_into().expect("u64 header bytes"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::SceneResourceResidencyPlan;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn texture_upload_plan_keeps_payload_out_until_store_sync() {
        let texture = SceneResource::Texture {
            id: SceneResourceId(7),
            source: PathBuf::from("/tmp/eye.gtex"),
            width: Some(4),
            height: Some(4),
            format: Some(SceneTextureFormat::R8Unorm),
            mip_count: Some(3),
            payload_bytes: Some(21),
        };
        let resources = vec![texture];
        let residency = SceneResourceResidencyPlan::from_resources(&resources);
        let mut storage = NativeVulkanSceneResourceStorage::default();
        storage.sync_residency_plan(&residency);

        let plan =
            NativeVulkanSceneTextureUploadPlan::from_resident_resources(&storage, &resources)
                .expect("texture upload plan");

        assert_eq!(plan.uploads().len(), 1);
        assert_eq!(plan.uploads()[0].requirement.resource, SceneResourceId(7));
        assert_eq!(plan.uploads()[0].requirement.payload_bytes, 21);
    }

    #[test]
    fn texture_upload_plan_rejects_missing_native_metadata() {
        let texture = SceneResource::Texture {
            id: SceneResourceId(7),
            source: PathBuf::from("/tmp/eye.gtex"),
            width: Some(4),
            height: Some(4),
            format: None,
            mip_count: Some(3),
            payload_bytes: Some(21),
        };
        let resources = vec![texture];
        let residency = SceneResourceResidencyPlan::from_resources(&resources);
        let mut storage = NativeVulkanSceneResourceStorage::default();
        storage.sync_residency_plan(&residency);

        let err = NativeVulkanSceneTextureUploadPlan::from_resident_resources(&storage, &resources)
            .expect_err("missing format must fail");

        assert!(err.contains("native format"));
    }

    #[test]
    fn reads_gtex_payload_and_builds_mip_uploads() {
        let root = unique_test_dir("gilder-scene-texture-upload");
        fs::create_dir_all(&root).expect("test dir");
        let path = root.join("mask.gtex");
        let payload = (0..21).collect::<Vec<u8>>();
        write_gtex(&path, 4, 4, GTEX_FORMAT_R8_UNORM, 3, &payload);
        let upload = NativeVulkanSceneTextureUpload {
            requirement: NativeVulkanSceneTextureImageRequirement {
                resource: SceneResourceId(7),
                width: 4,
                height: 4,
                format: SceneTextureFormat::R8Unorm,
                mip_count: 3,
                payload_bytes: 21,
            },
            source: path,
        };

        let texture_payload = read_gtex_texture_payload(&upload).expect("payload");

        assert_eq!(texture_payload.payload, payload);
        assert_eq!(
            texture_payload.mips,
            vec![
                NativeVulkanVulkanaliaImageMipUpload {
                    buffer_offset: 0,
                    byte_count: 16,
                    width: 4,
                    height: 4,
                },
                NativeVulkanVulkanaliaImageMipUpload {
                    buffer_offset: 16,
                    byte_count: 4,
                    width: 2,
                    height: 2,
                },
                NativeVulkanVulkanaliaImageMipUpload {
                    buffer_offset: 20,
                    byte_count: 1,
                    width: 1,
                    height: 1,
                },
            ]
        );
        fs::remove_dir_all(root).expect("remove test dir");
    }

    #[test]
    fn catalog_reuses_unchanged_texture_records() {
        let upload = NativeVulkanSceneTextureUpload {
            requirement: NativeVulkanSceneTextureImageRequirement {
                resource: SceneResourceId(7),
                width: 4,
                height: 4,
                format: SceneTextureFormat::R8Unorm,
                mip_count: 3,
                payload_bytes: 21,
            },
            source: PathBuf::from("/tmp/mask.gtex"),
        };
        let plan = NativeVulkanSceneTextureUploadPlan {
            uploads: vec![upload],
        };
        let mut catalog = NativeVulkanSceneTextureImageCatalog::default();

        let first = catalog.sync_upload_plan(&plan).expect("first").to_vec();
        let second = catalog.sync_upload_plan(&plan).expect("second").to_vec();

        assert!(matches!(
            first.as_slice(),
            [NativeVulkanSceneTextureImageSyncAction::Create { .. }]
        ));
        assert!(matches!(
            second.as_slice(),
            [NativeVulkanSceneTextureImageSyncAction::Reuse { .. }]
        ));
    }

    fn write_gtex(
        path: &Path,
        width: u32,
        height: u32,
        format: u32,
        mip_count: u32,
        payload: &[u8],
    ) {
        let mut bytes = Vec::with_capacity(GTEX_HEADER_BYTES + payload.len());
        bytes.extend_from_slice(GTEX_MAGIC);
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&format.to_le_bytes());
        bytes.extend_from_slice(&mip_count.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(payload);
        fs::write(path, bytes).expect("write gtex");
    }

    fn unique_test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }
}
