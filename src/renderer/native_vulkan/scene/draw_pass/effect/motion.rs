use super::NativeVulkanSceneEffectKind;

pub(super) fn classify(normalized_effect_file: &str) -> Option<NativeVulkanSceneEffectKind> {
    if normalized_effect_file.contains("sway") || normalized_effect_file.contains("shake") {
        Some(NativeVulkanSceneEffectKind::SwayShake)
    } else if normalized_effect_file.contains("flutter") {
        Some(NativeVulkanSceneEffectKind::Flutter)
    } else if normalized_effect_file.contains("drift") {
        Some(NativeVulkanSceneEffectKind::Drift)
    } else {
        None
    }
}
