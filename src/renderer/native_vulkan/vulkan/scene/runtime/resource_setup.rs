use super::*;

pub(super) fn create_scene_gpu_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    setup_command_buffer: vk::CommandBuffer,
    storage: &SceneStorage,
    target_format: vk::Format,
    initial_scene_color_image: vk::Image,
    extent: vk::Extent2D,
    max_sampler_anisotropy_x1: u32,
    descriptor_heap_properties: &crate::renderer::native_vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    advanced_blend_enabled: bool,
    advanced_blend_coherent: bool,
    capture_scene_graph: Option<u32>,
) -> Result<SceneGpuResources, String> {
    let backend_plan = native_vulkan_scene_backend_plan(storage);
    if backend_plan.rendering_device_graph.mesh_draws.is_empty() {
        return Err("scene present requires at least one render graph mesh draw".to_owned());
    }
    let descriptor_layout =
        scene_pipeline_descriptor_layout(storage, &backend_plan.rendering_device_graph)?;
    let sampled_binding_cycle = scene_sampled_image_binding_cycle(
        &backend_plan.rendering_device_graph,
        &descriptor_layout.sampled_slots,
    )?;
    let sampled_binding_plan = sampled_binding_cycle
        .first()
        .ok_or_else(|| "scene sampled binding cycle is empty".to_owned())?;
    let effect_target_plans = effect_target::scene_effect_target_image_plan(
        storage,
        &backend_plan.rendering_device_graph,
        target_format,
        extent,
    )?;
    let effect_target_commands =
        effect_target::scene_effect_target_commands(storage, &backend_plan.rendering_device_graph);
    let effect_target_command_plan = effect_target::scene_effect_target_command_plan(
        &effect_target_commands,
        &backend_plan.rendering_device_graph,
    );
    let effect_target_allocations = backend_plan
        .rendering_device_graph
        .target_allocations
        .clone();
    let scene_color_ranges = scene_color_draw_ranges(&backend_plan.rendering_device_graph);
    let graph_execution_order = graph_execution::scene_graph_execution_order(
        &backend_plan.rendering_device_graph,
        capture_scene_graph,
    )?;
    let pipeline_indices = scene_pipeline_indices_for_draws(
        storage,
        &backend_plan.rendering_device_graph,
        target_format,
        &effect_target_plans,
    )?;
    emit_scene_pipeline_diagnostics_if_requested(
        storage,
        &backend_plan.rendering_device_graph,
        target_format,
        &effect_target_plans,
        &pipeline_indices,
    )?;
    let draw_count = backend_plan.rendering_device_graph.mesh_draws.len();
    let include_fullscreen_utility =
        graph_uses_fullscreen_utility_primitive(&backend_plan.rendering_device_graph);
    let alpha_coverage_scissors = if std::env::var_os(
        "GILDER_NATIVE_VULKAN_SCENE_FULL_ALPHA_COVERAGE_TARGET",
    )
    .is_some()
    {
        vec![Vec::new(); draw_count]
    } else {
        scene_alpha_coverage_scissors(
            storage,
            &backend_plan.rendering_device_graph,
            [extent.width, extent.height],
        )
    };
    let vertex_payload = pack_scene_vertices(storage, include_fullscreen_utility);
    let index_payload = pack_scene_indices(storage, include_fullscreen_utility);
    let transform_payload = pack_scene_draw_uniforms(
        storage,
        &backend_plan.rendering_device_graph.mesh_draws,
        0.0,
        [extent.width, extent.height],
    );
    let material_payload = descriptor_layout.material_uniform_enabled.then(|| {
        pack_scene_material_uniforms(
            storage,
            &backend_plan.rendering_device_graph.mesh_draws,
            0.0,
        )
    });
    let dynamic_effect_uniforms =
        backend_plan
            .rendering_device_graph
            .mesh_draws
            .iter()
            .any(|draw| {
                material_parameter_layout(storage, draw.material).uses_dynamic_material_input()
            });
    let skinning_payload = descriptor_layout
        .skinning_storage_enabled
        .then(|| pack_scene_skinning_palette(&backend_plan.rendering_device_graph));
    let frame_topology = SceneFrameTopology::from_graph(&backend_plan.rendering_device_graph);

    let vertex_buffer = native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        "scene-mesh-vertex-buffer",
        vertex_payload.len() as u64,
        vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        Some(&vertex_payload),
    )?;
    let index_buffer = match native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        "scene-mesh-index-buffer",
        index_payload.len() as u64,
        vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        Some(&index_payload),
    ) {
        Ok(buffer) => buffer,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };
    let transform_buffer = match native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        "scene-draw-transform-uniform-buffer",
        transform_payload.len() as u64,
        vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        Some(&transform_payload),
    ) {
        Ok(buffer) => buffer,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };
    let material_buffer = match material_payload.as_ref() {
        Some(payload) => match native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            "scene-material-uniform-buffer",
            payload.len() as u64,
            vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
            Some(payload),
        ) {
            Ok(buffer) => Some(buffer),
            Err(err) => {
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
                return Err(err);
            }
        },
        None => None,
    };
    let skinning_buffer = match skinning_payload.as_ref() {
        Some(payload) => match native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            "scene-puppet-bone-storage-buffer",
            payload.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
            Some(payload),
        ) {
            Ok(buffer) => Some(buffer),
            Err(err) => {
                if let Some(buffer) = material_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
                return Err(err);
            }
        },
        None => None,
    };

    let white_upload = if sampled_binding_plan.fallback_descriptor_count == 0 {
        None
    } else {
        match create_white_texture_upload(device, memory_properties, setup_command_buffer) {
            Ok(upload) => Some(upload),
            Err(err) => {
                if let Some(buffer) = skinning_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                if let Some(buffer) = material_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
                return Err(err);
            }
        }
    };

    let (resource_descriptors, draw_commands) = scene_descriptor_plan_inputs(
        &backend_plan.rendering_device_graph.mesh_draws,
        &descriptor_layout,
        &pipeline_indices,
        &alpha_coverage_scissors,
    );
    let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
        NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
            resource_descriptors,
            sampler_count: descriptor_layout
                .sampled_slots
                .len()
                .saturating_mul(draw_count),
            properties: *descriptor_heap_properties,
        },
    );
    if !descriptor_heap_plan.backend_ready {
        let err = format!(
            "scene descriptor heap plan is not ready: {:?}",
            descriptor_heap_plan.blocking_reason
        );
        if let Some(upload) = white_upload {
            destroy_recorded_image_upload(device, upload);
        }
        if let Some(buffer) = material_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = skinning_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
        native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
        native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
        return Err(err);
    }
    let mut descriptor_heap =
        match native_vulkan_vulkanalia_create_descriptor_heap_resource_resources(
            device,
            memory_properties,
            &descriptor_heap_plan,
        ) {
            Ok(resources) => resources,
            Err(err) => {
                if let Some(upload) = white_upload {
                    destroy_recorded_image_upload(device, upload);
                }
                if let Some(buffer) = material_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                if let Some(buffer) = skinning_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
                native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
                return Err(err);
            }
        };

    let effect_targets = match effect_target::create_scene_effect_target_images(
        device,
        memory_properties,
        &effect_target_plans,
    ) {
        Ok(targets) => targets,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
                device,
                descriptor_heap,
            );
            if let Some(upload) = white_upload {
                destroy_recorded_image_upload(device, upload);
            }
            if let Some(buffer) = material_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            if let Some(buffer) = skinning_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };
    effect_target::record_scene_effect_target_initial_layouts(
        device,
        setup_command_buffer,
        &effect_targets,
    );
    let scene_textures = match scene_texture::create_scene_texture_images(
        device,
        memory_properties,
        setup_command_buffer,
        storage,
        &sampled_binding_cycle,
        max_sampler_anisotropy_x1,
    ) {
        Ok(textures) => textures,
        Err(err) => {
            effect_target::destroy_scene_effect_target_images(device, effect_targets);
            native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
                device,
                descriptor_heap,
            );
            if let Some(upload) = white_upload {
                destroy_recorded_image_upload(device, upload);
            }
            if let Some(buffer) = material_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            if let Some(buffer) = skinning_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };

    if let Err(err) = write_scene_descriptors(
        device,
        &mut descriptor_heap,
        &draw_commands,
        &transform_buffer,
        material_buffer.as_ref(),
        skinning_buffer.as_ref(),
        white_upload.as_ref().map(|upload| &upload.image),
        &scene_textures,
        &effect_targets,
        sampled_binding_plan,
        Some((initial_scene_color_image, target_format)),
    ) {
        scene_texture::destroy_scene_texture_images(device, scene_textures);
        effect_target::destroy_scene_effect_target_images(device, effect_targets);
        native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
            device,
            descriptor_heap,
        );
        if let Some(upload) = white_upload {
            destroy_recorded_image_upload(device, upload);
        }
        if let Some(buffer) = material_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = skinning_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
        native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
        native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
        return Err(err);
    }
    let pipeline_resources = match create_scene_pipelines(
        device,
        target_format,
        extent,
        storage,
        &backend_plan.rendering_device_graph,
        &descriptor_heap_plan,
        &descriptor_layout,
        &effect_target_plans,
        advanced_blend_enabled,
        advanced_blend_coherent,
    ) {
        Ok(resources) => resources,
        Err(err) => {
            scene_texture::destroy_scene_texture_images(device, scene_textures);
            effect_target::destroy_scene_effect_target_images(device, effect_targets);
            native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
                device,
                descriptor_heap,
            );
            if let Some(upload) = white_upload {
                destroy_recorded_image_upload(device, upload);
            }
            if let Some(buffer) = material_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            if let Some(buffer) = skinning_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, index_buffer);
            native_vulkan_vulkanalia_destroy_buffer(device, vertex_buffer);
            return Err(err);
        }
    };

    Ok(SceneGpuResources {
        vertex_buffer,
        index_buffer,
        transform_buffer,
        material_buffer,
        skinning_buffer,
        white_upload,
        scene_textures,
        effect_targets,
        effect_target_command_plan,
        effect_target_commands,
        effect_target_allocations,
        pass_nodes: backend_plan.rendering_device_graph.pass_nodes.clone(),
        scene_color_draw_ranges: scene_color_ranges,
        graph_execution_order,
        capture_scene_graph,
        descriptor_heap,
        descriptor_heap_plan,
        pipelines: pipeline_resources,
        draw_commands,
        sampled_slots: descriptor_layout.sampled_slots,
        sampled_binding_cycle,
        material_uniform_enabled: descriptor_layout.material_uniform_enabled,
        frame_topology,
        dynamic_effect_uniforms,
    })
}

