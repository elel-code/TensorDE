// Shared finite scalar/vector predicates for scene ABI records.

fn valid_oscillation_range(min: f32, max: f32, lower_bound: f32) -> bool {
    min.is_finite() && max.is_finite() && min >= lower_bound && max >= min
}

fn valid_vec3(value: SceneVec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}
