use std::collections::BTreeMap;

use vulkan_renderer::{
    AccessKind, BarrierBatch, Buffer, PassId, RenderGraph, RenderGraphImageState, RenderPass,
    ResourceBinding, ResourceId, ResourceKind, ResourceState, ResourceUse,
};

const RENDER_PASS: PassId = PassId(2);
const PRESENT_PASS: PassId = PassId(3);
const SURFACE_IMAGE: ResourceId = ResourceId(1);
const FIRST_VERTEX_BUFFER: u64 = 2;
const FIRST_UPLOAD_PASS: u32 = 10;

pub(crate) struct FrameBarriers {
    pub(crate) before_render: BarrierBatch,
    pub(crate) before_present: BarrierBatch,
}

pub(crate) struct FrameVertexBuffer<'a> {
    pub(crate) buffer: &'a Buffer,
    pub(crate) uploaded: bool,
}

pub(crate) fn compile_frame_barriers(
    surface_binding: ResourceBinding,
    image_initialized: bool,
    vertex_buffers: &[FrameVertexBuffer<'_>],
    queue_family: u32,
) -> Result<FrameBarriers, String> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        SURFACE_IMAGE,
        ResourceKind::Image,
        ResourceState::image(
            if image_initialized {
                RenderGraphImageState::Present
            } else {
                RenderGraphImageState::Undefined
            },
            queue_family,
        ),
    );
    let mut render_dependencies = Vec::with_capacity(vertex_buffers.len());
    let mut render_resources = vec![ResourceUse {
        resource: SURFACE_IMAGE,
        kind: ResourceKind::Image,
        access: AccessKind::Write,
        state: ResourceState::color_attachment_write(queue_family),
    }];
    for (index, vertex_buffer) in vertex_buffers.iter().enumerate() {
        let index = u32::try_from(index)
            .map_err(|_| "Vulkan frame has too many vertex buffer streams".to_string())?;
        let resource = ResourceId(FIRST_VERTEX_BUFFER + u64::from(index));
        // Dynamic buffers return to vertex-read state at the end of every
        // frame. Declaring that carried state emits the required read-to-write
        // dependency before an in-place upload; a freshly reallocated buffer
        // merely receives the same harmless execution dependency.
        graph.set_initial_state(
            resource,
            ResourceKind::Buffer,
            ResourceState::vertex_buffer(queue_family),
        );
        if vertex_buffer.uploaded {
            let upload_pass = PassId(
                FIRST_UPLOAD_PASS
                    .checked_add(index)
                    .ok_or_else(|| "Vulkan frame upload pass ID overflows".to_string())?,
            );
            graph.add_pass(RenderPass {
                id: upload_pass,
                label: format!("fika-vulkan-vertex-upload-{index}"),
                depends_on: Vec::new(),
                resources: vec![ResourceUse {
                    resource,
                    kind: ResourceKind::Buffer,
                    access: AccessKind::Write,
                    state: ResourceState::buffer_copy_destination(queue_family),
                }],
            });
            render_dependencies.push(upload_pass);
        }
        render_resources.push(ResourceUse {
            resource,
            kind: ResourceKind::Buffer,
            access: AccessKind::Read,
            state: ResourceState::vertex_buffer(queue_family),
        });
    }
    graph.add_pass(RenderPass {
        id: RENDER_PASS,
        label: "fika-vulkan-render".into(),
        depends_on: render_dependencies,
        resources: render_resources,
    });
    graph.add_pass(RenderPass {
        id: PRESENT_PASS,
        label: "fika-vulkan-present".into(),
        depends_on: vec![RENDER_PASS],
        resources: vec![ResourceUse {
            resource: SURFACE_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::present(queue_family),
        }],
    });
    let graph = graph
        .compile()
        .map_err(|error| format!("compile Vulkan frame graph: {error}"))?;
    let mut bindings = BTreeMap::from([(SURFACE_IMAGE, surface_binding)]);
    for (index, vertex_buffer) in vertex_buffers.iter().enumerate() {
        let resource = ResourceId(
            FIRST_VERTEX_BUFFER
                + u64::try_from(index)
                    .map_err(|_| "Vulkan frame vertex resource ID overflows".to_string())?,
        );
        bindings.insert(
            resource,
            ResourceBinding::whole_buffer(vertex_buffer.buffer),
        );
    }
    Ok(FrameBarriers {
        before_render: graph
            .barrier_batch_before(RENDER_PASS, &bindings)
            .map_err(|error| format!("resolve Vulkan render barriers: {error}"))?,
        before_present: graph
            .barrier_batch_before(PRESENT_PASS, &bindings)
            .map_err(|error| format!("resolve Vulkan present barrier: {error}"))?,
    })
}
