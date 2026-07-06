use crate::core::SceneBlendMode;

use super::{
    VulkanaliaSceneSampledImageDrawCommand, VulkanaliaSceneSampledImageShaderProgram,
    VulkanaliaSceneSampledImageVertexProgram, scene_sampled_image_shader_program,
};

const SCENE_BLEND_MODE_COUNT: usize = 9;
const SCENE_PIPELINE_PROGRAM_COUNT: usize = 18;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaScenePipelineBlendUsage {
    enabled: [bool; SCENE_BLEND_MODE_COUNT],
}

impl VulkanaliaScenePipelineBlendUsage {
    pub(in crate::renderer::native_vulkan::vulkan) fn all() -> Self {
        Self {
            enabled: [true; SCENE_BLEND_MODE_COUNT],
        }
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn from_modes(
        modes: impl IntoIterator<Item = SceneBlendMode>,
    ) -> Self {
        let mut usage = Self {
            enabled: [false; SCENE_BLEND_MODE_COUNT],
        };
        for mode in modes {
            usage.enabled[scene_blend_mode_index(mode)] = true;
        }
        usage
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn contains(self, mode: SceneBlendMode) -> bool {
        self.enabled[scene_blend_mode_index(mode)]
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn enabled_count(self) -> u32 {
        self.enabled.iter().filter(|enabled| **enabled).count() as u32
    }
}

fn scene_blend_mode_index(mode: SceneBlendMode) -> usize {
    match mode {
        SceneBlendMode::Alpha => 0,
        SceneBlendMode::Normal => 1,
        SceneBlendMode::Additive => 2,
        SceneBlendMode::Multiply => 3,
        SceneBlendMode::Screen => 4,
        SceneBlendMode::Max => 5,
        SceneBlendMode::Modulate => 6,
        SceneBlendMode::HslColor => 7,
        SceneBlendMode::AlphaToCoverage => 8,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaScenePipelineProgramUsage {
    enabled: [[bool; SCENE_BLEND_MODE_COUNT]; SCENE_PIPELINE_PROGRAM_COUNT],
}

impl VulkanaliaScenePipelineProgramUsage {
    pub(in crate::renderer::native_vulkan::vulkan) fn all() -> Self {
        Self {
            enabled: [[true; SCENE_BLEND_MODE_COUNT]; SCENE_PIPELINE_PROGRAM_COUNT],
        }
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn from_draw_commands(
        commands: &[VulkanaliaSceneSampledImageDrawCommand],
    ) -> Self {
        let mut usage = Self {
            enabled: [[false; SCENE_BLEND_MODE_COUNT]; SCENE_PIPELINE_PROGRAM_COUNT],
        };
        for command in commands {
            usage.enabled[scene_pipeline_program_index(
                scene_sampled_image_shader_program(&command.material),
                command.vertex_program,
            )][scene_blend_mode_index(command.material.render_state.blend.mode)] = true;
        }
        usage
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn contains(
        self,
        shader_program: VulkanaliaSceneSampledImageShaderProgram,
        vertex_program: VulkanaliaSceneSampledImageVertexProgram,
    ) -> bool {
        self.enabled[scene_pipeline_program_index(shader_program, vertex_program)]
            .iter()
            .any(|enabled| *enabled)
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn contains_blend(
        self,
        shader_program: VulkanaliaSceneSampledImageShaderProgram,
        vertex_program: VulkanaliaSceneSampledImageVertexProgram,
        blend_mode: SceneBlendMode,
    ) -> bool {
        self.enabled[scene_pipeline_program_index(shader_program, vertex_program)]
            [scene_blend_mode_index(blend_mode)]
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn enabled_count(self) -> u32 {
        self.enabled
            .iter()
            .filter(|blends| blends.iter().any(|enabled| *enabled))
            .count() as u32
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn graphics_pipeline_count_for_blend_usage(
        self,
        blend_usage: VulkanaliaScenePipelineBlendUsage,
    ) -> u32 {
        self.enabled
            .iter()
            .map(|blends| {
                blends
                    .iter()
                    .enumerate()
                    .filter(|(index, enabled)| **enabled && blend_usage.enabled[*index])
                    .count() as u32
            })
            .sum()
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn pass_specific_graphics_pipeline_count_for_blend_usage(
        self,
        blend_usage: VulkanaliaScenePipelineBlendUsage,
    ) -> u32 {
        self.enabled
            .iter()
            .enumerate()
            .filter(|(index, _blends)| *index != SCENE_PIPELINE_PROGRAM_SAMPLED_GENERIC)
            .map(|(_index, blends)| {
                blends
                    .iter()
                    .enumerate()
                    .filter(|(index, enabled)| **enabled && blend_usage.enabled[*index])
                    .count() as u32
            })
            .sum()
    }
}

const SCENE_PIPELINE_PROGRAM_SAMPLED_GENERIC: usize = 0;

fn scene_pipeline_program_index(
    shader_program: VulkanaliaSceneSampledImageShaderProgram,
    vertex_program: VulkanaliaSceneSampledImageVertexProgram,
) -> usize {
    match vertex_program {
        VulkanaliaSceneSampledImageVertexProgram::PuppetGpu => match shader_program {
            VulkanaliaSceneSampledImageShaderProgram::WaterRipple => 3,
            VulkanaliaSceneSampledImageShaderProgram::WaterWaves => 4,
            _ => 1,
        },
        VulkanaliaSceneSampledImageVertexProgram::ParticleGpu => 2,
        VulkanaliaSceneSampledImageVertexProgram::Sampled => match shader_program {
            VulkanaliaSceneSampledImageShaderProgram::Generic => {
                SCENE_PIPELINE_PROGRAM_SAMPLED_GENERIC
            }
            VulkanaliaSceneSampledImageShaderProgram::WaterRipple => 5,
            VulkanaliaSceneSampledImageShaderProgram::WaterWaves => 6,
            VulkanaliaSceneSampledImageShaderProgram::WaterWaves2 => 6,
            VulkanaliaSceneSampledImageShaderProgram::WaterFlow => 7,
            VulkanaliaSceneSampledImageShaderProgram::WaterCaustics => 8,
            VulkanaliaSceneSampledImageShaderProgram::FoliageSway => 9,
            VulkanaliaSceneSampledImageShaderProgram::AutoSway => 10,
            VulkanaliaSceneSampledImageShaderProgram::Scroll => 11,
            VulkanaliaSceneSampledImageShaderProgram::Skew => 12,
            VulkanaliaSceneSampledImageShaderProgram::Iris => 13,
            VulkanaliaSceneSampledImageShaderProgram::Opacity => 14,
            VulkanaliaSceneSampledImageShaderProgram::TechCircle => 15,
            VulkanaliaSceneSampledImageShaderProgram::AudioBars => 16,
            VulkanaliaSceneSampledImageShaderProgram::PassthroughBlend => 17,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::SceneRenderAlphaTextureMode;
    use crate::renderer::native_vulkan::vulkan::scene::present::{
        NativeVulkanVulkanaliaSceneSampledImageMaterial,
        NativeVulkanVulkanaliaSceneTextureSlotResourceBinding,
    };

    fn command(
        effect: Option<super::super::super::present::NativeVulkanVulkanaliaSceneEffectKind>,
        vertex_program: VulkanaliaSceneSampledImageVertexProgram,
    ) -> VulkanaliaSceneSampledImageDrawCommand {
        let mut material = NativeVulkanVulkanaliaSceneSampledImageMaterial::sampled_image(
            SceneBlendMode::Alpha,
            None,
            SceneRenderAlphaTextureMode::Multiply,
            1,
        );
        if let Some(effect) = effect {
            material.effect_kinds = vec![effect];
        }
        VulkanaliaSceneSampledImageDrawCommand {
            engine_pass_id: None,
            layer_index: 0,
            last_layer_index: 0,
            material,
            descriptor_binding:
                super::super::VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                    descriptor_group_base_index: 0,
                    texture_slot_bindings: vec![
                        NativeVulkanVulkanaliaSceneTextureSlotResourceBinding {
                            slot: 0,
                            resource_index: 0,
                        },
                    ],
                },
            render_target: super::super::VulkanaliaSceneSampledImageRenderTarget::Swapchain,
            draw_instance_index: 0,
            vertex_program,
            vertex_offset: 0,
            first_index: 0,
            index_count: 6,
        }
    }

    #[test]
    fn program_usage_collapses_puppet_and_particle_to_backing_pipeline_sets() {
        let mut fused_waterwaves = command(
            Some(super::super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterWaves),
            VulkanaliaSceneSampledImageVertexProgram::Sampled,
        );
        fused_waterwaves.material.fused_effect_kind = Some(
            super::super::super::present::NativeVulkanVulkanaliaSceneFusedEffectKind::WaterWaves2,
        );
        let usage = VulkanaliaScenePipelineProgramUsage::from_draw_commands(&[
            command(None, VulkanaliaSceneSampledImageVertexProgram::ParticleGpu),
            command(
                Some(
                    super::super::super::present::NativeVulkanVulkanaliaSceneEffectKind::WaterWaves,
                ),
                VulkanaliaSceneSampledImageVertexProgram::PuppetGpu,
            ),
            fused_waterwaves,
        ]);

        assert!(usage.contains(
            VulkanaliaSceneSampledImageShaderProgram::Generic,
            VulkanaliaSceneSampledImageVertexProgram::ParticleGpu,
        ));
        assert!(usage.contains(
            VulkanaliaSceneSampledImageShaderProgram::WaterWaves,
            VulkanaliaSceneSampledImageVertexProgram::PuppetGpu,
        ));
        assert!(usage.contains(
            VulkanaliaSceneSampledImageShaderProgram::WaterWaves,
            VulkanaliaSceneSampledImageVertexProgram::Sampled,
        ));
        assert!(usage.contains(
            VulkanaliaSceneSampledImageShaderProgram::WaterWaves2,
            VulkanaliaSceneSampledImageVertexProgram::Sampled,
        ));
        assert_eq!(usage.enabled_count(), 3);
    }
}
