//! Scene-owned and engine-owned graphics-program resolution.
//!
//! Package programs are resolved from the validated `.gscene` payload before
//! the static engine catalog is considered. Vertex attributes are derived from
//! the final compiled stage interface; authored locations are never replaced
//! with the catalog's fixed location convention.

#[cfg(test)]
mod uniform_alias_tests;
mod uniform_source;

use uniform_source::scene_owned_uniform_source;

use vulkanalia::vk;

use crate::engine::scene::{
    SceneRenderingDeviceDrawPrimitive, SceneShaderBindingKind, SceneShaderIoDirection,
    SceneShaderProgramRecord, SceneShaderScalarType, SceneShaderStage, SceneShaderStageIoRecord,
    SceneStorage, SceneStringId,
};
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneShader, native_vulkan_scene_shader_for_key,
    native_vulkan_scene_vertex_spirv_for_primitive,
};

#[derive(Debug, Clone, Copy)]
pub(super) enum SceneResolvedGraphicsProgram<'a> {
    SceneOwned {
        key: &'a str,
        vertex: &'a SceneShaderProgramRecord,
        fragment: &'a SceneShaderProgramRecord,
    },
    EngineBuiltIn {
        key: &'a str,
        shader: &'static BuiltinSceneShader,
        vertex_spirv: &'static [u32],
    },
}

impl<'a> SceneResolvedGraphicsProgram<'a> {
    pub(super) fn key(self) -> &'a str {
        match self {
            Self::SceneOwned { key, .. } | Self::EngineBuiltIn { key, .. } => key,
        }
    }

    pub(super) fn is_scene_owned(self) -> bool {
        matches!(self, Self::SceneOwned { .. })
    }

    pub(super) fn vertex_spirv(self, storage: &'a SceneStorage) -> &'a [u32] {
        match self {
            Self::SceneOwned { vertex, .. } => storage.shader_program_spirv(vertex),
            Self::EngineBuiltIn { vertex_spirv, .. } => vertex_spirv,
        }
    }

    pub(super) fn fragment_spirv(self, storage: &'a SceneStorage) -> &'a [u32] {
        match self {
            Self::SceneOwned { fragment, .. } => storage.shader_program_spirv(fragment),
            Self::EngineBuiltIn { shader, .. } => shader.fragment_spirv,
        }
    }

    pub(super) fn scene_owned_vertex(self) -> Option<&'a SceneShaderProgramRecord> {
        match self {
            Self::SceneOwned { vertex, .. } => Some(vertex),
            Self::EngineBuiltIn { .. } => None,
        }
    }
}

