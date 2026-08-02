use super::*;

pub(super) enum VulkanIconImage {
    Resident { _image: Image, view: ImageView },
    Imported(ImportedDmaBufImage),
}

impl VulkanIconImage {
    pub(super) fn retain(&self, rendering: &mut RenderingEncoder<'_>) {
        match self {
            Self::Resident { view, .. } => rendering.retain_resource(view),
            Self::Imported(image) => rendering.retain_resource(image),
        }
    }
}

pub(super) fn create_rgba_image(
    allocator: &MemoryAllocator,
    label: &str,
    width: u32,
    height: u32,
    usage: TextureUsages,
) -> Result<Image, String> {
    allocator
        .create_image(&ImageDescriptor {
            label: Some(label.into()),
            dimension: ImageDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            extent: Extent3D::new(width.max(1), height.max(1), 1),
            mip_levels: 1,
            array_layers: 1,
            samples: SampleCount::One,
            tiling: ImageTiling::Optimal,
            usage,
            memory: MemoryLocation::Device,
        })
        .map_err(|error| format!("create Vulkan RGBA image {label}: {error}"))
}

pub(super) fn upload_rgba_pixels(
    uploads: &mut UploadBatch<'_>,
    image: &Image,
    pixels: &[u8],
) -> Result<(), String> {
    let extent = image.extent();
    unsafe {
        uploads
            .write_image_data(
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                ImageUpload {
                    data_layout: ImageDataLayout::tightly_packed(extent, TexelBlockLayout::RGBA8)
                        .map_err(|error| format!("layout Vulkan RGBA upload: {error}"))?,
                    texel_block: TexelBlockLayout::RGBA8,
                    image_subresource: color_layers(),
                    image_offset: vk::Offset3D::default(),
                    image_extent: extent,
                },
                pixels,
            )
            .map_err(|error| format!("upload Vulkan RGBA image: {error}"))?;
    }
    Ok(())
}

pub(super) const fn color_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        mip_level: 0,
        base_array_layer: 0,
        layer_count: 1,
    }
}

pub(super) fn descriptor_capacity(size: u64, alignment: u64, slots: u64) -> Result<u64, String> {
    if size == 0 || !alignment.is_power_of_two() || slots == 0 {
        return Err("Vulkan icon descriptor layout is unusable".into());
    }
    size.checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .and_then(|stride| stride.checked_mul(slots))
        .ok_or_else(|| "Vulkan icon descriptor capacity overflows".into())
}

pub(super) fn compile_upload_graph(queue_family: u32) -> Result<CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        ICON_IMAGE,
        ResourceKind::Image,
        ResourceState::image(RenderGraphImageState::Undefined, queue_family),
    );
    graph.add_pass(RenderPass {
        id: ICON_UPLOAD,
        label: "fika-vulkan-icon-upload".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: ICON_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::image(RenderGraphImageState::CopyDestination, queue_family),
        }],
    });
    graph.add_pass(RenderPass {
        id: ICON_SAMPLE,
        label: "fika-vulkan-icon-sample".into(),
        depends_on: vec![ICON_UPLOAD],
        resources: vec![ResourceUse {
            resource: ICON_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::image(RenderGraphImageState::FragmentSampledRead, queue_family),
        }],
    });
    graph
        .compile()
        .map_err(|error| format!("compile Vulkan icon upload graph: {error}"))
}
