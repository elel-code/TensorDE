pub(super) fn shader_combo_enabled(shader_key: &str, name: &str) -> bool {
    shader_combo_value(shader_key, name, 0) != 0
}

pub(super) fn shader_combo_value(shader_key: &str, name: &str, default: i64) -> i64 {
    let prefix = format!("{}_", name.to_ascii_uppercase());
    shader_key
        .split("__")
        .find_map(|part| {
            part.to_ascii_uppercase()
                .strip_prefix(&prefix)
                .and_then(|value| value.parse::<i64>().ok())
        })
        .unwrap_or(default)
}

pub(super) fn shader_texture_slot_enabled(shader_key: &str, slot: u32) -> bool {
    shader_key
        .split("__")
        .find_map(|part| {
            part.strip_prefix("SLOTS_")
                .or_else(|| part.strip_prefix("slots_"))
                .and_then(|mask| u32::from_str_radix(mask, 16).ok())
        })
        .is_some_and(|mask| slot < 32 && mask & (1 << slot) != 0)
}
