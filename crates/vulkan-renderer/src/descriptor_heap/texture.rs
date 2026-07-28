//! Standard descriptor-heap bindings for separately sampled textures.
//!
//! The standard intentionally keeps image and sampler descriptors separate:
//! this maps directly to SPIR-V `texture2D` plus `sampler` declarations and
//! allows one filtering policy to be shared by many atlas images. A binding
//! owns only descriptor ranges; callers retain the image/view through command
//! submission and retire the ranges after their last GPU use.

use std::sync::Arc;

use vulkanalia::vk::{self, HasBuilder};

use super::{
    DescriptorAllocation, DescriptorHeap, DescriptorHeapError, DescriptorHeapKind,
    HeapDescriptorType,
};
use crate::{
    ConstantOffsetMapping, Error, FrameToken, ImageView, PushIndexMapping, Result,
    ShaderBindingMap, ShaderBindingMapping, ShaderBindingSource,
};

/// Filtering mode for a descriptor-heap sampler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerFilterMode {
    Nearest,
    Linear,
}

impl SamplerFilterMode {
    const fn as_vk(self) -> vk::Filter {
        match self {
            Self::Nearest => vk::Filter::NEAREST,
            Self::Linear => vk::Filter::LINEAR,
        }
    }

    const fn as_mipmap_vk(self) -> vk::SamplerMipmapMode {
        match self {
            Self::Nearest => vk::SamplerMipmapMode::NEAREST,
            Self::Linear => vk::SamplerMipmapMode::LINEAR,
        }
    }
}

/// Addressing mode for one descriptor-heap sampler axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerAddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
    ClampToBorder,
}

impl SamplerAddressMode {
    const fn as_vk(self) -> vk::SamplerAddressMode {
        match self {
            Self::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
            Self::Repeat => vk::SamplerAddressMode::REPEAT,
            Self::MirrorRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
            Self::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
        }
    }
}

/// Border color used when a sampler axis is [`SamplerAddressMode::ClampToBorder`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerBorderColor {
    TransparentBlack,
    OpaqueBlack,
    OpaqueWhite,
}

impl SamplerBorderColor {
    const fn as_vk(self) -> vk::BorderColor {
        match self {
            Self::TransparentBlack => vk::BorderColor::FLOAT_TRANSPARENT_BLACK,
            Self::OpaqueBlack => vk::BorderColor::FLOAT_OPAQUE_BLACK,
            Self::OpaqueWhite => vk::BorderColor::FLOAT_OPAQUE_WHITE,
        }
    }
}

/// Comparison operation for a depth-comparison sampler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerCompareFunction {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

impl SamplerCompareFunction {
    const fn as_vk(self) -> vk::CompareOp {
        match self {
            Self::Never => vk::CompareOp::NEVER,
            Self::Less => vk::CompareOp::LESS,
            Self::Equal => vk::CompareOp::EQUAL,
            Self::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
            Self::Greater => vk::CompareOp::GREATER,
            Self::NotEqual => vk::CompareOp::NOT_EQUAL,
            Self::GreaterEqual => vk::CompareOp::GREATER_OR_EQUAL,
            Self::Always => vk::CompareOp::ALWAYS,
        }
    }
}

/// Safe standard sampler description for descriptor-heap textures.
///
/// The standard path uses normalized coordinates and does not implicitly
/// enable anisotropic filtering. Advanced Vulkan sampler chains remain
/// available through [`SampledTextureBinding::new_raw`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SamplerDescriptor {
    pub mag_filter: SamplerFilterMode,
    pub min_filter: SamplerFilterMode,
    pub mipmap_filter: SamplerFilterMode,
    pub address_mode_u: SamplerAddressMode,
    pub address_mode_v: SamplerAddressMode,
    pub address_mode_w: SamplerAddressMode,
    pub mip_lod_bias: f32,
    pub lod_min_clamp: f32,
    pub lod_max_clamp: f32,
    pub compare: Option<SamplerCompareFunction>,
    pub border_color: SamplerBorderColor,
}

