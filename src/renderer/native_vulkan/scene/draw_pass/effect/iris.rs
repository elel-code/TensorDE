pub(super) const RUNTIME: &str = "native-iris-mask";

pub(super) fn matches(runtime: Option<&str>, normalized_effect_file: &str) -> bool {
    runtime == Some(RUNTIME) || normalized_effect_file.contains("iris")
}

pub(super) fn uses_first_class_target(runtime: Option<&str>, effect_file: &str) -> bool {
    if runtime == Some(RUNTIME) {
        return true;
    }
    let normalized = normalize_effect_file(effect_file);
    normalized == "effects/iris/effect.json" || normalized.ends_with("/effects/iris/effect.json")
}

fn normalize_effect_file(effect_file: &str) -> String {
    effect_file.replace('\\', "/").to_ascii_lowercase()
}
