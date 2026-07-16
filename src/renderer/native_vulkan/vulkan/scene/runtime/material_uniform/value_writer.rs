pub(super) fn set_vector(values: &mut [f32], start: usize, parameter: &[f32], count: usize) {
    for (lane, value) in parameter.iter().take(count).enumerate() {
        if let Some(destination) = values.get_mut(start + lane) {
            *destination = *value;
        }
    }
}
