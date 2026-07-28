//! Canonical v21 scene shader keys emitted by the Wallpaper Engine converter.

pub(super) fn canonical_scene_shader_key(authored: &str) -> String {
    if authored.is_empty() {
        return String::new();
    }
    let (program, variant) = authored
        .split_once("__")
        .map_or((authored, ""), |(program, variant)| (program, variant));
    let canonical_program = canonical_scene_shader_program(program);
    if variant.is_empty() {
        canonical_program
    } else {
        format!("{canonical_program}__{variant}")
    }
}

fn canonical_scene_shader_program(authored: &str) -> String {
    if authored.starts_with("we/") || authored.starts_with("effects/") {
        return authored.to_owned();
    }
    let mut components = authored.rsplit('/');
    let basename = components.next().unwrap_or(authored);
    let parent = components.next().unwrap_or_default();
    if parent.eq_ignore_ascii_case("effects") {
        return format!("effects/{}", basename.to_ascii_lowercase());
    }
    match authored {
        "genericimage2" | "genericimage4" | "genericparticle" | "clippingmaskimage4" | "color"
        | "text" | "composelayer" | "flat" | "minimalalpha" | "passthrough"
        | "utilitycomposite" => format!("we/{authored}"),
        _ => authored.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_programs_are_emitted_only_in_the_we_namespace() {
        assert_eq!(
            canonical_scene_shader_key("genericimage4__PUPPETSKINNING_1"),
            "we/genericimage4__PUPPETSKINNING_1"
        );
        assert_eq!(
            canonical_scene_shader_key("composelayer"),
            "we/composelayer"
        );
    }

    #[test]
    fn effect_program_identity_does_not_include_the_authored_package_path() {
        assert_eq!(
            canonical_scene_shader_key(
                "workshop/current/effects/Simple_Audio_Bars__SLOTS_1__SHAPE_7"
            ),
            "effects/simple_audio_bars__SLOTS_1__SHAPE_7"
        );
    }

    #[test]
    fn unrelated_program_paths_are_not_guessed() {
        assert_eq!(
            canonical_scene_shader_key("workshop/current/custom/genericimage4"),
            "workshop/current/custom/genericimage4"
        );
    }
}
