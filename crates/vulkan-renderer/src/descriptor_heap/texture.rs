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
    ConstantOffsetMapping, Error, ExportedDmaBufImage, FrameToken, ImageView, ImportedDmaBufImage,
    PushIndexMapping, Result, ShaderBindingMap, ShaderBindingMapping, ShaderBindingSource,
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

impl SampledTextureHeapOffsets {
    /// Resolves one independently allocated sampled image and sampler into
    /// the push-index pair consumed by a separately sampled texture shader.
    pub fn from_bindings(image: &SampledImageBinding, sampler: &SamplerBinding) -> Result<Self> {
        Ok(Self {
            image: image.push_index_heap_offset()?,
            sampler: sampler.push_index_heap_offset()?,
        })
    }
}

/// Descriptor element indices for direct descriptor-heap shader access.
///
/// `SPV_EXT_descriptor_heap` shaders (e.g. Slang `DescriptorHandle<T>`) index
/// resource heap builtins through TensorDE's unified image/buffer stride and
/// sampler heap builtins through the sampler stride, so these are element
/// indices, not the byte offsets of
/// [`SampledTextureHeapOffsets`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SampledTextureHeapIndices {
    pub image: u32,
    pub sampler: u32,
}

impl SampledTextureHeapIndices {
    /// Resolves one independently allocated sampled image and sampler into
    /// the element-index pair consumed by a direct descriptor-heap shader.
    pub fn from_bindings(image: &SampledImageBinding, sampler: &SamplerBinding) -> Result<Self> {
        Ok(Self {
            image: image.shader_heap_index()?,
            sampler: sampler.shader_heap_index()?,
        })
    }
}

/// Converts a descriptor byte offset into the element index used by direct
/// descriptor-heap shader access.
///
/// The allocation size is the compiler ABI stride, while descriptor writes
/// continue to use the exact driver-reported descriptor byte size.
fn shader_heap_element_index(offset: u64, stride: u64, label: &str) -> Result<u32> {
    super::descriptor_heap_element_index(offset, stride).map_err(|error| {
        Error::Validation(format!("resolve {label} descriptor heap index: {error}"))
    })
}

/// One sampled-image descriptor allocation in a resource heap.
///
/// This is the reusable image half of [`SampledTextureBinding`]. It allows a
/// texture cache to pair many images with one [`SamplerBinding`] instead of
/// duplicating identical sampler descriptors for every resident image.
#[derive(Debug)]
pub struct SampledImageBinding {
    image: DescriptorAllocation,
}

impl SampledImageBinding {
    /// Allocates and writes one `SAMPLED_IMAGE` descriptor.
    ///
    /// `layout` must match the image state at shader access time. The view and
    /// its parent image must remain alive through the last submitted use.
    pub fn new(
        resource_heap: &DescriptorHeap,
        view: &ImageView,
        layout: vk::ImageLayout,
    ) -> Result<Self> {
        validate_resource_heap_and_view(resource_heap, view)?;
        Self::new_with_view_create_info(resource_heap, &view.create_info(), layout)
    }

    /// Allocates and writes a sampled-image descriptor for an imported
    /// dma-buf image.
    ///
    /// The imported image must remain alive through the last submitted use;
    /// attach it to each consuming encoder with
    /// `CommandEncoder::retain_resource`.
    pub fn new_imported_dma_buf(
        resource_heap: &DescriptorHeap,
        image: &ImportedDmaBufImage,
        layout: vk::ImageLayout,
    ) -> Result<Self> {
        validate_resource_heap_and_owner(resource_heap, image.owner())?;
        Self::new_with_view_create_info(resource_heap, &image.view_create_info(), layout)
    }

    /// Allocates and writes a sampled-image descriptor for an exportable
    /// dma-buf image.
    ///
    /// The exported image must remain alive through the last submitted use;
    /// attach it to each consuming encoder with
    /// `CommandEncoder::retain_resource`.
    pub fn new_exported_dma_buf(
        resource_heap: &DescriptorHeap,
        image: &ExportedDmaBufImage,
        layout: vk::ImageLayout,
    ) -> Result<Self> {
        validate_resource_heap_and_owner(resource_heap, image.owner())?;
        Self::new_with_view_create_info(resource_heap, &image.view_create_info(), layout)
    }

