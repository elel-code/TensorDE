//! Vulkan scene graphics-pipeline resource creation and destruction.

use super::*;

struct ScenePipelineProgramSelection<'a> {
    vertex_key: &'a str,
    fragment_key: &'a str,
    vertex_entry_point: &'a str,
    fragment_entry_point: &'a str,
    vertex_spirv: &'a [u32],
    fragment_spirv: &'a [u32],
    fragment_local_read_shader: Option<&'static BuiltinSceneLocalReadShader>,
    vertex_attributes: Option<Vec<SceneVertexAttributePlan>>,
    default_mesh_vertex_input: bool,
}

struct ScenePipelineVertexSelection<'a> {
    key: &'a str,
    entry_point: &'a str,
    spirv: &'a [u32],
    attributes: Option<Vec<SceneVertexAttributePlan>>,
}

pub(in crate::renderer::rendering_device) struct ScenePipelineResourceCreateInputs<'a> {
    pub device: &'a vulkan_renderer::Backend,
    pub target_format: vulkan_renderer::TextureFormat,
    pub extent: vulkan_renderer::Extent2D,
    pub storage: &'a SceneStorage,
    pub graph: &'a SceneRenderingDeviceGraphPlan,
    pub resource_descriptor_kinds: &'a [vulkan_renderer::DescriptorSlotKind],
    pub particle_global_descriptor_base: Option<usize>,
    pub effect_target_plans: &'a [SceneEffectTargetImagePlan],
    pub advanced_blend_enabled: bool,
    pub advanced_blend_coherent: bool,
    pub scene_color_msaa_enabled: bool,
    pub local_read_scopes: &'a [SceneLocalReadScopePlan],
    pub pipeline_binary_cache: &'a vulkan_renderer::PipelineBinaryArchiveCache,
}

pub(super) struct SceneGraphicsPipelineCreateInputs<'a> {
    pub device: &'a vulkan_renderer::Backend,
    pub target_format: vulkan_renderer::TextureFormat,
    pub extent: vulkan_renderer::Extent2D,
    pub vertex_spirv: &'a [u32],
    pub fragment_spirv: &'a [u32],
    pub vertex_entry_point: &'a str,
    pub fragment_entry_point: &'a str,
    pub vertex_attributes: Option<&'a [SceneVertexAttributePlan]>,
    pub local_read_metadata: Option<&'a SceneLocalReadPipelineMetadata<'a>>,
    pub blend: SceneGpuBlend,
    pub cull_mode: SceneCullMode,
    pub color_write_mask: SceneColorWriteMask,
    pub advanced_source_premultiplied: bool,
    pub advanced_blend_overlap: vulkan_renderer::BlendOverlap,
    pub samples: ScenePipelineSamples,
    pub topology: vulkan_renderer::PrimitiveTopology,
    pub default_mesh_vertex_input: bool,
    pub dynamic_text: bool,
    pub pipeline_binary_cache: &'a vulkan_renderer::PipelineBinaryArchiveCache,
}

