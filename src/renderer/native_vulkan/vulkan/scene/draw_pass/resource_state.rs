use super::order::scene_sampled_image_draw_command_effect_target_reads;
use super::{
    VulkanaliaSceneOrderedDrawPipeline, VulkanaliaSceneOrderedDrawStep,
    VulkanaliaSceneSampledImageDrawCommand, VulkanaliaSceneSampledImageRenderTarget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneEffectTargetLayoutState {
    ShaderReadOnly,
    ColorAttachment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SceneEffectTargetWriteTransition {
    Discard,
    ShaderReadToColorAttachment,
    ColorAttachmentDependency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SceneEffectTargetResourceStates {
    layouts: Vec<SceneEffectTargetLayoutState>,
}

impl SceneEffectTargetResourceStates {
    pub(super) fn new(effect_target_count: usize) -> Self {
        Self {
            layouts: vec![SceneEffectTargetLayoutState::ShaderReadOnly; effect_target_count],
        }
    }

    pub(super) fn begin_write(
        &mut self,
        target_index: u32,
        clear: bool,
    ) -> SceneEffectTargetWriteTransition {
        let previous = self
            .layout(target_index)
            .unwrap_or(SceneEffectTargetLayoutState::ShaderReadOnly);
        self.set_layout(target_index, SceneEffectTargetLayoutState::ColorAttachment);
        match (clear, previous) {
            (true, SceneEffectTargetLayoutState::ShaderReadOnly) => {
                SceneEffectTargetWriteTransition::Discard
            }
            (true, SceneEffectTargetLayoutState::ColorAttachment) => {
                SceneEffectTargetWriteTransition::ColorAttachmentDependency
            }
            (false, SceneEffectTargetLayoutState::ShaderReadOnly) => {
                SceneEffectTargetWriteTransition::ShaderReadToColorAttachment
            }
            (false, SceneEffectTargetLayoutState::ColorAttachment) => {
                SceneEffectTargetWriteTransition::ColorAttachmentDependency
            }
        }
    }

    pub(super) fn prepare_shader_read(&mut self, target_index: u32) -> bool {
        if self.layout(target_index) != Some(SceneEffectTargetLayoutState::ColorAttachment) {
            return false;
        }
        self.set_layout(target_index, SceneEffectTargetLayoutState::ShaderReadOnly);
        true
    }

    pub(super) fn finish_shader_read_targets(&mut self) -> Vec<u32> {
        let mut targets = Vec::new();
        for target_index in 0..self.layouts.len() {
            if self.layouts[target_index] != SceneEffectTargetLayoutState::ColorAttachment {
                continue;
            }
            self.layouts[target_index] = SceneEffectTargetLayoutState::ShaderReadOnly;
            targets.push(target_index.min(u32::MAX as usize) as u32);
        }
        targets
    }

    fn layout(&self, target_index: u32) -> Option<SceneEffectTargetLayoutState> {
        self.layouts.get(target_index as usize).copied()
    }

    fn set_layout(&mut self, target_index: u32, layout: SceneEffectTargetLayoutState) {
        if let Some(current) = self.layouts.get_mut(target_index as usize) {
            *current = layout;
        }
    }
}

pub(super) fn scene_ordered_draw_effect_target_reads_for_target_run(
    ordered_draws: &[VulkanaliaSceneOrderedDrawStep],
    run_start_index: usize,
    sampled_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    effect_target_resource_base_index: usize,
    effect_target_resource_count: usize,
) -> Vec<u32> {
    let Some(first_draw) = ordered_draws.get(run_start_index) else {
        return Vec::new();
    };
    let run_target = ordered_draw_target(first_draw, sampled_commands);
    let mut reads = Vec::new();
    for draw in ordered_draws.iter().skip(run_start_index) {
        if ordered_draw_target(draw, sampled_commands) != run_target {
            break;
        }
        if draw.pipeline != VulkanaliaSceneOrderedDrawPipeline::SampledImage {
            continue;
        }
        for target_index in scene_sampled_image_draw_command_effect_target_reads(
            &sampled_commands[draw.command_index],
            effect_target_resource_base_index,
            effect_target_resource_count,
        ) {
            if !reads.contains(&target_index) {
                reads.push(target_index);
            }
        }
    }
    reads
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneOrderedDrawTarget {
    Swapchain,
    EffectTarget(u32),
}

fn ordered_draw_target(
    draw: &VulkanaliaSceneOrderedDrawStep,
    sampled_commands: &[VulkanaliaSceneSampledImageDrawCommand],
) -> SceneOrderedDrawTarget {
    match draw.pipeline {
        VulkanaliaSceneOrderedDrawPipeline::SolidQuad => SceneOrderedDrawTarget::Swapchain,
        VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
            match sampled_commands[draw.command_index].render_target {
                VulkanaliaSceneSampledImageRenderTarget::Swapchain => {
                    SceneOrderedDrawTarget::Swapchain
                }
                VulkanaliaSceneSampledImageRenderTarget::EffectTarget { target_index, .. } => {
                    SceneOrderedDrawTarget::EffectTarget(target_index)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_target_state_defers_shader_read_until_requested() {
        let mut states = SceneEffectTargetResourceStates::new(2);

        assert_eq!(
            states.begin_write(0, true),
            SceneEffectTargetWriteTransition::Discard
        );
        assert!(states.prepare_shader_read(0));
        assert!(!states.prepare_shader_read(0));
        assert_eq!(
            states.begin_write(0, false),
            SceneEffectTargetWriteTransition::ShaderReadToColorAttachment
        );
        assert_eq!(states.finish_shader_read_targets(), vec![0]);
        assert_eq!(states.finish_shader_read_targets(), Vec::<u32>::new());
    }

    #[test]
    fn effect_target_state_preserves_write_after_write_dependency() {
        let mut states = SceneEffectTargetResourceStates::new(1);

        assert_eq!(
            states.begin_write(0, true),
            SceneEffectTargetWriteTransition::Discard
        );
        assert_eq!(
            states.begin_write(0, true),
            SceneEffectTargetWriteTransition::ColorAttachmentDependency
        );
        assert_eq!(
            states.begin_write(0, false),
            SceneEffectTargetWriteTransition::ColorAttachmentDependency
        );
    }
}