impl Default for SamplerDescriptor {
    fn default() -> Self {
        Self {
            mag_filter: SamplerFilterMode::Linear,
            min_filter: SamplerFilterMode::Linear,
            mipmap_filter: SamplerFilterMode::Linear,
            address_mode_u: SamplerAddressMode::ClampToEdge,
            address_mode_v: SamplerAddressMode::ClampToEdge,
            address_mode_w: SamplerAddressMode::ClampToEdge,
            mip_lod_bias: 0.0,
            lod_min_clamp: 0.0,
            lod_max_clamp: f32::MAX,
            compare: None,
            border_color: SamplerBorderColor::TransparentBlack,
        }
    }
}

impl SamplerDescriptor {
    pub const fn linear_clamp() -> Self {
        Self {
            mag_filter: SamplerFilterMode::Linear,
            min_filter: SamplerFilterMode::Linear,
            mipmap_filter: SamplerFilterMode::Linear,
            address_mode_u: SamplerAddressMode::ClampToEdge,
            address_mode_v: SamplerAddressMode::ClampToEdge,
            address_mode_w: SamplerAddressMode::ClampToEdge,
            mip_lod_bias: 0.0,
            lod_min_clamp: 0.0,
            lod_max_clamp: f32::MAX,
            compare: None,
            border_color: SamplerBorderColor::TransparentBlack,
        }
    }

    fn to_vk(self) -> Result<vk::SamplerCreateInfo> {
        if !self.mip_lod_bias.is_finite()
            || !self.lod_min_clamp.is_finite()
            || self.lod_min_clamp < 0.0
            || !self.lod_max_clamp.is_finite()
            || self.lod_max_clamp < self.lod_min_clamp
        {
            return Err(Error::Validation(
                "sampler LOD bias and clamp range must be finite, ordered values".into(),
            ));
        }
        let compare_enable = self.compare.is_some();
        let compare_op = self
            .compare
            .map_or(vk::CompareOp::ALWAYS, SamplerCompareFunction::as_vk);
        Ok(vk::SamplerCreateInfo::builder()
            .mag_filter(self.mag_filter.as_vk())
            .min_filter(self.min_filter.as_vk())
            .mipmap_mode(self.mipmap_filter.as_mipmap_vk())
            .address_mode_u(self.address_mode_u.as_vk())
            .address_mode_v(self.address_mode_v.as_vk())
            .address_mode_w(self.address_mode_w.as_vk())
            .mip_lod_bias(self.mip_lod_bias)
            .anisotropy_enable(false)
            .max_anisotropy(1.0)
            .compare_enable(compare_enable)
            .compare_op(compare_op)
            .min_lod(self.lod_min_clamp)
            .max_lod(self.lod_max_clamp)
            .border_color(self.border_color.as_vk())
            .unnormalized_coordinates(false)
            .build())
    }
}

/// SPIR-V descriptor locations for one separately sampled texture.
///
/// This mirrors WebGPU's group/binding vocabulary while preserving Vulkan's
/// explicit descriptor-heap offsets. The image and sampler bindings MUST be
/// distinct because they occupy different descriptor heaps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SampledTextureShaderBindings {
    /// SPIR-V descriptor set/group number.
    pub descriptor_set: u32,
    /// Binding declared as a sampled image/`texture2D`.
    pub image_binding: u32,
    /// Binding declared as a sampler.
    pub sampler_binding: u32,
}

impl SampledTextureShaderBindings {
    pub const fn new(descriptor_set: u32, image_binding: u32, sampler_binding: u32) -> Self {
        Self {
            descriptor_set,
            image_binding,
            sampler_binding,
        }
    }

