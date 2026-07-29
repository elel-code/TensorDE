//! Vulkan scene graphics-pipeline resource creation and destruction.

use super::*;

struct ScenePipelineProgramSelection<'a> {
    vertex_key: &'a str,
    fragment_key: &'a str,
    vertex_entry_point: &'a str,
    fragment_entry_point: &'a str,
    vertex_spirv: &'a [u32],
    fragment_spirv: &'a [u32],
    fragment_descriptor_heap_mode: BuiltinSceneDescriptorHeapMode,
    fragment_local_read_shader: Option<&'static BuiltinSceneLocalReadShader>,
    vertex_uses_native_descriptor_heap: bool,
    vertex_attributes: Option<Vec<SceneVertexAttributePlan>>,
}

struct ScenePipelineVertexSelection<'a> {
    key: &'a str,
    entry_point: &'a str,
    spirv: &'a [u32],
    uses_native_descriptor_heap: bool,
    attributes: Option<Vec<SceneVertexAttributePlan>>,
}

pub(in crate::renderer::native_vulkan) fn create_scene_pipelines(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_layout: &ScenePipelineDescriptorLayout,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    advanced_blend_enabled: bool,
    advanced_blend_coherent: bool,
    scene_color_msaa_enabled: bool,
    local_read_scopes: &[SceneLocalReadScopePlan],
    local_read_limits: SceneLocalReadDeviceLimits,
) -> Result<ScenePipelineResources, String> {
    let keys = drawn_pass_pipeline_keys(
        storage,
        graph,
        target_format,
        effect_target_plans,
        local_read_scopes,
        scene_color_msaa_enabled,
    )?;
    if keys.is_empty() {
        return Err("scene present requires at least one drawable pass pipeline".to_owned());
    }
    if keys
        .iter()
        .any(|key| key.blend.requires_advanced_operation())
        && (!advanced_blend_enabled || !advanced_blend_coherent)
    {
        return Err(
            "scene composite blend requires coherent VK_EXT_blend_operation_advanced support"
                .to_owned(),
        );
    }
    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        let program = select_scene_pipeline_program(storage, key)?;
        let descriptor_access = match key.shader {
            ScenePipelineShader::Authored(shader_id) => {
                scene_pipeline_shader_descriptor_access(storage, shader_id)?
            }
            ScenePipelineShader::EffectPassthrough(_) => match key.local_read_role {
                Some(ScenePipelineLocalReadRole::Consumer(scope_index)) => {
                    let scope = local_read_scopes.get(scope_index).ok_or_else(|| {
                        format!("scene pipeline references missing local-read scope {scope_index}")
                    })?;
                    ScenePipelineShaderDescriptorAccess {
                        sampled_slots: Vec::new(),
                        input_attachment_slots: vec![scope.input_slot()],
                    }
                }
                _ => scene_passthrough_descriptor_access(),
            },
        };
        let local_read_metadata = match key.local_read_role {
            Some(ScenePipelineLocalReadRole::Producer(scope_index)) => {
                let scope = local_read_scopes.get(scope_index).ok_or_else(|| {
                    format!("scene pipeline references missing local-read scope {scope_index}")
                })?;
                Some(scope.pipeline_metadata(
                    SceneLocalReadScopePassRole::Producer,
                    &descriptor_access,
                    None,
                    local_read_limits,
                )?)
            }
            Some(ScenePipelineLocalReadRole::Consumer(scope_index)) => {
                let scope = local_read_scopes.get(scope_index).ok_or_else(|| {
                    format!("scene pipeline references missing local-read scope {scope_index}")
                })?;
                Some(scope.pipeline_metadata(
                    SceneLocalReadScopePassRole::Consumer,
                    &descriptor_access,
                    program.fragment_local_read_shader,
                    local_read_limits,
                )?)
            }
            None => {
                if !descriptor_access.input_attachment_slots.is_empty() {
                    destroy_scene_pipelines(
                        device,
                        ScenePipelineResources {
                            entries,
                            particle_compute: None,
                        },
                    );
                    return Err(format!(
                        "scene shader {:?} declares input attachments outside a planned local-read scope",
                        program.fragment_key
                    ));
                }
                None
            }
        };
        let pipeline_debug =
            std::env::var_os("GILDER_NATIVE_VULKAN_SCENE_PIPELINE_DEBUG").is_some();
        if pipeline_debug {
            eprintln!(
                "gilder-scene-pipeline-create: begin vertex={:?} fragment={:?} primitive={:?}",
                program.vertex_key,
                program.fragment_key,
                key.primitive
            );
        }
        match create_scene_pipeline(
            device,
            key.target_format,
            extent,
            program.vertex_spirv,
            program.fragment_spirv,
            program.vertex_entry_point,
            program.fragment_entry_point,
            program.fragment_descriptor_heap_mode,
            program.vertex_uses_native_descriptor_heap,
            program.vertex_attributes.as_deref(),
            descriptor_heap_plan,
            descriptor_layout,
            &descriptor_access,
            local_read_metadata.as_ref(),
            key.blend,
            key.cull_mode,
            key.color_write_mask,
            key.advanced_source_premultiplied,
            key.advanced_blend_overlap,
            key.samples,
            if key.primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard {
                vk::PrimitiveTopology::TRIANGLE_STRIP
            } else {
                vk::PrimitiveTopology::TRIANGLE_LIST
            },
            program.vertex_key == "gilder/dynamic-text",
        ) {
            Ok(pipeline) => {
                if pipeline_debug {
                    eprintln!(
                        "gilder-scene-pipeline-create: complete vertex={:?} fragment={:?} primitive={:?}",
                        program.vertex_key,
                        program.fragment_key,
                        key.primitive
                    );
                }
                entries.push(ScenePipelineEntry { key, pipeline });
            }
            Err(err) => {
                destroy_scene_pipelines(
                    device,
                    ScenePipelineResources {
                        entries,
                        particle_compute: None,
                    },
                );
                return Err(err);
            }
        }
    }
    let particle_compute = particle_compute::create_optional_particle_compute_pipeline(
        device,
        graph,
        descriptor_heap_plan,
    )?;
    Ok(ScenePipelineResources {
        entries,
        particle_compute,
    })
}

