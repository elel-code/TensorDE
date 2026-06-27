use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

pub(in crate::renderer::native_vulkan::vulkan) const DESCRIPTOR_HEAP_EXTENSION_NAME: &str =
    "VK_EXT_descriptor_heap";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaFeatureChainTemplate {
    pub api: &'static str,
    pub chain_root: &'static str,
    pub feature_structs: Vec<&'static str>,
    pub requested_feature_fields: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaCoreFeatureSnapshot {
    pub timeline_semaphore: bool,
    pub scalar_block_layout: bool,
    pub descriptor_indexing: bool,
    pub runtime_descriptor_array: bool,
    pub buffer_device_address: bool,
    pub synchronization2: bool,
    pub dynamic_rendering: bool,
    pub maintenance4: bool,
    pub dynamic_rendering_local_read: bool,
    pub maintenance5: bool,
    pub maintenance6: bool,
    pub push_descriptor: bool,
    pub descriptor_heap: bool,
    pub descriptor_heap_capture_replay: bool,
    pub host_image_copy: bool,
    pub index_type_uint8: bool,
    pub shader_subgroup_rotate: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVulkan14PropertySnapshot {
    pub max_push_descriptors: u32,
    pub max_combined_image_sampler_descriptor_count: u32,
    pub dynamic_rendering_local_read_depth_stencil_attachments: bool,
    pub dynamic_rendering_local_read_multisampled_attachments: bool,
    pub identical_memory_type_requirements: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
    pub sampler_heap_alignment: u64,
    pub resource_heap_alignment: u64,
    pub max_sampler_heap_size: u64,
    pub max_resource_heap_size: u64,
    pub min_sampler_heap_reserved_range: u64,
    pub min_sampler_heap_reserved_range_with_embedded: u64,
    pub min_resource_heap_reserved_range: u64,
    pub sampler_descriptor_size: u64,
    pub image_descriptor_size: u64,
    pub buffer_descriptor_size: u64,
    pub sampler_descriptor_alignment: u64,
    pub image_descriptor_alignment: u64,
    pub buffer_descriptor_alignment: u64,
    pub max_push_data_size: u64,
    pub max_descriptor_heap_embedded_samplers: u32,
    pub sampler_ycbcr_conversion_count: u32,
    pub sparse_descriptor_heaps: bool,
    pub protected_descriptor_heaps: bool,
}

pub fn native_vulkan_vulkanalia_feature_chain_template()
-> NativeVulkanVulkanaliaFeatureChainTemplate {
    let mut vulkan12_features = vk::PhysicalDeviceVulkan12Features::builder()
        .timeline_semaphore(true)
        .scalar_block_layout(true)
        .descriptor_indexing(true)
        .runtime_descriptor_array(true)
        .build();
    let mut vulkan13_features = vk::PhysicalDeviceVulkan13Features::builder()
        .synchronization2(true)
        .dynamic_rendering(true)
        .maintenance4(true)
        .build();
    let mut vulkan14_features = vk::PhysicalDeviceVulkan14Features::builder()
        .dynamic_rendering_local_read(true)
        .maintenance5(true)
        .maintenance6(true)
        .push_descriptor(true)
        .build();
    let mut descriptor_heap_features = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::builder()
        .descriptor_heap(true)
        .build();
    let features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut vulkan12_features)
        .push_next(&mut vulkan13_features)
        .push_next(&mut vulkan14_features)
        .push_next(&mut descriptor_heap_features)
        .build();

    NativeVulkanVulkanaliaFeatureChainTemplate {
        api: "Vulkan 1.4",
        chain_root: std::any::type_name_of_val(&features2),
        feature_structs: vec![
            std::any::type_name_of_val(&vulkan12_features),
            std::any::type_name_of_val(&vulkan13_features),
            std::any::type_name_of_val(&vulkan14_features),
            std::any::type_name_of_val(&descriptor_heap_features),
        ],
        requested_feature_fields: &[
            "timeline_semaphore",
            "scalar_block_layout",
            "descriptor_indexing",
            "runtime_descriptor_array",
            "synchronization2",
            "dynamic_rendering",
            "dynamic_rendering_local_read",
            "maintenance5",
            "maintenance6",
            "push_descriptor",
            "descriptor_heap",
        ],
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_core_feature_snapshot(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> (
    NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaVulkan14PropertySnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
) {
    let mut vulkan12_features = vk::PhysicalDeviceVulkan12Features::default();
    let mut vulkan13_features = vk::PhysicalDeviceVulkan13Features::default();
    let mut vulkan14_features = vk::PhysicalDeviceVulkan14Features::default();
    let mut descriptor_heap_features = vk::PhysicalDeviceDescriptorHeapFeaturesEXT::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut vulkan12_features)
        .push_next(&mut vulkan13_features)
        .push_next(&mut vulkan14_features)
        .push_next(&mut descriptor_heap_features)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }

    let mut vulkan14_properties = vk::PhysicalDeviceVulkan14Properties::default();
    let mut descriptor_heap_properties = vk::PhysicalDeviceDescriptorHeapPropertiesEXT::default();
    let mut properties2 = vk::PhysicalDeviceProperties2::builder()
        .push_next(&mut vulkan14_properties)
        .push_next(&mut descriptor_heap_properties)
        .build();
    unsafe {
        instance.get_physical_device_properties2(physical_device, &mut properties2);
    }

    (
        NativeVulkanVulkanaliaCoreFeatureSnapshot {
            timeline_semaphore: vulkan12_features.timeline_semaphore != 0,
            scalar_block_layout: vulkan12_features.scalar_block_layout != 0,
            descriptor_indexing: vulkan12_features.descriptor_indexing != 0,
            runtime_descriptor_array: vulkan12_features.runtime_descriptor_array != 0,
            buffer_device_address: vulkan12_features.buffer_device_address != 0,
            synchronization2: vulkan13_features.synchronization2 != 0,
            dynamic_rendering: vulkan13_features.dynamic_rendering != 0,
            maintenance4: vulkan13_features.maintenance4 != 0,
            dynamic_rendering_local_read: vulkan14_features.dynamic_rendering_local_read != 0,
            maintenance5: vulkan14_features.maintenance5 != 0,
            maintenance6: vulkan14_features.maintenance6 != 0,
            push_descriptor: vulkan14_features.push_descriptor != 0,
            descriptor_heap: descriptor_heap_features.descriptor_heap != 0,
            descriptor_heap_capture_replay: descriptor_heap_features.descriptor_heap_capture_replay
                != 0,
            host_image_copy: vulkan14_features.host_image_copy != 0,
            index_type_uint8: vulkan14_features.index_type_uint8 != 0,
            shader_subgroup_rotate: vulkan14_features.shader_subgroup_rotate != 0,
        },
        NativeVulkanVulkanaliaVulkan14PropertySnapshot {
            max_push_descriptors: vulkan14_properties.max_push_descriptors,
            max_combined_image_sampler_descriptor_count: vulkan14_properties
                .max_combined_image_sampler_descriptor_count,
            dynamic_rendering_local_read_depth_stencil_attachments: vulkan14_properties
                .dynamic_rendering_local_read_depth_stencil_attachments
                != 0,
            dynamic_rendering_local_read_multisampled_attachments: vulkan14_properties
                .dynamic_rendering_local_read_multisampled_attachments
                != 0,
            identical_memory_type_requirements: vulkan14_properties
                .identical_memory_type_requirements
                != 0,
        },
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
            sampler_heap_alignment: descriptor_heap_properties.sampler_heap_alignment,
            resource_heap_alignment: descriptor_heap_properties.resource_heap_alignment,
            max_sampler_heap_size: descriptor_heap_properties.max_sampler_heap_size,
            max_resource_heap_size: descriptor_heap_properties.max_resource_heap_size,
            min_sampler_heap_reserved_range: descriptor_heap_properties
                .min_sampler_heap_reserved_range,
            min_sampler_heap_reserved_range_with_embedded: descriptor_heap_properties
                .min_sampler_heap_reserved_range_with_embedded,
            min_resource_heap_reserved_range: descriptor_heap_properties
                .min_resource_heap_reserved_range,
            sampler_descriptor_size: descriptor_heap_properties.sampler_descriptor_size,
            image_descriptor_size: descriptor_heap_properties.image_descriptor_size,
            buffer_descriptor_size: descriptor_heap_properties.buffer_descriptor_size,
            sampler_descriptor_alignment: descriptor_heap_properties.sampler_descriptor_alignment,
            image_descriptor_alignment: descriptor_heap_properties.image_descriptor_alignment,
            buffer_descriptor_alignment: descriptor_heap_properties.buffer_descriptor_alignment,
            max_push_data_size: descriptor_heap_properties.max_push_data_size,
            max_descriptor_heap_embedded_samplers: descriptor_heap_properties
                .max_descriptor_heap_embedded_samplers,
            sampler_ycbcr_conversion_count: descriptor_heap_properties
                .sampler_ycbcr_conversion_count,
            sparse_descriptor_heaps: descriptor_heap_properties.sparse_descriptor_heaps != 0,
            protected_descriptor_heaps: descriptor_heap_properties.protected_descriptor_heaps != 0,
        },
    )
}

impl NativeVulkanVulkanaliaCoreFeatureSnapshot {
    pub(in crate::renderer::native_vulkan::vulkan) fn enables_vulkan_1_2_features(self) -> bool {
        self.timeline_semaphore
            || self.scalar_block_layout
            || self.descriptor_indexing
            || self.runtime_descriptor_array
            || self.buffer_device_address
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn enables_vulkan_1_3_features(self) -> bool {
        self.synchronization2 || self.dynamic_rendering || self.maintenance4
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn enables_vulkan_1_4_features(self) -> bool {
        self.dynamic_rendering_local_read
            || self.maintenance5
            || self.maintenance6
            || self.push_descriptor
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn enables_descriptor_heap_features(
        self,
    ) -> bool {
        self.descriptor_heap || self.descriptor_heap_capture_replay
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_vulkan12_device_features(
    core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
) -> vk::PhysicalDeviceVulkan12Features {
    vk::PhysicalDeviceVulkan12Features::builder()
        .timeline_semaphore(core_features.timeline_semaphore)
        .scalar_block_layout(core_features.scalar_block_layout)
        .descriptor_indexing(core_features.descriptor_indexing)
        .runtime_descriptor_array(core_features.runtime_descriptor_array)
        .buffer_device_address(core_features.buffer_device_address)
        .build()
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_vulkan13_device_features(
    core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
) -> vk::PhysicalDeviceVulkan13Features {
    vk::PhysicalDeviceVulkan13Features::builder()
        .synchronization2(core_features.synchronization2)
        .dynamic_rendering(core_features.dynamic_rendering)
        .maintenance4(core_features.maintenance4)
        .build()
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_vulkan14_device_features(
    core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
) -> vk::PhysicalDeviceVulkan14Features {
    vk::PhysicalDeviceVulkan14Features::builder()
        .dynamic_rendering_local_read(core_features.dynamic_rendering_local_read)
        .maintenance5(core_features.maintenance5)
        .maintenance6(core_features.maintenance6)
        .push_descriptor(core_features.push_descriptor)
        .build()
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_descriptor_heap_device_features(
    core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
) -> vk::PhysicalDeviceDescriptorHeapFeaturesEXT {
    vk::PhysicalDeviceDescriptorHeapFeaturesEXT::builder()
        .descriptor_heap(core_features.descriptor_heap)
        .descriptor_heap_capture_replay(core_features.descriptor_heap_capture_replay)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_chain_template_uses_vulkan_1_4_feature_struct() {
        let template = native_vulkan_vulkanalia_feature_chain_template();
        assert_eq!(template.api, "Vulkan 1.4");
        assert!(template.chain_root.ends_with("PhysicalDeviceFeatures2"));
        assert!(
            template
                .feature_structs
                .iter()
                .any(|name| name.ends_with("PhysicalDeviceVulkan12Features"))
        );
        assert!(
            template
                .feature_structs
                .iter()
                .any(|name| name.ends_with("PhysicalDeviceVulkan13Features"))
        );
        assert!(
            template
                .feature_structs
                .iter()
                .any(|name| name.ends_with("PhysicalDeviceVulkan14Features"))
        );
        assert!(
            template
                .feature_structs
                .iter()
                .any(|name| name.ends_with("PhysicalDeviceDescriptorHeapFeaturesEXT"))
        );
        assert!(
            template
                .requested_feature_fields
                .contains(&"timeline_semaphore")
        );
        assert!(
            template
                .requested_feature_fields
                .contains(&"synchronization2")
        );
        assert!(
            template
                .requested_feature_fields
                .contains(&"dynamic_rendering_local_read")
        );
        assert!(template.requested_feature_fields.contains(&"maintenance6"));
        assert!(
            template
                .requested_feature_fields
                .contains(&"descriptor_heap")
        );
    }

    #[test]
    fn core_feature_snapshot_covers_vulkan_1_2_1_3_and_1_4_decisions() {
        let features = NativeVulkanVulkanaliaCoreFeatureSnapshot {
            timeline_semaphore: true,
            scalar_block_layout: true,
            descriptor_indexing: true,
            runtime_descriptor_array: true,
            buffer_device_address: false,
            synchronization2: true,
            dynamic_rendering: true,
            maintenance4: true,
            dynamic_rendering_local_read: true,
            maintenance5: true,
            maintenance6: true,
            push_descriptor: true,
            descriptor_heap: true,
            descriptor_heap_capture_replay: false,
            host_image_copy: false,
            index_type_uint8: true,
            shader_subgroup_rotate: true,
        };
        let properties = NativeVulkanVulkanaliaVulkan14PropertySnapshot {
            max_push_descriptors: 32,
            max_combined_image_sampler_descriptor_count: 1_048_576,
            dynamic_rendering_local_read_depth_stencil_attachments: true,
            dynamic_rendering_local_read_multisampled_attachments: false,
            identical_memory_type_requirements: true,
        };
        let descriptor_heap_properties = NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
            sampler_heap_alignment: 64,
            resource_heap_alignment: 64,
            max_sampler_heap_size: 1024,
            max_resource_heap_size: 4096,
            min_sampler_heap_reserved_range: 0,
            min_sampler_heap_reserved_range_with_embedded: 0,
            min_resource_heap_reserved_range: 0,
            sampler_descriptor_size: 16,
            image_descriptor_size: 32,
            buffer_descriptor_size: 32,
            sampler_descriptor_alignment: 16,
            image_descriptor_alignment: 32,
            buffer_descriptor_alignment: 32,
            max_push_data_size: 256,
            max_descriptor_heap_embedded_samplers: 16,
            sampler_ycbcr_conversion_count: 8,
            sparse_descriptor_heaps: true,
            protected_descriptor_heaps: false,
        };

        assert!(features.timeline_semaphore);
        assert!(features.synchronization2);
        assert!(features.dynamic_rendering);
        assert!(features.dynamic_rendering_local_read);
        assert!(features.push_descriptor);
        assert!(features.descriptor_heap);
        assert_eq!(properties.max_push_descriptors, 32);
        assert!(properties.identical_memory_type_requirements);
        assert_eq!(descriptor_heap_properties.image_descriptor_size, 32);
        assert!(descriptor_heap_properties.sparse_descriptor_heaps);
    }

    #[test]
    fn core_feature_snapshot_builds_device_feature_chains() {
        let features = NativeVulkanVulkanaliaCoreFeatureSnapshot {
            timeline_semaphore: true,
            dynamic_rendering: true,
            maintenance5: true,
            push_descriptor: true,
            descriptor_heap: true,
            ..NativeVulkanVulkanaliaCoreFeatureSnapshot::default()
        };

        let vulkan12 = native_vulkan_vulkanalia_vulkan12_device_features(features);
        let vulkan13 = native_vulkan_vulkanalia_vulkan13_device_features(features);
        let vulkan14 = native_vulkan_vulkanalia_vulkan14_device_features(features);
        let descriptor_heap = native_vulkan_vulkanalia_descriptor_heap_device_features(features);

        assert!(features.enables_vulkan_1_2_features());
        assert!(features.enables_vulkan_1_3_features());
        assert!(features.enables_vulkan_1_4_features());
        assert_ne!(vulkan12.timeline_semaphore, 0);
        assert_ne!(vulkan13.dynamic_rendering, 0);
        assert_ne!(vulkan14.maintenance5, 0);
        assert_ne!(vulkan14.push_descriptor, 0);
        assert_ne!(descriptor_heap.descriptor_heap, 0);
    }
}
