use super::{
    DescriptorAllocation, DescriptorHeap, DescriptorHeapError, DescriptorHeapKind,
    HeapDescriptorType, descriptor_heap_element_index,
};
use crate::{
    Buffer, BufferUsages, Error, FrameToken, ImageView, Result, RetainedExternalImage,
    TextureLayout, TextureUsages,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferDescriptorKind {
    Uniform,
    Storage,
}

/// Resource-heap descriptor class for a retained but intentionally unwritten
/// shader lane.
///
/// A reserved lane preserves the exact direct-heap element ABI for a pipeline
/// layout union. It is valid only when the submitted shader path is proven not
/// to access that lane; this type does not create a null descriptor fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorSlotKind {
    UniformBuffer,
    StorageBuffer,
    SampledImage,
    StorageImage,
    InputAttachment,
}

impl DescriptorSlotKind {
    const fn heap_type(self) -> HeapDescriptorType {
        match self {
            Self::UniformBuffer => HeapDescriptorType::UniformBuffer,
            Self::StorageBuffer => HeapDescriptorType::StorageBuffer,
            Self::SampledImage => HeapDescriptorType::SampledImage,
            Self::StorageImage => HeapDescriptorType::StorageImage,
            Self::InputAttachment => HeapDescriptorType::InputAttachment,
        }
    }
}

/// Owns one exact resource-heap element without writing descriptor contents.
///
/// This is not a null descriptor. Command submission must select a shader path
/// that cannot access this element.
#[derive(Debug)]
pub struct ReservedDescriptorBinding {
    allocation: DescriptorAllocation,
    kind: DescriptorSlotKind,
}

impl ReservedDescriptorBinding {
    pub fn new(heap: &DescriptorHeap, kind: DescriptorSlotKind) -> Result<Self> {
        if heap.kind() != DescriptorHeapKind::Resource {
            return Err(Error::Validation(
                "reserved resource descriptor requires a resource heap".into(),
            ));
        }
        let allocation = heap.allocate(kind.heap_type()).map_err(descriptor_error)?;
        Ok(Self { allocation, kind })
    }

    pub const fn offset(&self) -> u64 {
        self.allocation.offset()
    }

    pub fn shader_heap_index(&self) -> Result<u32> {
        descriptor_heap_element_index(self.offset(), self.allocation.size())
            .map_err(|error| Error::Validation(error.to_string()))
    }

    pub const fn kind(&self) -> DescriptorSlotKind {
        self.kind
    }

    pub fn retire(
        self,
        heap: &DescriptorHeap,
        after: FrameToken,
    ) -> std::result::Result<(), DescriptorHeapError> {
        validate_owner(heap, &self.allocation)?;
        heap.retire(self.allocation, after)
    }

    pub fn release(self, heap: &DescriptorHeap) -> std::result::Result<(), DescriptorHeapError> {
        validate_owner(heap, &self.allocation)?;
        heap.release(self.allocation)
    }
}

impl BufferDescriptorKind {
    const fn heap_type(self) -> HeapDescriptorType {
        match self {
            Self::Uniform => HeapDescriptorType::UniformBuffer,
            Self::Storage => HeapDescriptorType::StorageBuffer,
        }
    }

    const fn usage(self) -> BufferUsages {
        match self {
            Self::Uniform => BufferUsages::UNIFORM,
            Self::Storage => BufferUsages::STORAGE,
        }
    }
}

#[derive(Debug)]
pub struct BufferDescriptorBinding {
    allocation: DescriptorAllocation,
    kind: BufferDescriptorKind,
}

impl BufferDescriptorBinding {
    pub fn new(
        heap: &DescriptorHeap,
        buffer: &Buffer,
        kind: BufferDescriptorKind,
        offset: u64,
        size: u64,
    ) -> Result<Self> {
        if heap.kind() != DescriptorHeapKind::Resource || !buffer.belongs_to(&heap.owner) {
            return Err(Error::Validation(
                "buffer descriptor and resource heap must belong to the same Device".into(),
            ));
        }
        if !buffer.usage().contains(kind.usage())
            || !buffer.usage().contains(BufferUsages::SHADER_DEVICE_ADDRESS)
        {
            return Err(Error::Validation(
                "descriptor buffer is missing its shader usage or SHADER_DEVICE_ADDRESS".into(),
            ));
        }
        let end = offset
            .checked_add(size)
            .ok_or_else(|| Error::Validation("descriptor buffer range overflows".into()))?;
        if size == 0 || end > buffer.size() {
            return Err(Error::Validation(
                "descriptor buffer range is empty or exceeds the buffer".into(),
            ));
        }
        let address = buffer
            .device_address()
            .and_then(|address| address.checked_add(offset))
            .ok_or_else(|| Error::Validation("descriptor buffer address is unavailable".into()))?;
        let allocation = heap.allocate(kind.heap_type()).map_err(descriptor_error)?;
        if let Err(error) =
            unsafe { heap.write_buffer(&allocation, kind.heap_type(), address, size) }
        {
            let _ = heap.release(allocation);
            return Err(error);
        }
        Ok(Self { allocation, kind })
    }

