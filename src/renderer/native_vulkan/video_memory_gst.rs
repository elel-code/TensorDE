//! GStreamer decoded sample memory classification for video frontends/importers.

use gstreamer as gst;

pub(super) fn native_vulkan_gst_memory_types(buffer: &gst::BufferRef) -> Vec<String> {
    (0..buffer.n_memory())
        .map(|index| native_vulkan_gst_memory_type(buffer.peek_memory(index)))
        .collect()
}

pub(super) fn native_vulkan_gst_memory_type(memory: &gst::MemoryRef) -> String {
    for memory_type in [
        "CUDAMemory",
        "GLMemory",
        "DMABuf",
        "VAMemory",
        "SystemMemory",
    ] {
        if memory.is_type(memory_type) {
            return memory_type.to_owned();
        }
    }
    let Some(memory_type) = memory
        .allocator()
        .map(|allocator| allocator.memory_type().to_string())
    else {
        return "unknown".to_owned();
    };
    let lower = memory_type.to_ascii_lowercase();
    if lower.contains("cuda") {
        "CUDAMemory".to_owned()
    } else if lower.contains("gl") {
        "GLMemory".to_owned()
    } else if lower.contains("dmabuf") || lower.contains("dma-buf") {
        "DMABuf".to_owned()
    } else if lower.contains("va") {
        "VAMemory".to_owned()
    } else if lower.contains("system") {
        "SystemMemory".to_owned()
    } else {
        memory_type
    }
}
