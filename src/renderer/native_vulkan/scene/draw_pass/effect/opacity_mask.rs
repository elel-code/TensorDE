pub(super) const RUNTIME: &str = "native-opacity-mask";

pub(super) fn matches(runtime: Option<&str>, normalized_effect_file: &str) -> bool {
    runtime == Some(RUNTIME) || normalized_effect_file.contains("opacity")
}

pub(super) fn uses_first_class_target(runtime: Option<&str>, effect_file: &str) -> bool {
    if runtime == Some(RUNTIME) {
        return true;
    }
    let normalized = effect_file.replace('\\', "/").to_ascii_lowercase();
    normalized == "effects/opacity/effect.json"
        || normalized.ends_with("/effects/opacity/effect.json")
}

pub(super) fn alpha_texture_slot<T>(texture_slots: &[T]) -> Option<u32>
where
    T: OpacityTextureSlot,
{
    texture_slots
        .iter()
        .filter_map(|slot| {
            let slot = slot.slot();
            (slot > 0).then_some(slot)
        })
        .min()
}

pub(super) trait OpacityTextureSlot {
    fn slot(&self) -> u32;
}

impl OpacityTextureSlot for super::NativeVulkanSceneTextureSlot {
    fn slot(&self) -> u32 {
        self.slot
    }
}