    fn new_with_view_create_info(
        resource_heap: &DescriptorHeap,
        view_create_info: &vk::ImageViewCreateInfo,
        layout: vk::ImageLayout,
    ) -> Result<Self> {
        let image = resource_heap
            .allocate(HeapDescriptorType::SampledImage)
            .map_err(descriptor_error)?;
        if let Err(error) = unsafe {
            resource_heap.write_image(
                &image,
                HeapDescriptorType::SampledImage,
                view_create_info,
                layout,
            )
        } {
            let _ = resource_heap.release(image);
            return Err(error);
        }
        Ok(Self { image })
    }

    /// Byte offset of this descriptor in its resource heap.
    pub const fn offset(&self) -> u64 {
        self.image.offset()
    }

    /// Checked 32-bit descriptor byte offset for a push-index mapping.
    pub fn push_index_heap_offset(&self) -> Result<u32> {
        u32::try_from(self.offset())
            .map_err(|_| Error::Validation("sampled-image descriptor offset exceeds u32".into()))
    }

    /// Element index of this descriptor for direct descriptor-heap shader
    /// access.
    ///
    /// `SPV_EXT_descriptor_heap` shaders (e.g. Slang `DescriptorHandle<T>`)
    /// index the resource heap builtins with the compiler's unified resource
    /// stride, so the pushed value is an element index rather than
    /// the byte offset used by [`Self::push_index_heap_offset`].
    pub fn shader_heap_index(&self) -> Result<u32> {
        shader_heap_element_index(self.image.offset(), self.image.size(), "sampled-image")
    }

    /// Retires this descriptor after its final submitted use.
    pub fn retire(
        self,
        resource_heap: &DescriptorHeap,
        after: FrameToken,
    ) -> std::result::Result<(), DescriptorHeapError> {
        if resource_heap.kind() != DescriptorHeapKind::Resource || !resource_heap.owns(&self.image)
        {
            return Err(DescriptorHeapError::WrongAllocator);
        }
        resource_heap.retire(self.image, after)
    }

    /// Releases an allocation that has never been visible to submitted work.
    pub fn release(
        self,
        resource_heap: &DescriptorHeap,
    ) -> std::result::Result<(), DescriptorHeapError> {
        if resource_heap.kind() != DescriptorHeapKind::Resource || !resource_heap.owns(&self.image)
        {
            return Err(DescriptorHeapError::WrongAllocator);
        }
        resource_heap.release(self.image)
    }
}

/// One sampler descriptor allocation in a sampler heap.
///
/// A renderer can keep this allocation for its filtering policy and reuse its
/// offset with every compatible [`SampledImageBinding`].
#[derive(Debug)]
pub struct SamplerBinding {
    sampler: DescriptorAllocation,
}

impl SamplerBinding {
    /// Allocates and writes one validated sampler descriptor.
    pub fn new(sampler_heap: &DescriptorHeap, sampler: SamplerDescriptor) -> Result<Self> {
        let sampler_info = sampler.to_vk()?;
        // SAFETY: `SamplerDescriptor` produces a validated, self-contained
        // VkSamplerCreateInfo with no borrowed pNext chain.
        unsafe { Self::new_raw(sampler_heap, &sampler_info) }
    }

    /// Raw interoperability variant of [`Self::new`].
    ///
    /// # Safety
    ///
    /// `sampler` and any pNext chain it references must be valid for
    /// `vkWriteSamplerDescriptorsEXT` during this call.
    pub unsafe fn new_raw(
        sampler_heap: &DescriptorHeap,
        sampler: &vk::SamplerCreateInfo,
    ) -> Result<Self> {
        if sampler_heap.kind() != DescriptorHeapKind::Sampler {
            return Err(Error::Validation(
                "sampler bindings require a sampler descriptor heap".into(),
            ));
        }
        let allocation = sampler_heap
            .allocate(HeapDescriptorType::Sampler)
            .map_err(descriptor_error)?;
        if let Err(error) = unsafe { sampler_heap.write_sampler(&allocation, sampler) } {
            let _ = sampler_heap.release(allocation);
            return Err(error);
        }
        Ok(Self {
            sampler: allocation,
        })
    }

    /// Byte offset of this descriptor in its sampler heap.
    pub const fn offset(&self) -> u64 {
        self.sampler.offset()
    }

    /// Checked 32-bit descriptor byte offset for a push-index mapping.
    pub fn push_index_heap_offset(&self) -> Result<u32> {
        u32::try_from(self.offset())
            .map_err(|_| Error::Validation("sampler descriptor offset exceeds u32".into()))
    }

