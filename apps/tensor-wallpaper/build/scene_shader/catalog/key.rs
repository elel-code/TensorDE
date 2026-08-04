pub(super) fn effect_shader_name_for_key(key: &str) -> &str {
    key.split("__").next().unwrap_or(key)
}

pub(super) fn effect_texture_slot_mask_for_key(key: &str) -> u32 {
    for part in key.split("__") {
        if let Some(hex) = part.strip_prefix("SLOTS_") {
            return u32::from_str_radix(hex, 16).unwrap_or_else(|err| {
                panic!("invalid built-in scene shader SLOTS mask in {key}: {err}")
            });
        }
    }
    0
}