    fn validate(self) -> Result<()> {
        if self.image_binding == self.sampler_binding {
            return Err(Error::Validation(
                "sampled-texture image and sampler bindings must be distinct".into(),
            ));
        }
        Ok(())
    }

    /// Creates one pipeline-stable push-index mapping for the separate image
    /// and sampler bindings.
    ///
    /// Each pushed `u32` is the descriptor's byte offset in its selected heap;
    /// `heap_index_stride` is therefore one. Replacing an atlas allocation
    /// changes only push data, not pipeline creation state.
    pub fn push_index_shader_binding_map(
        self,
        image_push_offset: u32,
        sampler_push_offset: u32,
    ) -> Result<ShaderBindingMap> {
        self.validate()?;
        ShaderBindingMap::new(vec![
            ShaderBindingMapping {
                descriptor_set: self.descriptor_set,
                first_binding: self.image_binding,
                binding_count: 1,
                resource_mask: vk::SpirvResourceTypeFlagsEXT::SAMPLED_IMAGE,
                source: ShaderBindingSource::PushIndex(PushIndexMapping {
                    heap_offset: 0,
                    push_offset: image_push_offset,
                    heap_index_stride: 1,
                    heap_array_stride: 0,
                }),
            },
            ShaderBindingMapping {
                descriptor_set: self.descriptor_set,
                first_binding: self.sampler_binding,
                binding_count: 1,
                resource_mask: vk::SpirvResourceTypeFlagsEXT::SAMPLER,
                source: ShaderBindingSource::PushIndex(PushIndexMapping {
                    heap_offset: 0,
                    push_offset: sampler_push_offset,
                    heap_index_stride: 1,
                    heap_array_stride: 0,
                }),
            },
        ])
        .map_err(|error| {
            Error::Validation(format!("build sampled-texture push-index mapping: {error}"))
        })
    }
}

/// Descriptor byte offsets written to the push-data locations declared by
/// [`SampledTextureShaderBindings::push_index_shader_binding_map`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SampledTextureHeapOffsets {
    pub image: u32,
    pub sampler: u32,
}

/// A pair of descriptor-heap allocations for one sampled image and sampler.
///
/// The binding is deliberately tied to descriptor offsets rather than a
/// legacy pipeline layout. It can produce the exact [`ShaderBindingMap`] used
/// during graphics or compute pipeline creation. After its last submitted
/// use, call [`Self::retire`] with that submission's [`FrameToken`]. If setup
/// is abandoned before submission, call [`Self::release`] instead.
#[derive(Debug)]
pub struct SampledTextureBinding {
    image: DescriptorAllocation,
    sampler: DescriptorAllocation,
}

impl SampledTextureBinding {
    /// Allocates and writes descriptors for one renderer-owned image view and
    /// one sampler.
    ///
    /// The resource heap receives a `SAMPLED_IMAGE` descriptor and the sampler
    /// heap receives a `SAMPLER` descriptor. Both heaps and `view` MUST belong
    /// to the same logical device.
    ///
    /// `layout` must match the image's state for every shader use. The image
    /// view and its parent image must remain alive until all command buffers
    /// that use this binding have completed; attach the view with
    /// `CommandEncoder::retain_resource`.
    pub fn new(
        resource_heap: &DescriptorHeap,
        sampler_heap: &DescriptorHeap,
        view: &ImageView,
        layout: vk::ImageLayout,
        sampler: SamplerDescriptor,
    ) -> Result<Self> {
        let sampler_info = sampler.to_vk()?;
        // SAFETY: `SamplerDescriptor` constructs a self-contained,
        // validated VkSamplerCreateInfo with no pNext chain.
        unsafe { Self::new_raw(resource_heap, sampler_heap, view, layout, &sampler_info) }
    }