    /// Element index of this descriptor for direct descriptor-heap shader
    /// access; see [`SampledImageBinding::shader_heap_index`].
    pub fn shader_heap_index(&self) -> Result<u32> {
        shader_heap_element_index(self.sampler.offset(), self.sampler.size(), "sampler")
    }

    /// Retires this descriptor after its final submitted use.
    pub fn retire(
        self,
        sampler_heap: &DescriptorHeap,
        after: FrameToken,
    ) -> std::result::Result<(), DescriptorHeapError> {
        if sampler_heap.kind() != DescriptorHeapKind::Sampler || !sampler_heap.owns(&self.sampler) {
            return Err(DescriptorHeapError::WrongAllocator);
        }
        sampler_heap.retire(self.sampler, after)
    }

    /// Releases an allocation that has never been visible to submitted work.
    pub fn release(
        self,
        sampler_heap: &DescriptorHeap,
    ) -> std::result::Result<(), DescriptorHeapError> {
        if sampler_heap.kind() != DescriptorHeapKind::Sampler || !sampler_heap.owns(&self.sampler) {
            return Err(DescriptorHeapError::WrongAllocator);
        }
        sampler_heap.release(self.sampler)
    }
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
    image: SampledImageBinding,
    sampler: SamplerBinding,
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
        let image = SampledImageBinding::new(resource_heap, view, layout)?;
        let sampler_allocation = match unsafe { SamplerBinding::new_raw(sampler_heap, sampler) } {
            Ok(allocation) => allocation,
            Err(error) => {
                let _ = image.release(resource_heap);
                return Err(error);
            }
        };
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
        SampledTextureHeapOffsets::from_bindings(&self.image, &self.sampler)
    }

    /// Returns descriptor element indices for direct descriptor-heap shader
    /// access; see [`SampledImageBinding::shader_heap_index`].
    pub fn shader_heap_indices(&self) -> Result<SampledTextureHeapIndices> {
        SampledTextureHeapIndices::from_bindings(&self.image, &self.sampler)
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
        self.image.retire(resource_heap, after)?;
        self.sampler.retire(sampler_heap, after)
    }

    /// Immediately returns both ranges when no command buffer has referenced
    /// them. Submitted descriptors MUST use [`Self::retire`] instead.
    pub fn release(
        self,
        resource_heap: &DescriptorHeap,
        sampler_heap: &DescriptorHeap,
    ) -> std::result::Result<(), DescriptorHeapError> {
        validate_owned_heaps(resource_heap, sampler_heap, &self)?;
        self.image.release(resource_heap)?;
        self.sampler.release(sampler_heap)
    }
}

fn validate_heaps_and_view(
    resource_heap: &DescriptorHeap,
    sampler_heap: &DescriptorHeap,
    view: &ImageView,
) -> Result<()> {
    validate_resource_heap_and_view(resource_heap, view)?;
    if sampler_heap.kind() != DescriptorHeapKind::Sampler {
        return Err(Error::Validation(
            "sampled textures require a sampler descriptor heap".into(),
        ));
    }
    if !Arc::ptr_eq(&resource_heap.owner, &sampler_heap.owner) {
        return Err(Error::Validation(
            "sampled texture view and descriptor heaps must share one Device".into(),
        ));
    }
    Ok(())
}

fn validate_resource_heap_and_view(resource_heap: &DescriptorHeap, view: &ImageView) -> Result<()> {
    validate_resource_heap_and_owner(resource_heap, view.owner())
}

fn validate_resource_heap_and_owner(
    resource_heap: &DescriptorHeap,
    owner: &Arc<crate::backend::DeviceOwner>,
) -> Result<()> {
    if resource_heap.kind() != DescriptorHeapKind::Resource {
        return Err(Error::Validation(
            "sampled images require a resource descriptor heap".into(),
        ));
    }
    if !Arc::ptr_eq(owner, &resource_heap.owner) {
        return Err(Error::Validation(
            "sampled image view and descriptor heap must share one Device".into(),
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
        || !resource_heap.owns(&binding.image.image)
        || !sampler_heap.owns(&binding.sampler.sampler)
    {
        return Err(DescriptorHeapError::WrongAllocator);
    }
    Ok(())
}

fn descriptor_error(error: DescriptorHeapError) -> Error {
    Error::Validation(format!("allocate sampled-texture descriptor: {error}"))
}

#[cfg(test)]
mod tests;
