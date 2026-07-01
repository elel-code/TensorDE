use super::NativeVulkanSceneEffectKind;

pub(super) const CAUSTICS_RUNTIME: &str = "native-water-caustics";

pub(super) fn classify(
    runtime: Option<&str>,
    normalized_effect_file: &str,
) -> Option<NativeVulkanSceneEffectKind> {
    if runtime == Some(CAUSTICS_RUNTIME) {
        return Some(NativeVulkanSceneEffectKind::WaterCaustics);
    }
    if normalized_effect_file.contains("waterripple")
        || normalized_effect_file.contains("water_ripple")
    {
        Some(NativeVulkanSceneEffectKind::WaterRipple)
    } else if normalized_effect_file.contains("waterwaves")
        || normalized_effect_file.contains("water_waves")
    {
        Some(NativeVulkanSceneEffectKind::WaterWaves)
    } else if normalized_effect_file.contains("waterflow")
        || normalized_effect_file.contains("water_flow")
    {
        Some(NativeVulkanSceneEffectKind::WaterFlow)
    } else if normalized_effect_file.contains("watercaustics")
        || normalized_effect_file.contains("water_caustics")
    {
        Some(NativeVulkanSceneEffectKind::WaterCaustics)
    } else {
        None
    }
}
