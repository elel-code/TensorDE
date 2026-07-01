use super::NativeVulkanSceneEffectKind;

pub(super) fn classify(normalized_effect_file: &str) -> Option<NativeVulkanSceneEffectKind> {
    if normalized_effect_file.contains("cloudmotion")
        || normalized_effect_file.contains("cloud_motion")
    {
        Some(NativeVulkanSceneEffectKind::CloudMotion)
    } else if normalized_effect_file.contains("lightshafts")
        || normalized_effect_file.contains("light_shafts")
    {
        Some(NativeVulkanSceneEffectKind::LightShafts)
    } else if normalized_effect_file.contains("colorkey")
        || normalized_effect_file.contains("color_key")
    {
        Some(NativeVulkanSceneEffectKind::ColorKey)
    } else if normalized_effect_file.contains("scroll") {
        Some(NativeVulkanSceneEffectKind::Scroll)
    } else if normalized_effect_file.contains("skew") {
        Some(NativeVulkanSceneEffectKind::Skew)
    } else if normalized_effect_file.contains("clipping_mask")
        || normalized_effect_file.contains("clippingmask")
    {
        Some(NativeVulkanSceneEffectKind::ClippingMask)
    } else if normalized_effect_file.contains("rounded_mask")
        || normalized_effect_file.contains("roundedmask")
    {
        Some(NativeVulkanSceneEffectKind::RoundedMask)
    } else if normalized_effect_file.contains("tech_circle")
        || normalized_effect_file.contains("techcircle")
    {
        Some(NativeVulkanSceneEffectKind::TechCircle)
    } else if normalized_effect_file.contains("enhanced_simple_audio_bars")
        || normalized_effect_file.contains("audio_bars")
        || normalized_effect_file.contains("audiobars")
    {
        Some(NativeVulkanSceneEffectKind::AudioBars)
    } else {
        None
    }
}
