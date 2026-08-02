use std::{collections::BTreeMap, sync::Arc};

use tensor_host::Fourcc;
use thiserror::Error;
use vulkan_renderer::{
    Device, DmaBufExportDescriptor, ExportedDmaBufImage, SampledImageDescriptor, TextureFormat, vk,
};

use crate::render::{
    DmabufPlane, DrmNodeId, ExportedDmabuf, NativeCursorTarget, NativeOutputTarget, OutputFormat,
    RenderOutputId,
};

use super::{native_image_usage, texture_format_for_fourcc, vulkan_format_for_fourcc};

const OUTPUT_IMAGE_COUNT: usize = 3;
const CURSOR_IMAGE_COUNT: usize = 3;
const MAX_DMABUF_PLANES: usize = 4;

#[derive(Clone, Debug)]
pub(crate) struct NativeOutputBuffer {
    pub(crate) slot: u8,
    pub(crate) dmabuf: ExportedDmabuf,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeCursorBuffer {
    pub(crate) slot: u8,
    pub(crate) dmabuf: ExportedDmabuf,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeOutputBuffers {
    pub(crate) primary: Vec<NativeOutputBuffer>,
    pub(crate) cursor: Vec<NativeCursorBuffer>,
}

#[derive(Clone, Debug)]
pub(super) struct NativeOutputImageInfo {
    pub(super) image: ExportedDmaBufImage,
    pub(super) sampled_descriptor: SampledImageDescriptor,
    pub(super) format: TextureFormat,
    pub(super) foreign_owned: bool,
}

impl NativeOutputBuffer {
    pub(crate) const COUNT: usize = OUTPUT_IMAGE_COUNT;
}

impl NativeCursorBuffer {
    pub(crate) const COUNT: usize = CURSOR_IMAGE_COUNT;
}

/// Tensor owns output topology and KMS presentation policy; the shared
/// renderer owns every Vulkan image, dedicated export allocation, image view,
/// and exported dma-buf fd behind these retained output slots.
pub(super) struct NativeTargetManager {
    render_node: DrmNodeId,
    active: BTreeMap<RenderOutputId, NativeTargetSet>,
    retired: Vec<NativeTargetSet>,
    cursor_active: BTreeMap<RenderOutputId, NativeCursorTargetSet>,
    cursor_retired: Vec<NativeCursorTargetSet>,
}

impl NativeTargetManager {
    pub(super) fn new(render_node: DrmNodeId) -> Self {
        Self {
            render_node,
            active: BTreeMap::new(),
            retired: Vec::new(),
            cursor_active: BTreeMap::new(),
            cursor_retired: Vec::new(),
        }
    }

    pub(super) fn register(
        &mut self,
        device: &Device,
        target: NativeOutputTarget,
        cursor: Option<NativeCursorTarget>,
    ) -> Result<NativeOutputBuffers, NativeTargetError> {
        let primary_matches = self
            .active
            .get(&target.output)
            .is_some_and(|current| current.target == target);
        let cursor_matches = match cursor {
            Some(cursor) => self
                .cursor_active
                .get(&target.output)
                .is_some_and(|current| current.target == cursor),
            None => !self.cursor_active.contains_key(&target.output),
        };
        if primary_matches && cursor_matches {
            return Ok(self.buffers(target.output));
        }

        let replacement = NativeTargetSet::create(device, self.render_node, target)?;
        let cursor_replacement = match cursor {
            Some(cursor) => {
                NativeCursorTargetSet::create(device, self.render_node, cursor).map(Some)?
            }
            None => None,
        };
        if let Some(previous) = self.active.insert(target.output, replacement) {
            self.retired.push(previous);
        }
        match cursor_replacement {
            Some(replacement) => {
                if let Some(previous) = self.cursor_active.insert(target.output, replacement) {
                    self.cursor_retired.push(previous);
                }
            }
            None => {
                if let Some(previous) = self.cursor_active.remove(&target.output) {
                    self.cursor_retired.push(previous);
                }
            }
        }
        Ok(self.buffers(target.output))
    }

    pub(super) fn mark_submitted(&mut self, output: RenderOutputId, slot: u8, timeline_value: u64) {
        if let Some(target) = self.active.get_mut(&output) {
            target.last_use_timeline = target.last_use_timeline.max(timeline_value);
            if let Some(image) = target.images.get_mut(usize::from(slot)) {
                image.foreign_owned = true;
            }
        }
    }

    pub(super) fn image_info(
        &self,
        output: RenderOutputId,
        slot: u8,
    ) -> Option<NativeOutputImageInfo> {
        self.active
            .get(&output)
            .and_then(|target| target.images.get(usize::from(slot)))
            .map(NativeOutputImage::info)
    }

    pub(super) fn unregister(&mut self, output: RenderOutputId) {
        if let Some(target) = self.active.remove(&output) {
            self.retired.push(target);
        }
        if let Some(target) = self.cursor_active.remove(&output) {
            self.cursor_retired.push(target);
        }
    }

    pub(super) fn retire_completed(&mut self, completed_timeline: u64) {
        retain_pending_targets(&mut self.retired, completed_timeline);
        retain_pending_targets(&mut self.cursor_retired, completed_timeline);
    }

    pub(super) fn destroy(&mut self) {
        self.active.clear();
        self.retired.clear();
        self.cursor_active.clear();
        self.cursor_retired.clear();
    }

    fn buffers(&self, output: RenderOutputId) -> NativeOutputBuffers {
        let primary = self
            .active
            .get(&output)
            .into_iter()
            .flat_map(|target| target.images.iter())
            .enumerate()
            .map(|(slot, image)| NativeOutputBuffer {
                slot: u8::try_from(slot).expect("native output slot count fits in u8"),
                dmabuf: image.dmabuf.clone(),
            })
            .collect();
        let cursor = self
            .cursor_active
            .get(&output)
            .into_iter()
            .flat_map(|target| target.images.iter())
            .enumerate()
            .map(|(slot, image)| NativeCursorBuffer {
                slot: u8::try_from(slot).expect("native cursor slot count fits in u8"),
                dmabuf: image.dmabuf.clone(),
            })
            .collect();
        NativeOutputBuffers { primary, cursor }
    }
}

trait RetainedNativeTarget {
    fn last_use_timeline(&self) -> u64;
}

fn retain_pending_targets<T: RetainedNativeTarget>(targets: &mut Vec<T>, completed_timeline: u64) {
    targets.retain(|target| target.last_use_timeline() > completed_timeline);
}

struct NativeTargetSet {
    target: NativeOutputTarget,
    images: Vec<NativeOutputImage>,
    last_use_timeline: u64,
}

impl RetainedNativeTarget for NativeTargetSet {
    fn last_use_timeline(&self) -> u64 {
        self.last_use_timeline
    }
}

impl NativeTargetSet {
    fn create(
        device: &Device,
        render_node: DrmNodeId,
        target: NativeOutputTarget,
    ) -> Result<Self, NativeTargetError> {
        let images = create_images(
            device,
            render_node,
            ExportImageTarget::from_output(target),
            OUTPUT_IMAGE_COUNT,
            false,
        )?;
        Ok(Self {
            target,
            images,
            last_use_timeline: 0,
        })
    }
}

struct NativeCursorTargetSet {
    target: NativeCursorTarget,
    images: Vec<NativeOutputImage>,
    last_use_timeline: u64,
}

impl RetainedNativeTarget for NativeCursorTargetSet {
    fn last_use_timeline(&self) -> u64 {
        self.last_use_timeline
    }
}

impl NativeCursorTargetSet {
    fn create(
        device: &Device,
        render_node: DrmNodeId,
        target: NativeCursorTarget,
    ) -> Result<Self, NativeTargetError> {
        let images = create_images(
            device,
            render_node,
            ExportImageTarget::from_cursor(target),
            CURSOR_IMAGE_COUNT,
            true,
        )?;
        Ok(Self {
            target,
            images,
            last_use_timeline: 0,
        })
    }
}

fn create_images(
    device: &Device,
    render_node: DrmNodeId,
    target: ExportImageTarget,
    count: usize,
    cursor: bool,
) -> Result<Vec<NativeOutputImage>, NativeTargetError> {
    let mut images = Vec::with_capacity(count);
    for slot in 0..count {
        match NativeOutputImage::create(device, render_node, target) {
            Ok(image) => images.push(image),
            Err(source) => {
                return Err(if cursor {
                    NativeTargetError::CreateCursorSlot { slot, source }
                } else {
                    NativeTargetError::CreateSlot { slot, source }
                });
            }
        }
    }
    Ok(images)
}

#[derive(Clone, Copy)]
struct ExportImageTarget {
    size: tensor_util::Size,
    format: OutputFormat,
}

impl ExportImageTarget {
    fn from_output(target: NativeOutputTarget) -> Self {
        Self {
            size: target.viewport.size(),
            format: target.format,
        }
    }

    fn from_cursor(target: NativeCursorTarget) -> Self {
        Self {
            size: target.size,
            format: target.format,
        }
    }
}

struct NativeOutputImage {
    image: ExportedDmaBufImage,
    format: TextureFormat,
    foreign_owned: bool,
    dmabuf: ExportedDmabuf,
}

impl NativeOutputImage {
    fn create(
        device: &Device,
        render_node: DrmNodeId,
        target: ExportImageTarget,
    ) -> Result<Self, NativeImageError> {
        let fourcc = target.format.format.code;
        let format =
            vulkan_format_for_fourcc(fourcc).ok_or(NativeImageError::UnsupportedFourcc(fourcc))?;
        let texture_format =
            texture_format_for_fourcc(fourcc).ok_or(NativeImageError::UnsupportedFourcc(fourcc))?;
        let image = device
            .create_exportable_dma_buf_image(&DmaBufExportDescriptor {
                label: Some("tensor-native-output".into()),
                format,
                extent: vk::Extent2D {
                    width: target.size.width,
                    height: target.size.height,
                },
                modifiers: vec![target.format.format.modifier.raw()],
                usage: native_image_usage(),
                components: vk::ComponentMapping::default(),
            })
            .map_err(|source| NativeImageError::Create(source.to_string()))?;
        let expected_plane_count = usize::try_from(target.format.plane_count)
            .map_err(|_| NativeImageError::InvalidPlaneCount(target.format.plane_count))?;
        if expected_plane_count == 0 || expected_plane_count > MAX_DMABUF_PLANES {
            return Err(NativeImageError::InvalidPlaneCount(
                target.format.plane_count,
            ));
        }
        if image.planes().len() != expected_plane_count {
            return Err(NativeImageError::PlaneCountMismatch {
                expected: target.format.plane_count,
                actual: image.planes().len(),
            });
        }
        let fd = Arc::new(
            image
                .try_clone_fd()
                .map_err(|source| NativeImageError::DuplicateFd(source.to_string()))?,
        );
        let planes = image
            .planes()
            .iter()
            .map(|plane| {
                Ok(DmabufPlane {
                    fd: Arc::clone(&fd),
                    offset: u32::try_from(plane.offset)
                        .map_err(|_| NativeImageError::LayoutOverflow)?,
                    stride: u32::try_from(plane.row_pitch)
                        .map_err(|_| NativeImageError::LayoutOverflow)?,
                })
            })
            .collect::<Result<Vec<_>, NativeImageError>>()?;
        Ok(Self {
            dmabuf: ExportedDmabuf {
                size: target.size,
                format: target.format.format,
                node: Some(render_node),
                planes,
            },
            image,
            format: texture_format,
            foreign_owned: false,
        })
    }

    fn info(&self) -> NativeOutputImageInfo {
        NativeOutputImageInfo {
            image: self.image.clone(),
            sampled_descriptor: SampledImageDescriptor::from_exported_dma_buf(&self.image),
            format: self.format,
            foreign_owned: self.foreign_owned,
        }
    }
}

#[derive(Debug, Error)]
pub(super) enum NativeTargetError {
    #[error("failed to create native output image slot {slot}: {source}")]
    CreateSlot {
        slot: usize,
        source: NativeImageError,
    },
    #[error("failed to create native cursor image slot {slot}: {source}")]
    CreateCursorSlot {
        slot: usize,
        source: NativeImageError,
    },
}

#[derive(Debug, Error)]
pub(super) enum NativeImageError {
    #[error("DRM fourcc {0} has no Vulkan output format")]
    UnsupportedFourcc(Fourcc),
    #[error("shared renderer failed to create an explicit-modifier output image: {0}")]
    Create(String),
    #[error("native output reports unsupported plane count {0}")]
    InvalidPlaneCount(u32),
    #[error("native output expected {expected} DRM planes, but the created image has {actual}")]
    PlaneCountMismatch { expected: u32, actual: usize },
    #[error("failed to duplicate the shared renderer dma-buf fd: {0}")]
    DuplicateFd(String),
    #[error("dma-buf plane offset or stride exceeds the Linux u32 ABI")]
    LayoutOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_slot_counts_remain_triple_buffered() {
        assert_eq!(NativeOutputBuffer::COUNT, 3);
        assert_eq!(NativeCursorBuffer::COUNT, 3);
    }

    #[test]
    fn completed_timeline_retires_only_old_native_targets() {
        struct Target(u64);
        impl RetainedNativeTarget for Target {
            fn last_use_timeline(&self) -> u64 {
                self.0
            }
        }
        let mut targets = vec![Target(2), Target(4)];
        retain_pending_targets(&mut targets, 2);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, 4);
    }
}
