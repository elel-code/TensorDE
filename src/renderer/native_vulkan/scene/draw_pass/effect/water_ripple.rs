pub(super) fn matches(normalized_effect_file: &str) -> bool {
    normalized_effect_file.contains("waterripple")
        || normalized_effect_file.contains("water_ripple")
}
