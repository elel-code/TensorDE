#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) struct NativeVulkanByteSummary {
    pub(super) hash: u64,
    pub(super) nonzero_bytes: u64,
    pub(super) min: u8,
    pub(super) max: u8,
    pub(super) unique_values: u32,
}

#[cfg(any(feature = "native-vulkan-video", test))]
pub(super) fn native_vulkan_byte_summary(bytes: &[u8]) -> NativeVulkanByteSummary {
    let mut seen = [false; 256];
    let mut nonzero_bytes = 0u64;
    let mut min = u8::MAX;
    let mut max = u8::MIN;
    for byte in bytes.iter().copied() {
        seen[byte as usize] = true;
        if byte != 0 {
            nonzero_bytes = nonzero_bytes.saturating_add(1);
        }
        min = min.min(byte);
        max = max.max(byte);
    }
    NativeVulkanByteSummary {
        hash: native_vulkan_stable_byte_hash(bytes),
        nonzero_bytes,
        min: if bytes.is_empty() { 0 } else { min },
        max: if bytes.is_empty() { 0 } else { max },
        unique_values: seen.into_iter().filter(|value| *value).count() as u32,
    }
}

pub(super) fn native_vulkan_stable_byte_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
