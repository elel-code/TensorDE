use std::hash::{Hash, Hasher};

use bytemuck::Pod;

pub(crate) fn vertex_pair_hash<T: Pod>(first: &[T], second: &[T]) -> Option<u64> {
    if first.is_empty() && second.is_empty() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    first.len().hash(&mut hasher);
    bytemuck::cast_slice::<T, u8>(first).hash(&mut hasher);
    second.len().hash(&mut hasher);
    bytemuck::cast_slice::<T, u8>(second).hash(&mut hasher);
    Some(hasher.finish())
}

pub(crate) fn hash_bytes_with_len(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.len().hash(&mut hasher);
    bytes.hash(&mut hasher);
    hasher.finish()
}