    /// Raw interoperability variant of [`Self::new`].
    ///
    /// # Safety
    ///
    /// `layout` must match the image's state for every shader use. `sampler`
    /// and any pNext chain it references must be valid for
    /// `vkWriteSamplerDescriptorsEXT`. The image view and its parent image
    /// must remain alive until all command buffers that use this binding have
    /// completed; attach the view with `CommandEncoder::retain_resource`.
    pub unsafe fn new_raw(
        resource_heap: &DescriptorHeap,
        sampler_heap: &DescriptorHeap,
        view: &ImageView,
        layout: vk::ImageLayout,
        sampler: &vk::SamplerCreateInfo,
    ) -> Result<Self> {
        validate_heaps_and_view(resource_heap, sampler_heap, view)?;
        let image = resource_heap
            .allocate(HeapDescriptorType::SampledImage)
            .map_err(descriptor_error)?;
        let sampler_allocation = match sampler_heap.allocate(HeapDescriptorType::Sampler) {
            Ok(allocation) => allocation,
            Err(error) => {
                let _ = resource_heap.release(image);
                return Err(descriptor_error(error));
            }
        };
        let view_create_info = view.create_info();
        if let Err(error) = unsafe {
            resource_heap.write_image(
                &image,
                HeapDescriptorType::SampledImage,
                &view_create_info,
                layout,
            )
        } {
            release_pair(resource_heap, sampler_heap, image, sampler_allocation);
            return Err(error);
        }
        if let Err(error) = unsafe { sampler_heap.write_sampler(&sampler_allocation, sampler) } {
            release_pair(resource_heap, sampler_heap, image, sampler_allocation);
            return Err(error);
        }
        Ok(Self {
            image,
            sampler: sampler_allocation,
        })
    }

    /// Byte offset of the sampled-image descriptor in the resource heap.
    pub const fn image_offset(&self) -> u64 {
        self.image.offset()
    }

    /// Byte offset of the sampler descriptor in the sampler heap.
    pub const fn sampler_offset(&self) -> u64 {
        self.sampler.offset()
    }

    /// Returns exact descriptor byte offsets suitable as push-index values.
    /// Vulkan's mapping representation is 32-bit, so oversized heaps fail
    /// explicitly instead of truncating.
    pub fn push_index_heap_offsets(&self) -> Result<SampledTextureHeapOffsets> {
        Ok(SampledTextureHeapOffsets {
            image: u32::try_from(self.image.offset()).map_err(|_| {
                Error::Validation("sampled-image descriptor offset exceeds u32".into())
            })?,
            sampler: u32::try_from(self.sampler.offset())
                .map_err(|_| Error::Validation("sampler descriptor offset exceeds u32".into()))?,
        })
    }

    /// Produces the descriptor-heap SPIR-V mapping for this binding.
    ///
    /// The resulting map uses one `SAMPLED_IMAGE` and one `SAMPLER` mapping,
    /// each with one descriptor and no array stride. The offsets are checked
    /// against Vulkan's 32-bit mapping representation rather than truncated.
    pub fn shader_binding_map(
        &self,
        bindings: SampledTextureShaderBindings,
    ) -> Result<ShaderBindingMap> {
        bindings.validate()?;
        let offsets = self.push_index_heap_offsets()?;
        ShaderBindingMap::new(vec![
            ShaderBindingMapping {
                descriptor_set: bindings.descriptor_set,
                first_binding: bindings.image_binding,
                binding_count: 1,
                resource_mask: vk::SpirvResourceTypeFlagsEXT::SAMPLED_IMAGE,
                source: ShaderBindingSource::ConstantOffset(ConstantOffsetMapping {
                    heap_offset: offsets.image,
                    heap_array_stride: 0,
                    sampler_heap_offset: 0,
                    sampler_heap_array_stride: 0,
                }),
            },
            ShaderBindingMapping {
                descriptor_set: bindings.descriptor_set,
                first_binding: bindings.sampler_binding,
                binding_count: 1,
                resource_mask: vk::SpirvResourceTypeFlagsEXT::SAMPLER,
                source: ShaderBindingSource::ConstantOffset(ConstantOffsetMapping {
                    // A separate OpTypeSampler uses heapOffset; Vulkan's
                    // samplerHeapOffset is only the sampler half of an
                    // OpTypeSampledImage.
                    heap_offset: offsets.sampler,
                    heap_array_stride: 0,
                    sampler_heap_offset: 0,
                    sampler_heap_array_stride: 0,
                }),
            },
        ])
        .map_err(|error| {
            Error::Validation(format!("build sampled-texture shader mapping: {error}"))
        })
    }

