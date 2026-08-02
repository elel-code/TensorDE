use super::*;

pub(super) const SCALE_SOURCE: ResourceId = ResourceId(2);
pub(super) const SCALE_TARGET: ResourceId = ResourceId(3);
pub(super) const SCALE_SOURCE_UPLOAD: PassId = PassId(10);
pub(super) const SCALE_TARGET_CLEAR: PassId = PassId(11);
pub(super) const SCALE_BLIT: PassId = PassId(12);
pub(super) const SCALE_SAMPLE: PassId = PassId(13);

pub(super) struct DecodedBitmap {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) pixels: Vec<u8>,
}

pub(super) fn decode_bitmap(slot: &IconGpuSlot) -> Option<DecodedBitmap> {
    let IconGpuSource::File { path, .. } = slot.source.as_ref()? else {
        return None;
    };
    if is_svg_path(path) {
        return None;
    }
    match LoadedIconSource::load(path)? {
        LoadedIconSource::Bitmap {
            width,
            height,
            pixels,
        } => Some(DecodedBitmap {
            width,
            height,
            pixels,
        }),
        LoadedIconSource::Svg { .. } => None,
    }
}

pub(super) fn fit_bitmap_blit(source: Extent3D, target: Extent3D) -> ImageBlit {
    let scale = (target.width as f64 / source.width as f64)
        .min(target.height as f64 / source.height as f64);
    let width = (source.width as f64 * scale)
        .round()
        .clamp(1.0, target.width as f64) as i32;
    let height = (source.height as f64 * scale)
        .round()
        .clamp(1.0, target.height as f64) as i32;
    let x = (target.width as i32 - width) / 2;
    let y = (target.height as i32 - height) / 2;
    ImageBlit {
        source_subresource: color_layers(),
        source_offsets: [
            vk::Offset3D::default(),
            vk::Offset3D {
                x: source.width as i32,
                y: source.height as i32,
                z: 1,
            },
        ],
        destination_subresource: color_layers(),
        destination_offsets: [
            vk::Offset3D { x, y, z: 0 },
            vk::Offset3D {
                x: x + width,
                y: y + height,
                z: 1,
            },
        ],
    }
}

pub(super) fn compile_scale_graph(queue_family: u32) -> Result<CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    for resource in [SCALE_SOURCE, SCALE_TARGET] {
        graph.set_initial_state(
            resource,
            ResourceKind::Image,
            ResourceState::image(RenderGraphImageState::Undefined, queue_family),
        );
    }
    graph.add_pass(RenderPass {
        id: SCALE_SOURCE_UPLOAD,
        label: "fika-vulkan-icon-scale-source-upload".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: SCALE_SOURCE,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::image(RenderGraphImageState::CopyDestination, queue_family),
        }],
    });
    graph.add_pass(RenderPass {
        id: SCALE_TARGET_CLEAR,
        label: "fika-vulkan-icon-scale-target-clear".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: SCALE_TARGET,
            kind: ResourceKind::Image,
            access: AccessKind::Write,
            state: ResourceState::image(RenderGraphImageState::ClearDestination, queue_family),
        }],
    });
    graph.add_pass(RenderPass {
        id: SCALE_BLIT,
        label: "fika-vulkan-icon-scale-blit".into(),
        depends_on: vec![SCALE_SOURCE_UPLOAD, SCALE_TARGET_CLEAR],
        resources: vec![
            ResourceUse {
                resource: SCALE_SOURCE,
                kind: ResourceKind::Image,
                access: AccessKind::Read,
                state: ResourceState::image(RenderGraphImageState::BlitSource, queue_family),
            },
            ResourceUse {
                resource: SCALE_TARGET,
                kind: ResourceKind::Image,
                access: AccessKind::Write,
                state: ResourceState::image(RenderGraphImageState::BlitDestination, queue_family),
            },
        ],
    });
    graph.add_pass(RenderPass {
        id: SCALE_SAMPLE,
        label: "fika-vulkan-icon-scale-sample".into(),
        depends_on: vec![SCALE_BLIT],
        resources: vec![ResourceUse {
            resource: SCALE_TARGET,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::image(RenderGraphImageState::FragmentSampledRead, queue_family),
        }],
    });
    graph
        .compile()
        .map_err(|error| format!("compile Vulkan icon scale graph: {error}"))
}