fn select_scene_pipeline_program(
    storage: &SceneStorage,
    key: ScenePipelineKey,
) -> Result<ScenePipelineProgramSelection<'_>, String> {
    let authored_id = match key.shader {
        ScenePipelineShader::Authored(shader_id)
        | ScenePipelineShader::EffectPassthrough(shader_id) => shader_id,
    };
    let authored = resolve_scene_graphics_program(storage, authored_id, key.primitive)?;
    let vertex = select_scene_pipeline_vertex(storage, authored)?;
    match key.shader {
        ScenePipelineShader::Authored(_) => match authored {
            SceneResolvedGraphicsProgram::SceneOwned { fragment, .. } => {
                require_scene_owned_stage_resources_connected(storage, fragment)?;
                Ok(ScenePipelineProgramSelection {
                    vertex_key: vertex.key,
                    fragment_key: authored.key(),
                    vertex_entry_point: vertex.entry_point,
                    fragment_entry_point: storage.string(fragment.entry_point).ok_or_else(|| {
                        format!(
                            "scene-owned fragment program {:?} has no entry point",
                            vertex.key
                        )
                    })?,
                    vertex_spirv: vertex.spirv,
                    fragment_spirv: authored.fragment_spirv(storage),
                    fragment_descriptor_heap_mode: BuiltinSceneDescriptorHeapMode::Native,
                    fragment_local_read_shader: None,
                    vertex_uses_native_descriptor_heap: vertex.uses_native_descriptor_heap,
                    vertex_attributes: vertex.attributes,
                })
            }
            SceneResolvedGraphicsProgram::EngineBuiltIn { shader, .. } => {
                Ok(ScenePipelineProgramSelection {
                    vertex_key: vertex.key,
                    fragment_key: authored.key(),
                    vertex_entry_point: vertex.entry_point,
                    fragment_entry_point: "main",
                    vertex_spirv: vertex.spirv,
                    fragment_spirv: authored.fragment_spirv(storage),
                    fragment_descriptor_heap_mode: shader.fragment_descriptor_heap_mode,
                    fragment_local_read_shader: shader.local_read_shader.as_ref(),
                    vertex_uses_native_descriptor_heap: vertex.uses_native_descriptor_heap,
                    vertex_attributes: vertex.attributes,
                })
            }
        },
        ScenePipelineShader::EffectPassthrough(_) => {
            let passthrough = native_vulkan_scene_shader_for_key("we/passthrough")
                .ok_or_else(|| "engine-owned scene shader \"we/passthrough\" is not built in".to_owned())?;
            Ok(ScenePipelineProgramSelection {
                vertex_key: vertex.key,
                fragment_key: passthrough.key,
                vertex_entry_point: vertex.entry_point,
                fragment_entry_point: "main",
                vertex_spirv: vertex.spirv,
                fragment_spirv: passthrough.fragment_spirv,
                fragment_descriptor_heap_mode: passthrough.fragment_descriptor_heap_mode,
                fragment_local_read_shader: passthrough.local_read_shader.as_ref(),
                vertex_uses_native_descriptor_heap: vertex.uses_native_descriptor_heap,
                vertex_attributes: vertex.attributes,
            })
        }
    }
}

