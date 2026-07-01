pub(super) const RUNTIME: &str = "native-water-caustics";

pub(super) fn matches(runtime: Option<&str>, normalized_effect_file: &str) -> bool {
    runtime == Some(RUNTIME)
        || normalized_effect_file.contains("watercaustics")
        || normalized_effect_file.contains("water_caustics")
}
