use super::super::NativeVulkanSceneEffectEvaluationBoundary;

pub(super) fn matches(normalized_effect_file: &str) -> bool {
    normalized_effect_file.contains("drift")
}

pub(super) fn evaluation_boundary() -> NativeVulkanSceneEffectEvaluationBoundary {
    NativeVulkanSceneEffectEvaluationBoundary::FinalFrameVertex
}