fn select_scene_pipeline_vertex<'a>(
    storage: &'a SceneStorage,
    program: SceneResolvedGraphicsProgram<'a>,
) -> Result<ScenePipelineVertexSelection<'a>, String> {
    match program {
        SceneResolvedGraphicsProgram::SceneOwned { vertex, .. } => {
            let attributes = scene_owned_vertex_attributes(storage, vertex)?;
            require_scene_owned_stage_resources_connected(storage, vertex)?;
            let key = program.key();
            let entry_point = storage.string(vertex.entry_point).ok_or_else(|| {
                format!("scene-owned vertex program {key:?} has no entry point")
            })?;
            Ok(ScenePipelineVertexSelection {
                key,
                entry_point,
                spirv: program.vertex_spirv(storage),
                uses_native_descriptor_heap: true,
                attributes: Some(attributes),
            })
        }
        SceneResolvedGraphicsProgram::EngineBuiltIn { .. } => {
            Ok(ScenePipelineVertexSelection {
                key: program.key(),
                entry_point: "main",
                spirv: program.vertex_spirv(storage),
                uses_native_descriptor_heap: false,
                attributes: None,
            })
        }
    }
}

fn require_scene_owned_stage_resources_connected(
    storage: &SceneStorage,
    stage: &crate::engine::scene::SceneShaderProgramRecord,
) -> Result<(), String> {
    let plan = scene_owned_stage_resource_plan(storage, stage)?;
    if plan.push_constant_bytes == 0
        && plan.bindings.is_empty()
        && plan.uniform_buffers.is_empty()
    {
        return Ok(());
    }
    let key = storage
        .string(stage.program_key)
        .ok_or_else(|| "scene-owned resource stage has no program key".to_owned())?;
    Err(format!(
        "scene-owned {:?} stage for {key:?} requires retained typed descriptor resources; the runtime refuses the legacy fixed uniform buffers",
        stage.stage
    ))
}

pub(in crate::renderer::native_vulkan) fn destroy_scene_pipelines(
    device: &Device,
    resources: ScenePipelineResources,
) {
    particle_compute::destroy_optional_particle_compute_pipeline(
        device,
        resources.particle_compute,
    );
    unsafe {
        for entry in resources.entries {
            device.destroy_pipeline(entry.pipeline, None);
        }
    }
}