    /// Retires both descriptor ranges after the last GPU submission that can
    /// read them.
    pub fn retire(
        self,
        resource_heap: &DescriptorHeap,
        sampler_heap: &DescriptorHeap,
        after: FrameToken,
    ) -> std::result::Result<(), DescriptorHeapError> {
        validate_owned_heaps(resource_heap, sampler_heap, &self)?;
        resource_heap.retire(self.image, after)?;
        sampler_heap.retire(self.sampler, after)
    }

    /// Immediately returns both ranges when no command buffer has referenced
    /// them. Submitted descriptors MUST use [`Self::retire`] instead.
    pub fn release(
        self,
        resource_heap: &DescriptorHeap,
        sampler_heap: &DescriptorHeap,
    ) -> std::result::Result<(), DescriptorHeapError> {
        validate_owned_heaps(resource_heap, sampler_heap, &self)?;
        resource_heap.release(self.image)?;
        sampler_heap.release(self.sampler)
    }
}

fn validate_heaps_and_view(
    resource_heap: &DescriptorHeap,
    sampler_heap: &DescriptorHeap,
    view: &ImageView,
) -> Result<()> {
    if resource_heap.kind() != DescriptorHeapKind::Resource {
        return Err(Error::Validation(
            "sampled textures require a resource descriptor heap".into(),
        ));
    }
    if sampler_heap.kind() != DescriptorHeapKind::Sampler {
        return Err(Error::Validation(
            "sampled textures require a sampler descriptor heap".into(),
        ));
    }
    if !Arc::ptr_eq(view.owner(), &resource_heap.owner)
        || !Arc::ptr_eq(&resource_heap.owner, &sampler_heap.owner)
    {
        return Err(Error::Validation(
            "sampled texture view and descriptor heaps must share one Device".into(),
        ));
    }
    Ok(())
}

fn validate_owned_heaps(
    resource_heap: &DescriptorHeap,
    sampler_heap: &DescriptorHeap,
    binding: &SampledTextureBinding,
) -> std::result::Result<(), DescriptorHeapError> {
    if resource_heap.kind() != DescriptorHeapKind::Resource
        || sampler_heap.kind() != DescriptorHeapKind::Sampler
        || !resource_heap.owns(&binding.image)
        || !sampler_heap.owns(&binding.sampler)
    {
        return Err(DescriptorHeapError::WrongAllocator);
    }
    Ok(())
}

fn descriptor_error(error: DescriptorHeapError) -> Error {
    Error::Validation(format!("allocate sampled-texture descriptor: {error}"))
}

