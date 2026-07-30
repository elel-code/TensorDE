//! Allocation-free writes into reflected scene-owned uniform layouts.

pub(super) fn write_matrix(destination: &mut [u8], matrix: &[[f32; 4]; 4]) -> Result<(), String> {
    if destination.len() != 64 {
        return Err(format!(
            "scene-owned matrix destination has {} bytes, expected 64",
            destination.len()
        ));
    }
    for (bytes, value) in destination.chunks_exact_mut(4).zip(matrix.iter().flatten()) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

pub(super) fn write_values(destination: &mut [u8], values: &[f32]) -> Result<(), String> {
    if destination.len() != values.len().saturating_mul(size_of::<f32>()) {
        return Err(format!(
            "scene-owned uniform destination has {} bytes for {} values",
            destination.len(),
            values.len()
        ));
    }
    for (bytes, value) in destination.chunks_exact_mut(4).zip(values) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

pub(super) fn write_strided_values(
    destination: &mut [u8],
    values: &[f32],
    array_stride: u32,
) -> Result<(), String> {
    let stride = array_stride as usize;
    let expected = values
        .len()
        .checked_sub(1)
        .and_then(|count| count.checked_mul(stride))
        .and_then(|offset| offset.checked_add(size_of::<f32>()))
        .ok_or_else(|| "scene-owned uniform array byte count overflows".to_owned())?;
    if stride < size_of::<f32>() || destination.len() != expected {
        return Err(format!(
            "scene-owned uniform array requires {expected} bytes at stride {stride}, received {}",
            destination.len()
        ));
    }
    for (index, value) in values.iter().enumerate() {
        let offset = index * stride;
        destination[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}