fn create_scene_pipeline(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    vertex_spirv: &[u32],
    fragment_spirv: &[u32],
    vertex_entry_point: &str,
    fragment_entry_point: &str,
    fragment_descriptor_heap_mode: BuiltinSceneDescriptorHeapMode,
    vertex_uses_native_descriptor_heap: bool,
    vertex_attributes: Option<&[SceneVertexAttributePlan]>,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_layout: &ScenePipelineDescriptorLayout,
    descriptor_access: &ScenePipelineShaderDescriptorAccess,
    local_read_metadata: Option<&SceneLocalReadPipelineMetadata<'_>>,
    blend: SceneGpuBlend,
    cull_mode: SceneCullMode,
    color_write_mask: SceneColorWriteMask,
    advanced_source_premultiplied: bool,
    advanced_blend_overlap: vk::BlendOverlapEXT,
    samples: ScenePipelineSamples,
    topology: vk::PrimitiveTopology,
    dynamic_text: bool,
) -> Result<vk::Pipeline, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene pipeline requires non-zero extent".to_owned());
    }
    let vertex_entry = std::ffi::CString::new(vertex_entry_point)
        .map_err(|_| "scene vertex entry point contains an embedded NUL".to_owned())?;
    let vertex_module = create_shader_module(device, vertex_spirv, "scene vertex")?;
    let result = (|| -> Result<vk::Pipeline, String> {
        let local_read_fragment_spirv = local_read_metadata
            .and_then(SceneLocalReadPipelineMetadata::local_read_fragment_spirv);
        let (fragment_spirv, fragment_entry_point, fragment_descriptor_heap_mode) =
            if let Some(local_read_fragment_spirv) = local_read_fragment_spirv {
                (
                    local_read_fragment_spirv,
                    "main",
                    BuiltinSceneDescriptorHeapMode::Mapped,
                )
            } else {
                (
                    fragment_spirv,
                    fragment_entry_point,
                    fragment_descriptor_heap_mode,
                )
            };
        let fragment_entry = std::ffi::CString::new(fragment_entry_point)
            .map_err(|_| "scene fragment entry point contains an embedded NUL".to_owned())?;
        let fragment_module = create_shader_module(device, fragment_spirv, "scene fragment")?;
        let result = create_scene_pipeline_with_modules(
            device,
            target_format,
            vertex_module,
            fragment_module,
            vertex_entry.as_bytes_with_nul(),
            fragment_entry.as_bytes_with_nul(),
            fragment_descriptor_heap_mode,
            vertex_uses_native_descriptor_heap,
            vertex_attributes,
            descriptor_heap_plan,
            descriptor_layout,
            descriptor_access,
            local_read_metadata,
            blend,
            cull_mode,
            color_write_mask,
            advanced_source_premultiplied,
            advanced_blend_overlap,
            samples,
            topology,
            dynamic_text,
        );
        unsafe {
            device.destroy_shader_module(fragment_module, None);
        }
        result
    })();
    unsafe {
        device.destroy_shader_module(vertex_module, None);
    }
    result
}