pub(in crate::renderer::rendering_device) fn create_scene_pipelines(
    inputs: ScenePipelineResourceCreateInputs<'_>,
) -> Result<ScenePipelineResources, String> {
    let ScenePipelineResourceCreateInputs {
        device,
        target_format,
        extent,
        storage,
        graph,
        resource_descriptor_kinds,
        particle_global_descriptor_base,
        effect_target_plans,
        advanced_blend_enabled,
        advanced_blend_coherent,
        scene_color_msaa_enabled,
        local_read_scopes,
        pipeline_binary_cache,
    } = inputs;
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
    let mut machine_code_binary_count = 0usize;
    let mut machine_code_bytes = 0usize;
    let mut machine_code_cache_hits = 0usize;
    let mut machine_code_cache_misses = 0usize;
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
                )?)
            }
            None => {
                if !descriptor_access.input_attachment_slots.is_empty() {
                    destroy_scene_pipelines(ScenePipelineResources {
                        entries,
                        video: None,
                        particle_compute: None,
                        machine_code_binary_count,
                        machine_code_bytes,
                        machine_code_cache_hits,
                        machine_code_cache_misses,
                    });
                    return Err(format!(
                        "scene shader {:?} declares input attachments outside a planned local-read scope",
                        program.fragment_key
                    ));
                }
                None
            }
        };
        let pipeline_debug =
            std::env::var_os("TENSOR_WALLPAPER_RENDERING_DEVICE_SCENE_PIPELINE_DEBUG").is_some();
        if pipeline_debug {
            eprintln!(
                "tensor-wallpaper-scene-pipeline-create: begin vertex={:?} fragment={:?} primitive={:?}",
                program.vertex_key, program.fragment_key, key.primitive
            );
        }
        match create_scene_pipeline(SceneGraphicsPipelineCreateInputs {
            device,
            target_format: key
                .target_format
                .ok_or_else(|| "drawable scene pipeline has no target format".to_owned())?,
            extent,
            vertex_spirv: program.vertex_spirv,
            fragment_spirv: program.fragment_spirv,
            vertex_entry_point: program.vertex_entry_point,
            fragment_entry_point: program.fragment_entry_point,
            vertex_attributes: program.vertex_attributes.as_deref(),
            local_read_metadata: local_read_metadata.as_ref(),
            blend: key.blend,
            cull_mode: key.cull_mode,
            color_write_mask: key.color_write_mask,
            advanced_source_premultiplied: key.advanced_source_premultiplied,
            advanced_blend_overlap: key.advanced_blend_overlap,
            samples: key.samples,
            topology: if key.primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard {
                vulkan_renderer::PrimitiveTopology::TriangleStrip
            } else {
                vulkan_renderer::PrimitiveTopology::TriangleList
            },
            default_mesh_vertex_input: program.default_mesh_vertex_input,
            dynamic_text: program.vertex_key == "tensor-wallpaper/dynamic-text",
            pipeline_binary_cache,
        }) {
            Ok(prepared) => {
                if pipeline_debug {
                    eprintln!(
                        "tensor-wallpaper-scene-pipeline-create: complete vertex={:?} fragment={:?} primitive={:?}",
                        program.vertex_key, program.fragment_key, key.primitive
                    );
                }
                machine_code_binary_count += prepared.archive().binaries.len();
                machine_code_bytes += prepared
                    .archive()
                    .binaries
                    .iter()
                    .map(|binary| binary.data.len())
                    .sum::<usize>();
                if prepared.archive_reused() {
                    machine_code_cache_hits += 1;
                } else {
                    machine_code_cache_misses += 1;
                }
                entries.push(ScenePipelineEntry {
                    key,
                    pipeline: prepared,
                });
            }
            Err(err) => {
                destroy_scene_pipelines(ScenePipelineResources {
                    entries,
                    video: None,
                    particle_compute: None,
                    machine_code_binary_count,
                    machine_code_bytes,
                    machine_code_cache_hits,
                    machine_code_cache_misses,
                });
                return Err(err);
            }
        }
    }
    let video = video::create_optional(
        device,
        graph,
        target_format,
        scene_color_msaa_enabled,
        pipeline_binary_cache,
    )?;
    if let Some(video) = video.as_ref() {
        let (binary_count, binary_bytes, archive_reused) = video.machine_code_metrics();
        machine_code_binary_count += binary_count;
        machine_code_bytes += binary_bytes;
        if archive_reused {
            machine_code_cache_hits += 1;
        } else {
            machine_code_cache_misses += 1;
        }
    }
    let particle_compute = particle_compute::create_optional_particle_compute_pipeline(
        device,
        graph,
        resource_descriptor_kinds,
        particle_global_descriptor_base,
        pipeline_binary_cache,
    )?;
    if let Some(compute) = particle_compute.as_ref() {
        let (binary_count, binary_bytes, archive_reused) = compute.machine_code_metrics();
        machine_code_binary_count += binary_count;
        machine_code_bytes += binary_bytes;
        if archive_reused {
            machine_code_cache_hits += 1;
        } else {
            machine_code_cache_misses += 1;
        }
    }
    Ok(ScenePipelineResources {
        entries,
        video,
        particle_compute,
        machine_code_binary_count,
        machine_code_bytes,
        machine_code_cache_hits,
        machine_code_cache_misses,
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
                Ok(ScenePipelineProgramSelection {
                    vertex_key: vertex.key,
                    fragment_key: authored.key(),
                    vertex_entry_point: vertex.entry_point,
                    fragment_entry_point: storage.string(fragment.entry_point).ok_or_else(
                        || {
                            format!(
                                "scene-owned fragment program {:?} has no entry point",
                                vertex.key
                            )
                        },
                    )?,
                    vertex_spirv: vertex.spirv,
                    fragment_spirv: authored.fragment_spirv(storage),
                    fragment_local_read_shader: None,
                    vertex_attributes: vertex.attributes,
                    default_mesh_vertex_input: false,
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
                    fragment_local_read_shader: shader.local_read_shader.as_ref(),
                    vertex_attributes: vertex.attributes,
                    default_mesh_vertex_input: key.primitive
                        == SceneRenderingDeviceDrawPrimitive::ObjectMesh,
                })
            }
        },
        ScenePipelineShader::EffectPassthrough(_) => {
            let passthrough =
                rendering_device_scene_shader_for_key("we/passthrough").ok_or_else(|| {
                    "engine-owned scene shader \"we/passthrough\" is not built in".to_owned()
                })?;
            let passthrough_vertex = rendering_device_scene_vertex_shader_for_primitive(
                passthrough,
                key.primitive,
            )
            .ok_or_else(|| {
                format!(
                    "engine-owned scene shader \"we/passthrough\" has no {:?} vertex program",
                    key.primitive
                )
            })?;
            Ok(ScenePipelineProgramSelection {
                vertex_key: passthrough.key,
                fragment_key: passthrough.key,
                vertex_entry_point: "main",
                fragment_entry_point: "main",
                vertex_spirv: passthrough_vertex.spirv,
                fragment_spirv: passthrough.fragment_spirv,
                fragment_local_read_shader: passthrough.local_read_shader.as_ref(),
                vertex_attributes: None,
                default_mesh_vertex_input: matches!(
                    key.primitive,
                    SceneRenderingDeviceDrawPrimitive::ObjectMesh
                        | SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
                ),
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
            let key = program.key();
            let entry_point = storage
                .string(vertex.entry_point)
                .ok_or_else(|| format!("scene-owned vertex program {key:?} has no entry point"))?;
            Ok(ScenePipelineVertexSelection {
                key,
                entry_point,
                spirv: program.vertex_spirv(storage),
                attributes: Some(attributes),
            })
        }
        SceneResolvedGraphicsProgram::EngineBuiltIn { .. } => Ok(ScenePipelineVertexSelection {
            key: program.key(),
            entry_point: "main",
            spirv: program.vertex_spirv(storage),
            attributes: None,
        }),
    }
}

