//! Audio-spectrum demand detection for retained scene draw pipelines.

use super::first_material_pass;
use crate::engine::scene::{
    SceneMaterialHandle, SceneRenderingDeviceMeshDraw, SceneStorage, SceneStringId,
};
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneParameterLayout, native_vulkan_scene_shader_for_key,
};

pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) fn scene_uses_audio_spectrum(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
) -> bool {
    storage.script_programs().iter().any(|program| {
        program.target == crate::engine::scene::SceneScriptTarget::TechCircleSectorWidth
    }) || draws
        .iter()
        .any(|draw| draw_uses_audio_spectrum(storage, draw))
}

fn draw_uses_audio_spectrum(storage: &SceneStorage, draw: &SceneRenderingDeviceMeshDraw) -> bool {
    scene_owned_shader_uses_audio_spectrum(storage, draw.shader_key)
        || storage
        .string(draw.shader_key)
        .is_some_and(shader_uses_audio_spectrum)
        || material_uses_audio_spectrum(storage, draw.material)
}

fn scene_owned_shader_uses_audio_spectrum(
    storage: &SceneStorage,
    shader_key: SceneStringId,
) -> bool {
    storage
        .shader_programs()
        .iter()
        .filter(|program| program.program_key == shader_key)
        .flat_map(|program| storage.shader_program_uniform_buffers(program))
        .flat_map(|buffer| storage.shader_uniform_buffer_members(buffer))
        .filter(|member| !member.material_parameter.is_some())
        .filter_map(|member| storage.string(member.name))
        .any(|name| matches!(name, "g_AudioSpectrum64Left" | "g_AudioSpectrum64Right"))
}

pub(super) fn material_uses_audio_spectrum(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> bool {
    let Some(pass) = first_material_pass(storage, material) else {
        return false;
    };
    let Some(shader_key) = storage.string(pass.shader_key) else {
        return false;
    };
    shader_uses_audio_spectrum(shader_key)
}

fn shader_uses_audio_spectrum(shader_key: &str) -> bool {
    native_vulkan_scene_shader_for_key(shader_key).is_some_and(|shader| {
        shader.parameter_layout == BuiltinSceneParameterLayout::AudioBars
            || shader.parameter_layout == BuiltinSceneParameterLayout::AudioLine
            || shader.parameter_layout == BuiltinSceneParameterLayout::Oscilloscope
            || (shader.parameter_layout == BuiltinSceneParameterLayout::FinalEffectProgram
                && shader_key.eq_ignore_ascii_case("we/audio-bars-final"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneShaderBindingKind, SceneShaderBindingRecord,
        SceneShaderProgramRecord, SceneShaderScalarType, SceneShaderStage,
        SceneShaderUniformBufferRecord, SceneShaderUniformMemberRecord,
    };

    #[test]
    fn scene_owned_audio_uniform_requests_spectrum_without_a_catalog_layout() {
        let storage = SceneStorage::from_document(scene_owned_audio_document())
            .expect("scene-owned audio storage");

        assert!(scene_owned_shader_uses_audio_spectrum(
            &storage,
            SceneStringId(0)
        ));
        assert!(native_vulkan_scene_shader_for_key(
            "package/effects/audioline__SLOTS_1"
        )
        .is_none());
    }

    fn scene_owned_audio_document() -> SceneBinaryDocument {
        let mut spirv = vec![0x0723_0203, 0x0001_0600, 0, 2, 0];
        spirv.extend(spirv_instruction(17, &[5_128]));
        spirv.extend(spirv_string_instruction(10, "SPV_EXT_descriptor_heap"));
        SceneBinaryDocument {
            strings: vec![
                "package/effects/audioline__SLOTS_1".to_owned(),
                "main".to_owned(),
                "GlobalParams".to_owned(),
                "g_AudioSpectrum64Left".to_owned(),
            ],
            shader_programs: vec![SceneShaderProgramRecord {
                program_key: SceneStringId(0),
                stage: SceneShaderStage::Fragment,
                entry_point: SceneStringId(1),
                spirv_start: 0,
                spirv_count: spirv.len() as u32,
                binding_start: 0,
                binding_count: 1,
                stage_io_start: 0,
                stage_io_count: 0,
                uniform_buffer_start: 0,
                uniform_buffer_count: 1,
                push_constant_bytes: 4,
            }],
            shader_bindings: vec![SceneShaderBindingRecord {
                kind: SceneShaderBindingKind::UniformBuffer,
                register: 0,
                descriptor_count: 1,
                push_offset: 0,
            }],
            shader_uniform_buffers: vec![SceneShaderUniformBufferRecord {
                name: SceneStringId(2),
                register: 0,
                byte_size: 1_012,
                member_start: 0,
                member_count: 1,
            }],
            shader_uniform_members: vec![SceneShaderUniformMemberRecord {
                name: SceneStringId(3),
                material_parameter: SceneStringId::NONE,
                byte_offset: 0,
                byte_size: 1_012,
                scalar_type: SceneShaderScalarType::F32,
                rows: 1,
                columns: 1,
                array_count: 64,
                array_stride: 16,
                matrix_stride: 0,
            }],
            shader_spirv: spirv,
            ..SceneBinaryDocument::default()
        }
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