    pub const fn offset(&self) -> u64 {
        self.allocation.offset()
    }

    pub fn shader_heap_index(&self) -> Result<u32> {
        descriptor_heap_element_index(self.offset(), self.allocation.size())
            .map_err(|error| Error::Validation(error.to_string()))
    }

    pub const fn kind(&self) -> BufferDescriptorKind {
        self.kind
    }

    pub fn retire(
        self,
        heap: &DescriptorHeap,
        after: FrameToken,
    ) -> std::result::Result<(), DescriptorHeapError> {
        validate_owner(heap, &self.allocation)?;
        heap.retire(self.allocation, after)
    }

    pub fn release(self, heap: &DescriptorHeap) -> std::result::Result<(), DescriptorHeapError> {
        validate_owner(heap, &self.allocation)?;
        heap.release(self.allocation)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageDescriptorKind {
    Sampled,
    Storage,
    InputAttachment,
}

impl ImageDescriptorKind {
    const fn heap_type(self) -> HeapDescriptorType {
        match self {
            Self::Sampled => HeapDescriptorType::SampledImage,
            Self::Storage => HeapDescriptorType::StorageImage,
            Self::InputAttachment => HeapDescriptorType::InputAttachment,
        }
    }

    const fn usage(self) -> TextureUsages {
        match self {
            Self::Sampled => TextureUsages::SAMPLED,
            Self::Storage => TextureUsages::STORAGE,
            Self::InputAttachment => TextureUsages::INPUT_ATTACHMENT,
        }
    }
}

#[derive(Debug)]
pub struct ImageDescriptorBinding {
    allocation: DescriptorAllocation,
    kind: ImageDescriptorKind,
}

/// Stable descriptor lane for a decoder/host-owned image selected at frame
/// time. The allocation is retained across rewrites; the currently bound
/// external-image lease is replaced only after the caller has proved the
/// descriptor's frame slot is no longer in flight.
#[derive(Debug)]
pub struct DynamicExternalImageDescriptorBinding {
    allocation: DescriptorAllocation,
    kind: ImageDescriptorKind,
    image: Option<RetainedExternalImage>,
}

impl DynamicExternalImageDescriptorBinding {
    pub fn reserve(heap: &DescriptorHeap, kind: ImageDescriptorKind) -> Result<Self> {
        if heap.kind() != DescriptorHeapKind::Resource {
            return Err(Error::Validation(
                "dynamic external image descriptor requires a resource heap".into(),
            ));
        }
        let allocation = heap.allocate(kind.heap_type()).map_err(descriptor_error)?;
        Ok(Self {
            allocation,
            kind,
            image: None,
        })
    }

    /// Writes one externally owned image into this stable lane and retains its
    /// host lease until a later synchronized rewrite or binding destruction.
    pub fn bind(
        &mut self,
        heap: &DescriptorHeap,
        image: RetainedExternalImage,
        layout: TextureLayout,
    ) -> Result<()> {
        if heap.kind() != DescriptorHeapKind::Resource
            || !heap.owns(&self.allocation)
            || !heap.belongs_to(image.owner())
        {
            return Err(Error::Validation(
                "external image descriptor, heap, and image must belong to the same Device".into(),
            ));
        }
        if !image.usage().contains(self.kind.usage()) {
            return Err(Error::Validation(
                "external image is missing the descriptor's required usage".into(),
            ));
        }
        validate_image_layout(self.kind, layout)?;
        image.with_view_create_info(|view| unsafe {
            heap.write_image(
                &self.allocation,
                self.kind.heap_type(),
                view,
                layout.to_vk(),
            )
        })?;
        self.image = Some(image);
        Ok(())
    }

    pub const fn is_bound(&self) -> bool {
        self.image.is_some()
    }

    pub fn retained_image(&self) -> Option<&RetainedExternalImage> {
        self.image.as_ref()
    }

    pub fn shader_heap_index(&self) -> Result<u32> {
        descriptor_heap_element_index(self.allocation.offset(), self.allocation.size())
            .map_err(|error| Error::Validation(error.to_string()))
    }

    pub fn retire(
        self,
        heap: &DescriptorHeap,
        after: FrameToken,
    ) -> std::result::Result<(), DescriptorHeapError> {
        validate_owner(heap, &self.allocation)?;
        heap.retire(self.allocation, after)
    }

    pub fn release(self, heap: &DescriptorHeap) -> std::result::Result<(), DescriptorHeapError> {
        validate_owner(heap, &self.allocation)?;
        heap.release(self.allocation)
    }
}

impl ImageDescriptorBinding {
    pub fn new(
        heap: &DescriptorHeap,
        view: &ImageView,
        kind: ImageDescriptorKind,
        layout: TextureLayout,
    ) -> Result<Self> {
        if heap.kind() != DescriptorHeapKind::Resource || !heap.belongs_to(view.owner()) {
            return Err(Error::Validation(
                "image descriptor and resource heap must belong to the same Device".into(),
            ));
        }
        if !view.usage().contains(kind.usage()) {
            return Err(Error::Validation(
                "image view is missing the usage required by its descriptor kind".into(),
            ));
        }
        validate_image_layout(kind, layout)?;
        let allocation = heap.allocate(kind.heap_type()).map_err(descriptor_error)?;
        if let Err(error) = unsafe {
            heap.write_image(
                &allocation,
                kind.heap_type(),
                &view.create_info(),
                layout.to_vk(),
            )
        } {
            let _ = heap.release(allocation);
            return Err(error);
        }
        Ok(Self { allocation, kind })
    }

    /// Rewrites this stable heap element to another compatible image view.
    ///
    /// The caller must synchronize descriptor mutation against every submitted
    /// command that can still read the previous descriptor contents.
    pub fn rewrite(
        &self,
        heap: &DescriptorHeap,
        view: &ImageView,
        layout: TextureLayout,
    ) -> Result<()> {
        if heap.kind() != DescriptorHeapKind::Resource
            || !heap.owns(&self.allocation)
            || !heap.belongs_to(view.owner())
        {
            return Err(Error::Validation(
                "image descriptor, resource heap, and view must belong to the same Device".into(),
            ));
        }
        if !view.usage().contains(self.kind.usage()) {
            return Err(Error::Validation(
                "image view is missing the usage required by its descriptor kind".into(),
            ));
        }
        validate_image_layout(self.kind, layout)?;
        unsafe {
            heap.write_image(
                &self.allocation,
                self.kind.heap_type(),
                &view.create_info(),
                layout.to_vk(),
            )
        }
    }

    pub const fn offset(&self) -> u64 {
        self.allocation.offset()
    }

    pub fn shader_heap_index(&self) -> Result<u32> {
        descriptor_heap_element_index(self.offset(), self.allocation.size())
            .map_err(|error| Error::Validation(error.to_string()))
    }

    pub const fn kind(&self) -> ImageDescriptorKind {
        self.kind
    }

    pub fn retire(
        self,
        heap: &DescriptorHeap,
        after: FrameToken,
    ) -> std::result::Result<(), DescriptorHeapError> {
        validate_owner(heap, &self.allocation)?;
        heap.retire(self.allocation, after)
    }

    pub fn release(self, heap: &DescriptorHeap) -> std::result::Result<(), DescriptorHeapError> {
        validate_owner(heap, &self.allocation)?;
        heap.release(self.allocation)
    }
}

fn validate_image_layout(kind: ImageDescriptorKind, layout: TextureLayout) -> Result<()> {
    let valid = match kind {
        ImageDescriptorKind::Sampled => matches!(
            layout,
            TextureLayout::ShaderReadOnly | TextureLayout::General
        ),
        ImageDescriptorKind::Storage => layout == TextureLayout::General,
        ImageDescriptorKind::InputAttachment => matches!(
            layout,
            TextureLayout::RenderingLocalRead | TextureLayout::General
        ),
    };
    if valid {
        Ok(())
    } else {
        Err(Error::Validation(format!(
            "{kind:?} descriptor does not support {layout:?} layout"
        )))
    }
}

fn validate_owner(
    heap: &DescriptorHeap,
    allocation: &DescriptorAllocation,
) -> std::result::Result<(), DescriptorHeapError> {
    if heap.kind() != DescriptorHeapKind::Resource || !heap.owns(allocation) {
        Err(DescriptorHeapError::WrongAllocator)
    } else {
        Ok(())
    }
}

fn descriptor_error(error: DescriptorHeapError) -> Error {
    Error::Validation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_image_descriptor_layouts_preserve_access_roles() {
        assert!(
            validate_image_layout(ImageDescriptorKind::Sampled, TextureLayout::ShaderReadOnly)
                .is_ok()
        );
        assert!(
            validate_image_layout(
                ImageDescriptorKind::InputAttachment,
                TextureLayout::RenderingLocalRead
            )
            .is_ok()
        );
        assert!(
            validate_image_layout(ImageDescriptorKind::Storage, TextureLayout::General).is_ok()
        );
    }

    #[test]
    fn typed_image_descriptor_layouts_reject_role_mismatches() {
        assert!(
            validate_image_layout(ImageDescriptorKind::Storage, TextureLayout::ShaderReadOnly)
                .is_err()
        );
        assert!(
            validate_image_layout(ImageDescriptorKind::Sampled, TextureLayout::ColorAttachment)
                .is_err()
        );
        assert!(
            validate_image_layout(
                ImageDescriptorKind::InputAttachment,
                TextureLayout::ShaderReadOnly
            )
            .is_err()
        );
    }

    #[test]
    fn reserved_slot_kinds_keep_exact_resource_descriptor_classes() {
        assert_eq!(
            DescriptorSlotKind::UniformBuffer.heap_type(),
            HeapDescriptorType::UniformBuffer
        );
        assert_eq!(
            DescriptorSlotKind::InputAttachment.heap_type(),
            HeapDescriptorType::InputAttachment
        );
    }
}