fn release_pair(
    resource_heap: &DescriptorHeap,
    sampler_heap: &DescriptorHeap,
    image: DescriptorAllocation,
    sampler: DescriptorAllocation,
) {
    let _ = resource_heap.release(image);
    let _ = sampler_heap.release(sampler);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(image_offset: u64, sampler_offset: u64) -> SampledTextureBinding {
        SampledTextureBinding {
            image: DescriptorAllocation {
                range: image_offset..image_offset + 32,
                allocator_id: 1,
            },
            sampler: DescriptorAllocation {
                range: sampler_offset..sampler_offset + 16,
                allocator_id: 2,
            },
        }
    }

    #[test]
    fn sampled_texture_maps_separate_image_and_sampler_heap_offsets() {
        let map = binding(256, 64)
            .shader_binding_map(SampledTextureShaderBindings::new(2, 3, 7))
            .unwrap();
        assert_eq!(map.mappings().len(), 2);
        assert_eq!(map.mappings()[0].descriptor_set, 2);
        assert_eq!(map.mappings()[0].first_binding, 3);
        assert_eq!(
            map.mappings()[0].resource_mask,
            vk::SpirvResourceTypeFlagsEXT::SAMPLED_IMAGE
        );
        assert_eq!(
            map.mappings()[0].source,
            ShaderBindingSource::ConstantOffset(ConstantOffsetMapping {
                heap_offset: 256,
                heap_array_stride: 0,
                sampler_heap_offset: 0,
                sampler_heap_array_stride: 0,
            })
        );
        assert_eq!(map.mappings()[1].first_binding, 7);
        assert_eq!(
            map.mappings()[1].resource_mask,
            vk::SpirvResourceTypeFlagsEXT::SAMPLER
        );
        assert_eq!(
            map.mappings()[1].source,
            ShaderBindingSource::ConstantOffset(ConstantOffsetMapping {
                heap_offset: 64,
                heap_array_stride: 0,
                sampler_heap_offset: 0,
                sampler_heap_array_stride: 0,
            })
        );
    }

    #[test]
    fn sampled_texture_rejects_overlapping_bindings_and_truncating_offsets() {
        let texture_binding = binding(0, 16);
        assert!(
            texture_binding
                .shader_binding_map(SampledTextureShaderBindings::new(0, 1, 1))
                .is_err()
        );
        assert!(
            binding(u64::from(u32::MAX) + 1, 16)
                .shader_binding_map(SampledTextureShaderBindings::new(0, 0, 1))
                .is_err()
        );
    }

    #[test]
    fn sampled_texture_push_index_map_keeps_pipeline_independent_of_heap_slot() {
        let map = SampledTextureShaderBindings::new(2, 3, 7)
            .push_index_shader_binding_map(0, 4)
            .unwrap();
        assert_eq!(
            map.mappings()[0].source,
            ShaderBindingSource::PushIndex(PushIndexMapping {
                heap_offset: 0,
                push_offset: 0,
                heap_index_stride: 1,
                heap_array_stride: 0,
            })
        );
        assert_eq!(
            map.mappings()[1].source,
            ShaderBindingSource::PushIndex(PushIndexMapping {
                heap_offset: 0,
                push_offset: 4,
                heap_index_stride: 1,
                heap_array_stride: 0,
            })
        );
        assert_eq!(
            binding(256, 64).push_index_heap_offsets().unwrap(),
            SampledTextureHeapOffsets {
                image: 256,
                sampler: 64,
            }
        );
    }

    #[test]
    fn standard_sampler_is_linear_clamp_without_anisotropy() {
        let sampler = SamplerDescriptor::linear_clamp().to_vk().unwrap();
        assert_eq!(sampler.mag_filter, vk::Filter::LINEAR);
        assert_eq!(sampler.min_filter, vk::Filter::LINEAR);
        assert_eq!(sampler.mipmap_mode, vk::SamplerMipmapMode::LINEAR);
        assert_eq!(
            sampler.address_mode_u,
            vk::SamplerAddressMode::CLAMP_TO_EDGE
        );
        assert_eq!(sampler.anisotropy_enable, vk::FALSE);
        assert_eq!(sampler.unnormalized_coordinates, vk::FALSE);
    }

    #[test]
    fn standard_sampler_rejects_invalid_lod_ranges() {
        assert!(
            SamplerDescriptor {
                lod_min_clamp: 4.0,
                lod_max_clamp: 2.0,
                ..SamplerDescriptor::default()
            }
            .to_vk()
            .is_err()
        );
        assert!(
            SamplerDescriptor {
                mip_lod_bias: f32::NAN,
                ..SamplerDescriptor::default()
            }
            .to_vk()
            .is_err()
        );
    }
}