pub(in crate::renderer::rendering_device) fn destroy_scene_pipelines(
    resources: ScenePipelineResources,
) {
    particle_compute::destroy_optional_particle_compute_pipeline(resources.particle_compute);
    drop(resources.entries);
}

pub(super) fn create_scene_pipeline(
    inputs: SceneGraphicsPipelineCreateInputs<'_>,
) -> Result<vulkan_renderer::MachineCodeGraphicsPipeline, String> {
    let SceneGraphicsPipelineCreateInputs {
        device,
        target_format,
        extent,
        vertex_spirv,
        fragment_spirv,
        vertex_entry_point,
        fragment_entry_point,
        vertex_attributes,
        local_read_metadata,
        blend,
        cull_mode,
        color_write_mask,
        advanced_source_premultiplied,
        advanced_blend_overlap,
        samples,
        topology,
        default_mesh_vertex_input,
        dynamic_text,
        pipeline_binary_cache,
    } = inputs;
    if extent.width == 0 || extent.height == 0 {
        return Err("scene pipeline requires non-zero extent".to_owned());
    }
    let vertex_entry = std::ffi::CString::new(vertex_entry_point)
        .map_err(|_| "scene vertex entry point contains an embedded NUL".to_owned())?;
    let vertex_module = device
        .create_shader_module(vulkan_renderer::ShaderModuleDescriptor {
            label: Some("tensor-wallpaper-scene-vertex".into()),
            spirv: vertex_spirv.to_vec(),
        })
        .map_err(|error| format!("create shared scene vertex shader module: {error}"))?;
    let local_read_fragment_spirv =
        local_read_metadata.and_then(SceneLocalReadPipelineMetadata::local_read_fragment_spirv);
    let (fragment_spirv, fragment_entry_point) =
        if let Some(local_read_fragment_spirv) = local_read_fragment_spirv {
            (local_read_fragment_spirv, "main")
        } else {
            (fragment_spirv, fragment_entry_point)
        };
    let fragment_entry = std::ffi::CString::new(fragment_entry_point)
        .map_err(|_| "scene fragment entry point contains an embedded NUL".to_owned())?;
    let fragment_module = device
        .create_shader_module(vulkan_renderer::ShaderModuleDescriptor {
            label: Some("tensor-wallpaper-scene-fragment".into()),
            spirv: fragment_spirv.to_vec(),
        })
        .map_err(|error| format!("create shared scene fragment shader module: {error}"))?;
    create_graphics_pipeline(
        device,
        target_format,
        &vertex_module,
        &fragment_module,
        vertex_entry.as_c_str(),
        fragment_entry.as_c_str(),
        blend,
        cull_mode,
        color_write_mask,
        advanced_source_premultiplied,
        advanced_blend_overlap,
        samples,
        topology,
        default_mesh_vertex_input,
        dynamic_text,
        vertex_attributes,
        local_read_metadata,
        pipeline_binary_cache,
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
            selection.vertex_attributes.expect("typed attributes"),
            vec![
                SceneVertexAttributePlan {
                    location: 0,
                    format: vulkan_renderer::VertexFormat::Float32x2,
                    offset: 8,
                },
                SceneVertexAttributePlan {
                    location: 1,
                    format: vulkan_renderer::VertexFormat::Float32x2,
                    offset: 0,
                },
            ]
        );
    }

    #[test]
    fn accepts_owned_descriptor_abi_after_retained_arena_connection() {
        let storage = scene_owned_storage(true);

        let selection = select_scene_pipeline_program(&storage, authored_key())
            .expect("connected retained resources");

        assert_eq!(selection.vertex_entry_point, "vertexMain");
        assert_eq!(selection.fragment_entry_point, "fragmentMain");
    }

    #[test]
    fn effect_passthrough_uses_the_complete_engine_owned_material_program() {
        let storage = scene_owned_storage(true);
        let mut key = authored_key();
        key.shader = ScenePipelineShader::EffectPassthrough(SceneStringId(0));

        let selection =
            select_scene_pipeline_program(&storage, key).expect("engine passthrough program");

        assert_eq!(selection.vertex_key, "we/passthrough");
        assert_eq!(selection.fragment_key, "we/passthrough");
        assert_eq!(selection.vertex_entry_point, "main");
        assert_eq!(selection.fragment_entry_point, "main");
        assert!(selection.vertex_attributes.is_none());
        assert!(selection.default_mesh_vertex_input);
    }

    #[test]
    fn object_uv_support_effect_passthrough_uses_retained_quad_vertex_input() {
        let storage = scene_owned_storage(true);
        let mut key = authored_key();
        key.shader = ScenePipelineShader::EffectPassthrough(SceneStringId(0));
        key.primitive = SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad;

        let selection = select_scene_pipeline_program(&storage, key)
            .expect("engine passthrough retained-quad program");

        assert_eq!(selection.vertex_key, "we/passthrough");
        assert_eq!(selection.fragment_key, "we/passthrough");
        assert!(selection.vertex_attributes.is_none());
        assert!(selection.default_mesh_vertex_input);
    }

    fn authored_key() -> ScenePipelineKey {
        ScenePipelineKey {
            shader: ScenePipelineShader::Authored(SceneStringId(0)),
            primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            blend: SceneGpuBlend::Replace,
            cull_mode: SceneCullMode::None,
            color_write_mask: SceneColorWriteMask::Rgba,
            advanced_source_premultiplied: false,
            advanced_blend_overlap: vulkan_renderer::BlendOverlap::Uncorrelated,
            target_format: Some(vulkan_renderer::TextureFormat::Rgba8Unorm),
            samples: ScenePipelineSamples::Single,
            local_read_role: None,
        }
    }

    fn scene_owned_storage(with_binding: bool) -> SceneStorage {
        let vertex_spirv = if with_binding {
            descriptor_heap_spirv()
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
            shader_stage_io: vec![vertex_input(3, 0, 2), vertex_input(4, 1, 3)],
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

    fn descriptor_heap_spirv() -> Vec<u32> {
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
