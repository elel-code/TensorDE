pub(super) const RUNTIME: &str = "native-opacity-mask";

pub(super) fn matches(runtime: Option<&str>, normalized_effect_file: &str) -> bool {
    runtime == Some(RUNTIME) || normalized_effect_file.contains("opacity")
}
