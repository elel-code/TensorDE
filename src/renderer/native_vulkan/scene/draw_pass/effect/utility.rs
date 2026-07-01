use super::NativeVulkanSceneEffectKind;

pub(super) fn classify(normalized_effect_file: &str) -> Option<NativeVulkanSceneEffectKind> {
    if normalized_effect_file.contains("blur") {
        Some(NativeVulkanSceneEffectKind::Blur)
    } else if normalized_effect_file.contains("composelayer")
        || normalized_effect_file.contains("fullscreenlayer")
    {
        Some(NativeVulkanSceneEffectKind::CompositeLayer)
    } else if normalized_effect_file.contains("newproperty")
        || normalized_effect_file.contains("user")
    {
        Some(NativeVulkanSceneEffectKind::UserBindings)
    } else {
        None
    }
}