pub(super) fn write_scene_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    transform_buffer: &NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    white_image: Option<&NativeVulkanVulkanaliaImage>,
    scene_textures: &[scene_texture::SceneTextureImageResource],
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
    scene_color: Option<(vk::Image, vk::Format)>,
) -> Result<(), String> {
    for (draw_index, draw) in draw_commands.iter().enumerate() {
        native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer(
            device,
            descriptor_heap,
            draw.resource_descriptor_base,
            transform_buffer
                .device_address
                .saturating_add(draw_index as u64 * SCENE_DRAW_UNIFORM_BYTES),
            SCENE_DRAW_UNIFORM_BYTES,
        )?;
        let mut resource_descriptor_index = draw.resource_descriptor_base + 1;
        if let Some(material_buffer) = material_buffer {
            native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer(
                device,
                descriptor_heap,
                resource_descriptor_index,
                material_buffer
                    .device_address
                    .saturating_add(draw_index as u64 * SCENE_MATERIAL_UNIFORM_BYTES),
                SCENE_MATERIAL_UNIFORM_BYTES,
            )?;
            resource_descriptor_index += 1;
        }
        if let Some(skinning_buffer) = skinning_buffer {
            native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer(
                device,
                descriptor_heap,
                resource_descriptor_index,
                skinning_buffer
                    .device_address
                    .saturating_add(draw.skinning_byte_offset),
                draw.skinning_byte_count,
            )?;
        }
    }
    write_scene_sampled_descriptors(
        device,
        descriptor_heap,
        draw_commands,
        white_image,
        scene_textures,
        effect_targets,
        sampled_binding_plan,
        scene_color,
        material_buffer.is_some(),
        skinning_buffer.is_some(),
    )
}