pub(super) fn resolve_scene_graphics_program(
    storage: &SceneStorage,
    shader_id: SceneStringId,
    primitive: SceneRenderingDeviceDrawPrimitive,
) -> Result<SceneResolvedGraphicsProgram<'_>, String> {
    let key = storage
        .string(shader_id)
        .ok_or_else(|| "scene graphics program has no shader key".to_owned())?;
    let vertex = storage.shader_program(shader_id, SceneShaderStage::Vertex);
    let fragment = storage.shader_program(shader_id, SceneShaderStage::Fragment);
    match (vertex, fragment) {
        (Some(vertex), Some(fragment)) => {
            return Ok(SceneResolvedGraphicsProgram::SceneOwned {
                key,
                vertex,
                fragment,
            });
        }
        (Some(_), None) => {
            return Err(format!(
                "scene-owned graphics program {key:?} has no fragment stage"
            ));
        }
        (None, Some(_)) => {
            return Err(format!(
                "scene-owned graphics program {key:?} has no vertex stage"
            ));
        }
        (None, None) => {}
    }

    if key.starts_with("workshop/") {
        return Err(format!(
            "package-owned graphics program {key:?} has no embedded native stages"
        ));
    }
    let shader = native_vulkan_scene_shader_for_key(key)
        .ok_or_else(|| format!("engine-owned scene shader {key:?} is not built in"))?;
    let vertex_spirv = native_vulkan_scene_vertex_spirv_for_primitive(shader, primitive).ok_or_else(|| {
        format!("engine-owned scene shader {key:?} has no {primitive:?} vertex program")
    })?;
    Ok(SceneResolvedGraphicsProgram::EngineBuiltIn {
        key,
        shader,
        vertex_spirv,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SceneVertexAttributePlan {
    pub location: u32,
    pub format: vk::Format,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SceneOwnedStageResourcePlan<'a> {
    pub stage: SceneShaderStage,
    pub push_constant_bytes: u32,
    pub bindings: Vec<SceneOwnedDescriptorBindingPlan>,
    pub uniform_buffers: Vec<SceneOwnedUniformBufferPlan<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SceneOwnedDescriptorBindingPlan {
    pub kind: SceneShaderBindingKind,
    pub register: u32,
    pub descriptor_count: u32,
    pub push_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SceneOwnedUniformBufferPlan<'a> {
    pub name: &'a str,
    pub register: u32,
    pub byte_size: u32,
    pub members: Vec<SceneOwnedUniformMemberPlan<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SceneOwnedUniformMemberPlan<'a> {
    pub name: &'a str,
    pub source: SceneOwnedUniformSource<'a>,
    pub byte_offset: u32,
    pub byte_size: u32,
    pub scalar_type: SceneShaderScalarType,
    pub rows: u32,
    pub columns: u32,
    pub array_count: u32,
    pub array_stride: u32,
    pub matrix_stride: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneOwnedUniformSource<'a> {
    SceneTime,
    FrameDelta,
    AudioSpectrum64Left,
    AudioSpectrum64Right,
    ModelViewProjectionMatrix,
    EffectModelViewProjectionMatrix,
    LayerModelMatrix,
    SampledTextureResolution { slot: u32 },
    MaterialParameter { authored_name: &'a str },
}

pub(super) fn scene_owned_stage_resource_plan<'a>(
    storage: &'a SceneStorage,
    program: &SceneShaderProgramRecord,
) -> Result<SceneOwnedStageResourcePlan<'a>, String> {
    let key = storage
        .string(program.program_key)
        .ok_or_else(|| "scene-owned stage resource plan has no program key".to_owned())?;
    let bindings = storage
        .shader_program_bindings(program)
        .iter()
        .map(|binding| SceneOwnedDescriptorBindingPlan {
            kind: binding.kind,
            register: binding.register,
            descriptor_count: binding.descriptor_count,
            push_offset: binding.push_offset,
        })
        .collect::<Vec<_>>();
    let uniform_buffers = storage
        .shader_program_uniform_buffers(program)
        .iter()
        .map(|buffer| {
            let name = storage.string(buffer.name).ok_or_else(|| {
                format!("scene-owned program {key:?} has an unnamed uniform buffer")
            })?;
            let members = storage
                .shader_uniform_buffer_members(buffer)
                .iter()
                .map(|member| {
                    let name = storage.string(member.name).ok_or_else(|| {
                        format!("scene-owned program {key:?} has an unnamed uniform member")
                    })?;
                    Ok(SceneOwnedUniformMemberPlan {
                        name,
                        source: scene_owned_uniform_source(
                            key,
                            name,
                            storage.string(member.material_parameter),
                            member,
                        )?,
                        byte_offset: member.byte_offset,
                        byte_size: member.byte_size,
                        scalar_type: member.scalar_type,
                        rows: member.rows,
                        columns: member.columns,
                        array_count: member.array_count,
                        array_stride: member.array_stride,
                        matrix_stride: member.matrix_stride,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(SceneOwnedUniformBufferPlan {
                name,
                register: buffer.register,
                byte_size: buffer.byte_size,
                members,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(SceneOwnedStageResourcePlan {
        stage: program.stage,
        push_constant_bytes: program.push_constant_bytes,
        bindings,
        uniform_buffers,
    })
}

pub(super) fn scene_owned_vertex_attributes(
    storage: &SceneStorage,
    vertex: &SceneShaderProgramRecord,
) -> Result<Vec<SceneVertexAttributePlan>, String> {
    if vertex.stage != SceneShaderStage::Vertex {
        return Err("scene-owned vertex input plan requires a vertex-stage program".to_owned());
    }
    let key = storage
        .string(vertex.program_key)
        .ok_or_else(|| "scene-owned vertex program has no key".to_owned())?;
    let mut attributes = storage
        .shader_program_stage_io(vertex)
        .iter()
        .filter(|item| item.direction == SceneShaderIoDirection::Input)
        .map(|item| scene_vertex_attribute(storage, key, item))
        .collect::<Result<Vec<_>, _>>()?;
    attributes.sort_unstable_by_key(|attribute| attribute.location);
    Ok(attributes)
}

fn scene_vertex_attribute(
    storage: &SceneStorage,
    key: &str,
    input: &SceneShaderStageIoRecord,
) -> Result<SceneVertexAttributePlan, String> {
    let name = storage
        .string(input.name)
        .ok_or_else(|| format!("scene-owned vertex program {key:?} has an unnamed input"))?;
    if input.location_count != 1 || input.columns != 1 {
        return Err(format!(
            "scene-owned vertex input {name:?} in {key:?} spans an unsupported matrix or location range"
        ));
    }
    let (format, offset) = match name {
        "a_Position"
            if input.scalar_type == SceneShaderScalarType::F32
                && matches!(input.rows, 2..=4) =>
        {
            // Scene meshes are two-dimensional. Vulkan supplies zero for the
            // absent z component of the authored vec3 input.
            (vk::Format::R32G32_SFLOAT, 0)
        }
        "a_TexCoord"
            if input.scalar_type == SceneShaderScalarType::F32 && input.rows == 2 =>
        {
            (vk::Format::R32G32_SFLOAT, 8)
        }
        "a_BlendIndices"
            if input.scalar_type == SceneShaderScalarType::U32 && input.rows == 4 =>
        {
            (vk::Format::R32G32B32A32_UINT, 20)
        }
        "a_BlendWeights"
            if input.scalar_type == SceneShaderScalarType::F32 && input.rows == 4 =>
        {
            (vk::Format::R32G32B32A32_SFLOAT, 36)
        }
        _ => {
            return Err(format!(
                "scene-owned vertex input {name:?} in {key:?} has no proven scene-mesh semantic"
            ));
        }
    };
    Ok(SceneVertexAttributePlan {
        location: input.location,
        format,
        offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneShaderBindingRecord, SceneShaderProgramRecord,
        SceneShaderStageIoRecord, SceneShaderUniformBufferRecord, SceneShaderUniformMemberRecord,
    };

    #[test]
    fn scene_owned_program_wins_over_a_same_basename_catalog_entry() {
        let storage = owned_graphics_storage(
            "workshop/example/effects/rounded_mask__SLOTS_1",
            Vec::new(),
        );

        let program = resolve_scene_graphics_program(
            &storage,
            SceneStringId(0),
            SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .expect("scene-owned program");

        assert!(program.is_scene_owned());
        assert_eq!(program.key(), "workshop/example/effects/rounded_mask__SLOTS_1");
        assert_eq!(program.vertex_spirv(&storage), minimal_spirv());
        assert_eq!(program.fragment_spirv(&storage), minimal_spirv());
    }

    #[test]
    fn package_identity_never_falls_back_to_the_static_catalog() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["workshop/example/effects/rounded_mask__SLOTS_1".to_owned()],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");

        let error = resolve_scene_graphics_program(
            &storage,
            SceneStringId(0),
            SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .expect_err("missing package program must fail");

        assert!(error.contains("package-owned"));
        assert!(error.contains("no embedded native stages"));
    }

    #[test]
    fn incomplete_scene_owned_program_fails_before_catalog_resolution() {
        let spirv = minimal_spirv();
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "workshop/example/effects/rounded_mask__SLOTS_1".to_owned(),
                "main".to_owned(),
            ],
            shader_programs: vec![program(SceneShaderStage::Vertex, 0, spirv.len())],
            shader_spirv: spirv,
            ..SceneBinaryDocument::default()
        })
        .expect("incomplete scene-owned storage");

        let error = resolve_scene_graphics_program(
            &storage,
            SceneStringId(0),
            SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        )
        .expect_err("incomplete scene-owned program must fail");

        assert!(error.contains("scene-owned graphics program"));
        assert!(error.contains("no fragment stage"));
    }

    #[test]
    fn typed_locations_select_mesh_semantics_instead_of_fixed_locations() {
        let storage = owned_graphics_storage(
            "workshop/example/effects/rounded_mask__SLOTS_1",
            vec![
                stage_input(2, 0, SceneShaderScalarType::F32, 2),
                stage_input(3, 1, SceneShaderScalarType::F32, 3),
            ],
        );
        let vertex = storage
            .shader_program(SceneStringId(0), SceneShaderStage::Vertex)
            .expect("vertex stage");

        let attributes = scene_owned_vertex_attributes(&storage, vertex).expect("attributes");

        assert_eq!(
            attributes,
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
    fn unknown_vertex_semantic_fails_explicitly() {
        let storage = owned_graphics_storage(
            "workshop/example/effects/custom",
            vec![stage_input(4, 0, SceneShaderScalarType::F32, 2)],
        );
        let vertex = storage
            .shader_program(SceneStringId(0), SceneShaderStage::Vertex)
            .expect("vertex stage");

        let error = scene_owned_vertex_attributes(&storage, vertex)
            .expect_err("unknown semantic must fail");

        assert!(error.contains("no proven scene-mesh semantic"));
    }

    #[test]
    fn rounded_mask_stage_plans_preserve_typed_uniform_and_push_abi() {
        let storage = rounded_mask_resource_storage();
        let vertex = storage
            .shader_program(SceneStringId(0), SceneShaderStage::Vertex)
            .expect("vertex stage");
        let fragment = storage
            .shader_program(SceneStringId(0), SceneShaderStage::Fragment)
            .expect("fragment stage");

        let vertex_plan =
            scene_owned_stage_resource_plan(&storage, vertex).expect("vertex resource plan");
        let fragment_plan =
            scene_owned_stage_resource_plan(&storage, fragment).expect("fragment resource plan");

        assert_eq!(vertex_plan.push_constant_bytes, 4);
        assert_eq!(vertex_plan.bindings.len(), 1);
        assert_eq!(vertex_plan.bindings[0].kind, SceneShaderBindingKind::UniformBuffer);
        assert_eq!(vertex_plan.bindings[0].push_offset, 0);
        assert_eq!(vertex_plan.uniform_buffers[0].byte_size, 176);
        assert_eq!(
            vertex_plan.uniform_buffers[0]
                .members
                .iter()
                .map(|member| member.source)
                .collect::<Vec<_>>(),
            vec![
                SceneOwnedUniformSource::ModelViewProjectionMatrix,
                SceneOwnedUniformSource::LayerModelMatrix,
            ]
        );
        assert_eq!(
            vertex_plan.uniform_buffers[0]
                .members
                .iter()
                .map(|member| (member.name, member.byte_offset, member.byte_size))
                .collect::<Vec<_>>(),
            vec![
                ("g_ModelViewProjectionMatrix", 0, 64),
                ("g_LayerModelMatrix", 64, 64),
            ]
        );
        assert_eq!(fragment_plan.push_constant_bytes, 12);
        assert_eq!(
            fragment_plan
                .bindings
                .iter()
                .map(|binding| (binding.kind, binding.register, binding.push_offset))
                .collect::<Vec<_>>(),
            vec![
                (SceneShaderBindingKind::SampledImage, 0, 0),
                (SceneShaderBindingKind::Sampler, 0, 4),
                (SceneShaderBindingKind::UniformBuffer, 0, 8),
            ]
        );
        assert_eq!(fragment_plan.uniform_buffers[0].byte_size, 48);
        assert_eq!(
            fragment_plan.uniform_buffers[0]
                .members
                .iter()
                .map(|member| member.source)
                .collect::<Vec<_>>(),
            vec![
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "Color",
                },
                SceneOwnedUniformSource::SampledTextureResolution { slot: 0 },
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "Radius",
                },
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "Border width",
                },
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "Softness",
                },
                SceneOwnedUniformSource::MaterialParameter {
                    authored_name: "ui_editor_properties_opacity",
                },
            ]
        );
        assert_eq!(
            fragment_plan.uniform_buffers[0]
                .members
                .iter()
                .map(|member| (member.name, member.byte_offset))
                .collect::<Vec<_>>(),
            vec![
                ("u_Color", 0),
                ("g_Texture0Resolution", 16),
                ("u_Radius", 32),
                ("u_BorderWidth", 36),
                ("u_Softness", 40),
                ("u_Alpha", 44),
            ]
        );
    }

    #[test]
    fn system_time_uniforms_have_strict_typed_sources() {
        let scalar = uniform_member(0, u32::MAX, 0, 4, 1, 1, 0);

        assert_eq!(
            scene_owned_uniform_source("test", "g_Time", None, &scalar).unwrap(),
            SceneOwnedUniformSource::SceneTime
        );
        assert_eq!(
            scene_owned_uniform_source("test", "g_FrameTime", None, &scalar).unwrap(),
            SceneOwnedUniformSource::FrameDelta
        );

        let vector = uniform_member(0, u32::MAX, 0, 16, 4, 1, 0);
        assert!(
            scene_owned_uniform_source("test", "g_Time", None, &vector)
                .unwrap_err()
                .contains("incompatible runtime shape")
        );

        let mut audio = uniform_member(0, u32::MAX, 0, 1012, 1, 1, 0);
        audio.array_count = 64;
        audio.array_stride = 16;
        assert_eq!(
            scene_owned_uniform_source("test", "g_AudioSpectrum64Left", None, &audio).unwrap(),
            SceneOwnedUniformSource::AudioSpectrum64Left
        );
    }

    fn owned_graphics_storage(
        key: &str,
        vertex_inputs: Vec<SceneShaderStageIoRecord>,
    ) -> SceneStorage {
        let spirv = minimal_spirv();
        let input_count = vertex_inputs.len() as u32;
        SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                key.to_owned(),
                "main".to_owned(),
                "a_TexCoord".to_owned(),
                "a_Position".to_owned(),
                "a_Unknown".to_owned(),
            ],
            shader_programs: vec![
                program(SceneShaderStage::Vertex, input_count, spirv.len()),
                program(SceneShaderStage::Fragment, 0, spirv.len()),
            ],
            shader_stage_io: vertex_inputs,
            shader_spirv: spirv,
            ..SceneBinaryDocument::default()
        })
        .expect("owned graphics storage")
    }

    fn program(
        stage: SceneShaderStage,
        stage_io_count: u32,
        spirv_count: usize,
    ) -> SceneShaderProgramRecord {
        SceneShaderProgramRecord {
            program_key: SceneStringId(0),
            stage,
            entry_point: SceneStringId(1),
            spirv_start: 0,
            spirv_count: spirv_count as u32,
            binding_start: 0,
            binding_count: 0,
            stage_io_start: 0,
            stage_io_count,
            uniform_buffer_start: 0,
            uniform_buffer_count: 0,
            push_constant_bytes: 0,
        }
    }

    fn stage_input(
        name: u32,
        location: u32,
        scalar_type: SceneShaderScalarType,
        rows: u32,
    ) -> SceneShaderStageIoRecord {
        SceneShaderStageIoRecord {
            name: SceneStringId(name),
            direction: SceneShaderIoDirection::Input,
            location,
            scalar_type,
            rows,
            columns: 1,
            location_count: 1,
        }
    }

    fn minimal_spirv() -> Vec<u32> {
        vec![0x0723_0203, 0x0001_0600, 0, 2, 0]
    }

    fn rounded_mask_resource_storage() -> SceneStorage {
        let spirv = native_heap_spirv();
        let strings = [
            "workshop/example/effects/rounded_mask__SLOTS_1",
            "main",
            "GlobalParams",
            "g_ModelViewProjectionMatrix",
            "g_LayerModelMatrix",
            "u_Color",
            "g_Texture0Resolution",
            "u_Radius",
            "u_BorderWidth",
            "u_Softness",
            "u_Alpha",
            "Color",
            "Radius",
            "Border width",
            "Softness",
            "ui_editor_properties_opacity",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        SceneStorage::from_document(SceneBinaryDocument {
            strings,
            shader_programs: vec![
                resource_program(SceneShaderStage::Vertex, 0, 1, 0, spirv.len()),
                resource_program(SceneShaderStage::Fragment, 1, 3, 1, spirv.len()),
            ],
            shader_bindings: vec![
                binding(SceneShaderBindingKind::UniformBuffer, 0, 0),
                binding(SceneShaderBindingKind::SampledImage, 0, 0),
                binding(SceneShaderBindingKind::Sampler, 0, 4),
                binding(SceneShaderBindingKind::UniformBuffer, 0, 8),
            ],
            shader_uniform_buffers: vec![
                SceneShaderUniformBufferRecord {
                    name: SceneStringId(2),
                    register: 0,
                    byte_size: 176,
                    member_start: 0,
                    member_count: 2,
                },
                SceneShaderUniformBufferRecord {
                    name: SceneStringId(2),
                    register: 0,
                    byte_size: 48,
                    member_start: 2,
                    member_count: 6,
                },
            ],
            shader_uniform_members: vec![
                uniform_member(3, u32::MAX, 0, 64, 4, 4, 16),
                uniform_member(4, u32::MAX, 64, 64, 4, 4, 16),
                uniform_member(5, 11, 0, 16, 4, 1, 0),
                uniform_member(6, u32::MAX, 16, 16, 4, 1, 0),
                uniform_member(7, 12, 32, 4, 1, 1, 0),
                uniform_member(8, 13, 36, 4, 1, 1, 0),
                uniform_member(9, 14, 40, 4, 1, 1, 0),
                uniform_member(10, 15, 44, 4, 1, 1, 0),
            ],
            shader_spirv: spirv,
            ..SceneBinaryDocument::default()
        })
        .expect("rounded-mask resource storage")
    }

    fn resource_program(
        stage: SceneShaderStage,
        binding_start: u32,
        binding_count: u32,
        uniform_buffer_start: u32,
        spirv_count: usize,
    ) -> SceneShaderProgramRecord {
        SceneShaderProgramRecord {
            program_key: SceneStringId(0),
            stage,
            entry_point: SceneStringId(1),
            spirv_start: 0,
            spirv_count: spirv_count as u32,
            binding_start,
            binding_count,
            stage_io_start: 0,
            stage_io_count: 0,
            uniform_buffer_start,
            uniform_buffer_count: 1,
            push_constant_bytes: binding_count * 4,
        }
    }

    fn binding(
        kind: SceneShaderBindingKind,
        register: u32,
        push_offset: u32,
    ) -> SceneShaderBindingRecord {
        SceneShaderBindingRecord {
            kind,
            register,
            descriptor_count: 1,
            push_offset,
        }
    }

    fn uniform_member(
        name: u32,
        material_parameter: u32,
        byte_offset: u32,
        byte_size: u32,
        rows: u32,
        columns: u32,
        matrix_stride: u32,
    ) -> SceneShaderUniformMemberRecord {
        SceneShaderUniformMemberRecord {
            name: SceneStringId(name),
            material_parameter: SceneStringId(material_parameter),
            byte_offset,
            byte_size,
            scalar_type: SceneShaderScalarType::F32,
            rows,
            columns,
            array_count: 1,
            array_stride: 0,
            matrix_stride,
        }
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
