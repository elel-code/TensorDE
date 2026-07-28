use super::*;

pub(super) fn create_pipeline(
    device: &Device,
    cache: &PipelineCache,
    format: vk::Format,
) -> Result<GraphicsPipeline, String> {
    let vertex_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-icon-vertex".into()),
            spirv: vulkan_icon_spirv::VERTEX.to_vec(),
        })
        .map_err(|error| format!("create Vulkan icon vertex shader: {error}"))?;
    let fragment_shader = device
        .create_shader_module(ShaderModuleDescriptor {
            label: Some("fika-vulkan-icon-fragment".into()),
            spirv: vulkan_icon_spirv::FRAGMENT.to_vec(),
        })
        .map_err(|error| format!("create Vulkan icon fragment shader: {error}"))?;
    let vertex_bindings = ShaderBindingMap::default();
    let fragment_bindings = SampledTextureShaderBindings::new(0, 0, 1)
        .push_index_shader_binding_map(IMAGE_PUSH_OFFSET, SAMPLER_PUSH_OFFSET)
        .map_err(|error| format!("create Vulkan icon shader mapping: {error}"))?;
    let attributes = [
        VertexAttribute {
            format: vk::Format::R32G32_SFLOAT,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: vk::Format::R32G32_SFLOAT,
            offset: 8,
            shader_location: 1,
        },
        VertexAttribute {
            format: vk::Format::R32G32B32A32_SFLOAT,
            offset: 16,
            shader_location: 2,
        },
        VertexAttribute {
            format: vk::Format::R32G32_SFLOAT,
            offset: 32,
            shader_location: 3,
        },
    ];
    let buffers = [VertexBufferLayout {
        array_stride: std::mem::size_of::<IconVertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    }];
    let targets = [Some(ColorTargetState {
        format,
        blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        write_mask: vk::ColorComponentFlags::R
            | vk::ColorComponentFlags::G
            | vk::ColorComponentFlags::B
            | vk::ColorComponentFlags::A,
    })];
    device
        .create_graphics_pipeline(&GraphicsPipelineDescriptor {
            label: Some("fika-vulkan-icon-pipeline"),
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
        assert_eq!(SAMPLER_PUSH_OFFSET, 4);
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
        assert_eq!(graph.barriers[0].source.layout, vk::ImageLayout::UNDEFINED);
        assert_eq!(
            graph.barriers[1].destination.layout,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
    }

    #[test]
    fn bitmap_scale_graph_orders_clear_blit_and_sample() {
        let graph = compile_scale_graph(4).unwrap();
        assert_eq!(graph.barriers.len(), 5);
        assert!(graph.barriers.iter().any(|barrier| {
            barrier.after == SCALE_BLIT
                && barrier.resource == SCALE_TARGET
                && barrier.source.access == vk::AccessFlags2::TRANSFER_WRITE
                && barrier.destination.access == vk::AccessFlags2::TRANSFER_WRITE
        }));
        assert!(graph.barriers.iter().any(|barrier| {
            barrier.after == SCALE_SAMPLE
                && barrier.destination.layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        }));
    }

    #[test]
    fn bitmap_blit_preserves_aspect_ratio_and_centers_letterbox() {
        let blit = fit_bitmap_blit(
            vk::Extent3D {
                width: 400,
                height: 200,
                depth: 1,
            },
            vk::Extent3D {
                width: 128,
                height: 128,
                depth: 1,
            },
        );
        assert_eq!(
            blit.destination_offsets[0],
            vk::Offset3D { x: 0, y: 32, z: 0 }
        );
        assert_eq!(
            blit.destination_offsets[1],
            vk::Offset3D {
                x: 128,
                y: 96,
                z: 1
            }
        );
    }
}