pub(super) fn write_scene_frame_sampled_descriptors(
    device: &Device,
    scene: &mut SceneGpuResources,
    reference_phase: usize,
    scene_color_image: vk::Image,
    scene_color_format: vk::Format,
) -> Result<(), String> {
    let sampled_binding_plan = scene
        .sampled_binding_cycle
        .get(reference_phase)
        .ok_or_else(|| format!("scene sampled binding phase {reference_phase} is missing"))?;
    write_scene_sampled_descriptors(
        device,
        &mut scene.descriptor_heap,
        &scene.draw_commands,
        scene.white_upload.as_ref().map(|upload| &upload.image),
        &scene.scene_textures,
        &scene.effect_targets,
        sampled_binding_plan,
        Some((scene_color_image, scene_color_format)),
        scene.material_uniform_enabled,
        scene.skinning_buffer.is_some(),
    )
}

pub(super) fn write_scene_sampled_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    white_image: Option<&NativeVulkanVulkanaliaImage>,
    scene_textures: &[scene_texture::SceneTextureImageResource],
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
    scene_color: Option<(vk::Image, vk::Format)>,
    material_uniform_enabled: bool,
    skinning_storage_enabled: bool,
) -> Result<(), String> {
    let fallback_image_view_info = white_image.map(scene_white_image_view_info);
    let fallback_sampler_info = scene_sampled_sampler_info();
    for (draw_index, draw) in draw_commands.iter().enumerate() {
        let resource_descriptor_index = draw.resource_descriptor_base
            + 1
            + usize::from(material_uniform_enabled)
            + usize::from(skinning_storage_enabled);
        for sampled_index in 0..sampled_binding_plan.sampled_slot_count {
            let source = sampled_binding_plan
                .source(draw_index, sampled_index)
                .ok_or_else(|| {
                    format!(
                        "scene draw {draw_index} sampled descriptor {sampled_index} has no binding plan"
                    )
                })?;
            let (image_view_info, sampler_info) = match source {
                SceneSampledImageSource::FallbackWhite => (
                    fallback_image_view_info.ok_or_else(|| {
                        "scene fallback sampled binding has no fallback texture".to_owned()
                    })?,
                    fallback_sampler_info,
                ),
                SceneSampledImageSource::SceneTexture { resource } => {
                    let texture = scene_texture::scene_texture_image(scene_textures, resource)
                        .ok_or_else(|| {
                            format!(
                                "scene sampled texture resource {} has no GPU image",
                                resource.0
                            )
                        })?;
                    (
                        scene_texture::scene_texture_image_view_info(texture),
                        scene_texture::scene_texture_sampler_info(texture),
                    )
                }
                SceneSampledImageSource::SceneColorSnapshot => {
                    let (image, format) = scene_color.ok_or_else(|| {
                        "scene color snapshot descriptor is unavailable before image acquire"
                            .to_owned()
                    })?;
                    (scene_color_image_view_info(image, format), fallback_sampler_info)
                }
                SceneSampledImageSource::EffectTarget {
                    physical_slot,
                    batch_atlas_tile,
                } => {
                    let resource = effect_targets
                        .iter()
                        .find(|resource| resource.plan.physical_slot == physical_slot)
                        .ok_or_else(|| {
                            format!(
                                "scene sampled effect target physical slot {physical_slot} has no image"
                            )
                        })?;
                    (
                        effect_target::effect_target_sampled_image_view_info(
                            resource,
                            batch_atlas_tile,
                        ),
                        fallback_sampler_info,
                    )
                }
            };
            native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler(
                device,
                descriptor_heap,
                resource_descriptor_index + sampled_index,
                draw.sampler_descriptor_base + sampled_index,
                &image_view_info,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                &sampler_info,
            )?;
        }
    }
    Ok(())
}

