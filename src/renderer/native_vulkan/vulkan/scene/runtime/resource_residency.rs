//! End-of-run scene GPU resource residency evidence.

use serde::Serialize;

use crate::renderer::native_vulkan::vulkan::descriptor_heap::NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot;
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaBufferSnapshot, NativeVulkanVulkanaliaImageSnapshot,
};

use super::SceneGpuResources;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneMemoryClassSnapshot {
    pub resource_class: &'static str,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
    pub allocation_count: usize,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneResourceResidencySnapshot {
    pub all_images_device_local: bool,
    pub image_device_local_bytes: u64,
    pub image_host_visible_bytes: u64,
    pub image_memory_classes: Vec<NativeVulkanSceneMemoryClassSnapshot>,
    pub buffer_device_local_bytes: u64,
    pub buffer_host_visible_bytes: u64,
    pub buffer_memory_classes: Vec<NativeVulkanSceneMemoryClassSnapshot>,
}

pub(super) fn scene_resource_residency_snapshot(
    resources: &SceneGpuResources,
) -> NativeVulkanSceneResourceResidencySnapshot {
    let mut image_memory_classes = Vec::new();
    if let Some(upload) = &resources.white_upload {
        add_image_class(
            &mut image_memory_classes,
            "fallback-image",
            &upload.image.snapshot,
        );
    }
    for texture in &resources.scene_textures {
        add_image_class(
            &mut image_memory_classes,
            "scene-texture",
            &texture.upload.image.snapshot,
        );
    }
    for target in &resources.effect_targets {
        add_image_class(
            &mut image_memory_classes,
            "effect-target",
            &target.image.snapshot,
        );
    }
    for target in &resources.scene_color_msaa_targets {
        add_image_class(
            &mut image_memory_classes,
            "scene-color-msaa-target",
            &target.snapshot,
        );
    }

    let mut buffer_memory_classes = Vec::new();
    add_buffer_class(
        &mut buffer_memory_classes,
        "vertex-buffer",
        &resources.vertex_buffer.snapshot,
    );
    add_buffer_class(
        &mut buffer_memory_classes,
        "index-buffer",
        &resources.index_buffer.snapshot,
    );
    for frame in &resources.frame_resources {
        add_buffer_class(
            &mut buffer_memory_classes,
            "transform-buffer",
            &frame.transform_buffer.snapshot,
        );
        if let Some(buffer) = &frame.material_buffer {
            add_buffer_class(
                &mut buffer_memory_classes,
                "material-buffer",
                &buffer.snapshot,
            );
        }
        if let Some(buffer) = &frame.skinning_buffer {
            add_buffer_class(
                &mut buffer_memory_classes,
                "skinning-buffer",
                &buffer.snapshot,
            );
        }
        add_descriptor_heap_class(
            &mut buffer_memory_classes,
            "descriptor-resource-heap",
            &frame.descriptor_heap.snapshot.resource_heap,
        );
        if let Some(heap) = &frame.descriptor_heap.snapshot.sampler_heap {
            add_descriptor_heap_class(&mut buffer_memory_classes, "descriptor-sampler-heap", heap);
        }
    }

    NativeVulkanSceneResourceResidencySnapshot {
        all_images_device_local: image_memory_classes.iter().all(is_device_local),
        image_device_local_bytes: memory_bytes_with_flag(&image_memory_classes, "device-local"),
        image_host_visible_bytes: memory_bytes_with_flag(&image_memory_classes, "host-visible"),
        buffer_device_local_bytes: memory_bytes_with_flag(&buffer_memory_classes, "device-local"),
        buffer_host_visible_bytes: memory_bytes_with_flag(&buffer_memory_classes, "host-visible"),
        image_memory_classes,
        buffer_memory_classes,
    }
}

fn add_image_class(
    classes: &mut Vec<NativeVulkanSceneMemoryClassSnapshot>,
    resource_class: &'static str,
    snapshot: &NativeVulkanVulkanaliaImageSnapshot,
) {
    add_memory_class(
        classes,
        resource_class,
        snapshot.selected_memory_type_index,
        &snapshot.selected_memory_property_flags,
        snapshot.memory_size,
    );
}

fn add_buffer_class(
    classes: &mut Vec<NativeVulkanSceneMemoryClassSnapshot>,
    resource_class: &'static str,
    snapshot: &NativeVulkanVulkanaliaBufferSnapshot,
) {
    add_memory_class(
        classes,
        resource_class,
        snapshot.selected_memory_type_index,
        &snapshot.selected_memory_property_flags,
        snapshot.memory_size,
    );
}

fn add_descriptor_heap_class(
    classes: &mut Vec<NativeVulkanSceneMemoryClassSnapshot>,
    resource_class: &'static str,
    snapshot: &NativeVulkanVulkanaliaDescriptorHeapBufferSnapshot,
) {
    add_memory_class(
        classes,
        resource_class,
        snapshot.selected_memory_type_index,
        &snapshot.selected_memory_property_flags,
        snapshot.memory_size,
    );
}

fn add_memory_class(
    classes: &mut Vec<NativeVulkanSceneMemoryClassSnapshot>,
    resource_class: &'static str,
    selected_memory_type_index: u32,
    selected_memory_property_flags: &[&'static str],
    memory_bytes: u64,
) {
    if let Some(class) = classes.iter_mut().find(|class| {
        class.resource_class == resource_class
            && class.selected_memory_type_index == selected_memory_type_index
            && class.selected_memory_property_flags == selected_memory_property_flags
    }) {
        class.allocation_count += 1;
        class.memory_bytes = class.memory_bytes.saturating_add(memory_bytes);
        return;
    }
    classes.push(NativeVulkanSceneMemoryClassSnapshot {
        resource_class,
        selected_memory_type_index,
        selected_memory_property_flags: selected_memory_property_flags.to_vec(),
        allocation_count: 1,
        memory_bytes,
    });
}

fn is_device_local(class: &NativeVulkanSceneMemoryClassSnapshot) -> bool {
    class
        .selected_memory_property_flags
        .contains(&"device-local")
}

fn memory_bytes_with_flag(
    classes: &[NativeVulkanSceneMemoryClassSnapshot],
    flag: &'static str,
) -> u64 {
    classes
        .iter()
        .filter(|class| class.selected_memory_property_flags.contains(&flag))
        .map(|class| class.memory_bytes)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_classes_merge_only_matching_resource_and_memory_types() {
        let mut classes = Vec::new();
        add_memory_class(&mut classes, "texture", 1, &["device-local"], 64);
        add_memory_class(&mut classes, "texture", 1, &["device-local"], 128);
        add_memory_class(&mut classes, "target", 1, &["device-local"], 256);
        add_memory_class(&mut classes, "texture", 2, &["host-visible"], 32);

        assert_eq!(classes.len(), 3);
        assert_eq!(classes[0].allocation_count, 2);
        assert_eq!(classes[0].memory_bytes, 192);
        assert_eq!(memory_bytes_with_flag(&classes, "device-local"), 448);
        assert_eq!(memory_bytes_with_flag(&classes, "host-visible"), 32);
    }
}
