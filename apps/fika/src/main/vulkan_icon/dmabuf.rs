use super::*;

impl VulkanIconRenderer {
    pub(super) fn import_dmabuf_texture(
        &self,
        device: &Device,
        uploads: &mut UploadBatch<'_>,
        slot: &mut IconGpuSlot,
    ) -> Result<Option<VulkanIconTexture>, String> {
        let Some(source) = slot.dmabuf.take() else {
            return Ok(None);
        };
        let Some((format, components)) =
            crate::ui::render::dmabuf::vulkan_format_for_fourcc(source.fourcc)
        else {
            return Ok(None);
        };
        let descriptor = DmaBufImageDescriptor {
            label: Some("fika-vulkan-imported-icon".into()),
            format,
            extent: vk::Extent2D {
                width: slot.width.max(1),
                height: slot.height.max(1),
            },
            modifier: source.plane.modifier,
            planes: vec![DmaBufPlaneLayout {
                offset: u64::from(source.plane.offset),
                row_pitch: u64::from(source.plane.stride),
            }],
            usage: vk::ImageUsageFlags::SAMPLED,
            components,
        };
        let imported = device
            .import_dma_buf_image(&descriptor, &source.plane.fd)
            .map_err(|error| format!("import Vulkan icon dma-buf: {error}"))?;
        let bindings = BTreeMap::from([(ICON_IMAGE, imported.resource_binding())]);
        let before_sample = self
            .import_graph
            .barrier_batch_before(ICON_IMPORT_SAMPLE, &bindings)
            .map_err(|error| format!("resolve Vulkan dma-buf icon acquire barrier: {error}"))?;
        unsafe { uploads.encoder_mut().pipeline_barrier(&before_sample) };
        uploads.encoder_mut().retain_resource(&imported);
        let binding = SampledImageBinding::new_imported_dma_buf(
            &self.resource_heap,
            &imported,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        )
        .map_err(|error| format!("create Vulkan imported icon descriptor: {error}"))?;
        Ok(Some(VulkanIconTexture {
            image: VulkanIconImage::Imported(imported),
            binding,
            width: slot.width.max(1),
            height: slot.height.max(1),
            content_width: slot.content_width,
            content_height: slot.content_height,
            content_hash: slot.content_hash,
            rounding: slot.rounding,
            last_used_frame: self.gpu_frame,
        }))
    }
}

pub(super) fn compile_import_graph(queue_family: u32) -> Result<CompiledGraph, String> {
    let mut graph = RenderGraph::default();
    graph.set_initial_state(
        ICON_IMAGE,
        ResourceKind::Image,
        ResourceState::foreign_image(vk::ImageLayout::GENERAL),
    );
    graph.add_pass(RenderPass {
        id: ICON_IMPORT_SAMPLE,
        label: "fika-vulkan-icon-import-sample".into(),
        depends_on: Vec::new(),
        resources: vec![ResourceUse {
            resource: ICON_IMAGE,
            kind: ResourceKind::Image,
            access: AccessKind::Read,
            state: ResourceState::image(
                vk::PipelineStageFlags2::FRAGMENT_SHADER,
                vk::AccessFlags2::SHADER_SAMPLED_READ,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                queue_family,
            ),
        }],
    });
    graph
        .compile()
        .map_err(|error| format!("compile Vulkan dma-buf icon import graph: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_graph_acquires_foreign_general_image_for_fragment_sampling() {
        let graph = compile_import_graph(7).unwrap();
        assert_eq!(graph.barriers.len(), 1);
        let barrier = graph.barriers[0];
        assert_eq!(
            barrier.source,
            ResourceState::foreign_image(vk::ImageLayout::GENERAL)
        );
        assert_eq!(barrier.destination.queue_family, 7);
        assert_eq!(
            barrier.destination.layout,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
        assert_eq!(
            barrier.destination.access,
            vk::AccessFlags2::SHADER_SAMPLED_READ
        );
    }
}
