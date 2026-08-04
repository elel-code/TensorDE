use super::*;

pub(super) fn create_pipeline(
    device: &Device,
    cache: &PipelineCache,
    format: TextureFormat,
) -> Result<GraphicsPipeline, String> {
    let vertex_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("tensor-files-vulkan-icon-vertex".into()),
            spirv: vulkan_icon_spirv::VERTEX.to_vec(),
        })
        .map_err(|error| format!("create Vulkan icon vertex shader: {error}"))?;
    let fragment_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("tensor-files-vulkan-icon-fragment".into()),
            spirv: vulkan_icon_spirv::FRAGMENT.to_vec(),
        })
        .map_err(|error| format!("create Vulkan icon fragment shader: {error}"))?;
    let vertex_bindings = ShaderBindingMap::default();
    // The Slang fragment shader selects its texture and sampler through
    // direct descriptor-heap indices in push data, so no binding map exists.
    let fragment_bindings = ShaderBindingMap::default();
    let attributes = [
        VertexAttribute {
            format: vulkan_renderer::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: vulkan_renderer::VertexFormat::Float32x2,
            offset: 8,
            shader_location: 1,
        },
        VertexAttribute {
            format: vulkan_renderer::VertexFormat::Float32x4,
            offset: 16,
            shader_location: 2,
        },
        VertexAttribute {
            format: vulkan_renderer::VertexFormat::Float32x2,
            offset: 32,
            shader_location: 3,
        },
    ];
    let buffers = [VertexBufferLayout {
        slot: 0,
        array_stride: std::mem::size_of::<IconVertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    }];
    let targets = [Some(ColorTargetState {
        format,
        blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        write_mask: vulkan_renderer::ColorWrites::ALL,
    })];
    device
        .create_graphics_pipeline(&GraphicsPipelineDescriptor {
            label: Some("tensor-files-vulkan-icon-pipeline"),
            vertex: VertexState {
                stage: ProgrammableStage {
                    module: &vertex_shader,
                    entry_point: c"main",
                    bindings: &vertex_bindings,
                },
                buffers: &buffers,
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: FragmentState {
                stage: ProgrammableStage {
                    module: &fragment_shader,
                    entry_point: c"main",
                    bindings: &fragment_bindings,
                },
                targets: &targets,
            },
            advanced_blend: None,
            local_read_mapping: None,
            cache: Some(cache),
        })
        .map_err(|error| format!("create Vulkan icon pipeline: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_vertex_layout_matches_native_shader_contract() {
        assert_eq!(std::mem::size_of::<IconVertex>(), 40);
        assert_eq!(IMAGE_PUSH_OFFSET, 0);
    }

    #[test]
    fn resident_heap_keeps_four_full_generations() {
        assert_eq!(descriptor_capacity(48, 32, 2048).unwrap(), 131_072);
        assert!(descriptor_capacity(32, 0, 1).is_err());
    }

    #[test]
    fn fresh_icon_upload_transitions_to_shader_read() {
        let graph = compile_upload_graph(3).unwrap();
        assert_eq!(graph.barriers.len(), 2);
        assert_eq!(
            graph.barriers[0].source.image_state(),
            Some(RenderGraphImageState::Undefined)
        );
        assert_eq!(
            graph.barriers[1].destination.image_state(),
            Some(RenderGraphImageState::FragmentSampledRead)
        );
    }

    #[test]
    fn bitmap_scale_graph_orders_clear_blit_and_sample() {
        let graph = compile_scale_graph(4).unwrap();
        assert_eq!(graph.barriers.len(), 5);
        assert!(graph.barriers.iter().any(|barrier| {
            barrier.after == SCALE_BLIT
                && barrier.resource == SCALE_TARGET
                && barrier.source.image_state() == Some(RenderGraphImageState::ClearDestination)
                && barrier.destination.image_state() == Some(RenderGraphImageState::BlitDestination)
        }));
        assert!(graph.barriers.iter().any(|barrier| {
            barrier.after == SCALE_SAMPLE
                && barrier.destination.image_state()
                    == Some(RenderGraphImageState::FragmentSampledRead)
        }));
    }

    #[test]
    fn bitmap_blit_preserves_aspect_ratio_and_centers_letterbox() {
        let blit = fit_bitmap_blit(Extent3D::new(400, 200, 1), Extent3D::new(128, 128, 1));
        assert_eq!(blit.destination_offsets[0], Origin3D::new(0, 32, 0));
        assert_eq!(blit.destination_offsets[1], Origin3D::new(128, 96, 1));
    }
}