fn create_scene_pipeline_with_modules(
    device: &Device,
    target_format: vk::Format,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    vertex_entry_point: &[u8],
    fragment_entry_point: &[u8],
    fragment_descriptor_heap_mode: BuiltinSceneDescriptorHeapMode,
    vertex_uses_native_descriptor_heap: bool,
    vertex_attributes: Option<&[SceneVertexAttributePlan]>,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_layout: &ScenePipelineDescriptorLayout,
    descriptor_access: &ScenePipelineShaderDescriptorAccess,
    local_read_metadata: Option<&SceneLocalReadPipelineMetadata<'_>>,
    blend: SceneGpuBlend,
    cull_mode: SceneCullMode,
    color_write_mask: SceneColorWriteMask,
    advanced_source_premultiplied: bool,
    advanced_blend_overlap: vk::BlendOverlapEXT,
    samples: ScenePipelineSamples,
    topology: vk::PrimitiveTopology,
    dynamic_text: bool,
) -> Result<vk::Pipeline, String> {
    let mut vertex_mappings = if vertex_uses_native_descriptor_heap {
        Vec::new()
    } else {
        vec![
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                descriptor_heap_plan,
                2,
                0,
                0,
            )?,
        ]
    };
    if !vertex_uses_native_descriptor_heap && descriptor_layout.material_uniform_enabled {
        vertex_mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                descriptor_heap_plan,
                3,
                0,
                1,
            )?,
        );
    }
    let skinning_descriptor_index = 1 + usize::from(descriptor_layout.material_uniform_enabled);
    if !vertex_uses_native_descriptor_heap && descriptor_layout.skinning_storage_enabled {
        vertex_mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping(
                descriptor_heap_plan,
                4,
                0,
                skinning_descriptor_index,
                false,
            )?,
        );
    }
    let mut vertex_mapping_info =
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(&vertex_mappings)?;
    let mut vertex_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::VERTEX)
        .module(vertex_module)
        .name(vertex_entry_point)
        .build();
    if !vertex_mappings.is_empty() {
        vertex_stage.next = &mut vertex_mapping_info as *mut _ as *const std::ffi::c_void;
    }

    let fragment_mappings = scene_fragment_descriptor_mappings(
        fragment_descriptor_heap_mode,
        descriptor_heap_plan,
        descriptor_layout,
        descriptor_access,
        local_read_metadata,
    )?;
    let mut fragment_mapping_info =
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(&fragment_mappings)?;
    let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(fragment_module)
        .name(fragment_entry_point)
        .build();
    if !fragment_mappings.is_empty() {
        fragment_stage.next = &mut fragment_mapping_info as *mut _ as *const std::ffi::c_void;
    }
    create_graphics_pipeline(
        device,
        target_format,
        [vertex_stage, fragment_stage],
        blend,
        cull_mode,
        color_write_mask,
        advanced_source_premultiplied,
        advanced_blend_overlap,
        samples,
        topology,
        dynamic_text,
        vertex_attributes,
        local_read_metadata,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneShaderBindingKind, SceneShaderBindingRecord,
        SceneShaderIoDirection, SceneShaderProgramRecord, SceneShaderScalarType, SceneShaderStage,
        SceneShaderStageIoRecord,
    };

    #[test]
    fn selects_scene_owned_spirv_entry_points_and_typed_vertex_locations() {
        let storage = scene_owned_storage(false);

        let selection = select_scene_pipeline_program(&storage, authored_key())
            .expect("scene-owned pipeline program");

        assert_eq!(selection.vertex_key, "workshop/example/effects/custom");
        assert_eq!(selection.fragment_key, selection.vertex_key);
        assert_eq!(selection.vertex_entry_point, "vertexMain");
        assert_eq!(selection.fragment_entry_point, "fragmentMain");
        assert_eq!(
            selection.fragment_descriptor_heap_mode,
            BuiltinSceneDescriptorHeapMode::Native
        );
        assert!(selection.vertex_uses_native_descriptor_heap);
        assert_eq!(
            selection.vertex_attributes.expect("typed attributes"),
            vec![
                SceneVertexAttributePlan {
                    location: 0,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 8,
                },
                SceneVertexAttributePlan {
                    location: 1,
                    format: vk::Format::R32G32_SFLOAT,
                    offset: 0,
                },
            ]
        );
    }

    #[test]
    fn rejects_owned_descriptor_abi_before_legacy_fixed_uniform_reuse() {
        let storage = scene_owned_storage(true);

        let error = select_scene_pipeline_program(&storage, authored_key())
            .err()
            .expect("unconnected retained resources must fail");

        assert!(error.contains("scene-owned Vertex stage"));
        assert!(error.contains("retained typed descriptor resources"));
        assert!(error.contains("refuses the legacy fixed uniform buffers"));
    }

    fn authored_key() -> ScenePipelineKey {
        ScenePipelineKey {
            shader: ScenePipelineShader::Authored(SceneStringId(0)),
            primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            blend: SceneGpuBlend::Replace,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgba,
            advanced_source_premultiplied: false,
            advanced_blend_overlap: vk::BlendOverlapEXT::UNCORRELATED,
            target_format: vk::Format::R8G8B8A8_UNORM,
            samples: ScenePipelineSamples::Single,
            local_read_role: None,
        }
    }

    fn scene_owned_storage(with_binding: bool) -> SceneStorage {
        let vertex_spirv = if with_binding {
            native_heap_spirv()
        } else {
            minimal_spirv()
        };
        let fragment_spirv = minimal_spirv();
        let fragment_spirv_start = vertex_spirv.len() as u32;
        let mut spirv = vertex_spirv.clone();
        spirv.extend_from_slice(&fragment_spirv);
        let binding_count = u32::from(with_binding);
        SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "workshop/example/effects/custom".to_owned(),
                "vertexMain".to_owned(),
                "fragmentMain".to_owned(),
                "a_TexCoord".to_owned(),
                "a_Position".to_owned(),
            ],
            shader_programs: vec![
                SceneShaderProgramRecord {
                    program_key: SceneStringId(0),
                    stage: SceneShaderStage::Vertex,
                    entry_point: SceneStringId(1),
                    spirv_start: 0,
                    spirv_count: vertex_spirv.len() as u32,
                    binding_start: 0,
                    binding_count,
                    stage_io_start: 0,
                    stage_io_count: 2,
                    uniform_buffer_start: 0,
                    uniform_buffer_count: 0,
                    push_constant_bytes: binding_count * 4,
                },
                SceneShaderProgramRecord {
                    program_key: SceneStringId(0),
                    stage: SceneShaderStage::Fragment,
                    entry_point: SceneStringId(2),
                    spirv_start: fragment_spirv_start,
                    spirv_count: fragment_spirv.len() as u32,
                    binding_start: binding_count,
                    binding_count: 0,
                    stage_io_start: 2,
                    stage_io_count: 0,
                    uniform_buffer_start: 0,
                    uniform_buffer_count: 0,
                    push_constant_bytes: 0,
                },
            ],
            shader_bindings: with_binding
                .then_some(SceneShaderBindingRecord {
                    kind: SceneShaderBindingKind::SampledImage,
                    register: 0,
                    descriptor_count: 1,
                    push_offset: 0,
                })
                .into_iter()
                .collect(),
            shader_stage_io: vec![
                vertex_input(3, 0, 2),
                vertex_input(4, 1, 3),
            ],
            shader_spirv: spirv,
            ..SceneBinaryDocument::default()
        })
        .expect("scene-owned storage")
    }

    fn vertex_input(name: u32, location: u32, rows: u32) -> SceneShaderStageIoRecord {
        SceneShaderStageIoRecord {
            name: SceneStringId(name),
            direction: SceneShaderIoDirection::Input,
            location,
            scalar_type: SceneShaderScalarType::F32,
            rows,
            columns: 1,
            location_count: 1,
        }
    }

    fn minimal_spirv() -> Vec<u32> {
        vec![0x0723_0203, 0x0001_0600, 0, 2, 0]
    }

    fn native_heap_spirv() -> Vec<u32> {
        let mut words = minimal_spirv();
        words.extend(spirv_instruction(17, &[5_128]));
        words.extend(spirv_string_instruction(10, "SPV_EXT_descriptor_heap"));
        words
    }

    fn spirv_instruction(opcode: u32, operands: &[u32]) -> Vec<u32> {
        let word_count = u32::try_from(operands.len() + 1).expect("instruction word count");
        std::iter::once((word_count << 16) | opcode)
            .chain(operands.iter().copied())
            .collect()
    }

    fn spirv_string_instruction(opcode: u32, value: &str) -> Vec<u32> {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        bytes.resize(bytes.len().next_multiple_of(4), 0);
        let operands = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().expect("SPIR-V word")))
            .collect::<Vec<_>>();
        spirv_instruction(opcode, &operands)
    }
}
