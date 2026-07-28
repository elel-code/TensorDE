//! Required extensions and extension feature bits for profile revision 11.

use vulkanalia::Instance;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

pub(super) const ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS: &[&str] = &[
    "VK_KHR_global_priority",
    "VK_KHR_load_store_op_none",
    "VK_KHR_shader_quad_control",
    "VK_KHR_shader_maximal_reconvergence",
    "VK_KHR_shader_subgroup_uniform_control_flow",
    "VK_KHR_map_memory2",
    "VK_KHR_dynamic_rendering",
    "VK_KHR_shader_subgroup_rotate",
    "VK_KHR_shader_float_controls2",
    "VK_KHR_shader_expect_assume",
    "VK_KHR_line_rasterization",
    "VK_KHR_vertex_attribute_divisor",
    "VK_KHR_index_type_uint8",
    "VK_KHR_maintenance5",
    "VK_KHR_dynamic_rendering_local_read",
    "VK_KHR_push_descriptor",
    "VK_KHR_robustness2",
    "VK_KHR_pipeline_binary",
    "VK_KHR_fragment_shading_rate",
    "VK_KHR_shader_clock",
    "VK_KHR_workgroup_memory_explicit_layout",
    "VK_KHR_compute_shader_derivatives",
    "VK_KHR_maintenance7",
    "VK_KHR_maintenance8",
    "VK_KHR_maintenance9",
    "VK_KHR_depth_clamp_zero_one",
    "VK_KHR_copy_memory_indirect",
    "VK_KHR_shader_untyped_pointers",
    "VK_KHR_swapchain",
    "VK_KHR_present_mode_fifo_latest_ready",
    "VK_KHR_present_id2",
    "VK_KHR_present_wait2",
    "VK_KHR_swapchain_maintenance1",
    "VK_KHR_cooperative_matrix",
];

pub(super) fn query_roadmap_2026_extension_features(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device_extensions: &[String],
) -> Vec<&'static str> {
    let mut missing = Vec::new();
    macro_rules! require_extension_feature {
        ($extension:literal, $feature_type:ty, $field:ident, $name:literal) => {
            if extension_available(device_extensions, $extension) {
                let mut feature = <$feature_type>::default();
                query_feature_struct(instance, physical_device, &mut feature);
                if feature.$field == 0 {
                    missing.push($name);
                }
            }
        };
    }

    require_extension_feature!(
        "VK_KHR_shader_quad_control",
        vk::PhysicalDeviceShaderQuadControlFeaturesKHR,
        shader_quad_control,
        "shaderQuadControl"
    );
    require_extension_feature!(
        "VK_KHR_shader_maximal_reconvergence",
        vk::PhysicalDeviceShaderMaximalReconvergenceFeaturesKHR,
        shader_maximal_reconvergence,
        "shaderMaximalReconvergence"
    );
    require_extension_feature!(
        "VK_KHR_shader_subgroup_uniform_control_flow",
        vk::PhysicalDeviceShaderSubgroupUniformControlFlowFeaturesKHR,
        shader_subgroup_uniform_control_flow,
        "shaderSubgroupUniformControlFlow"
    );

    if extension_available(device_extensions, "VK_KHR_robustness2") {
        let mut feature = vk::PhysicalDeviceRobustness2FeaturesKHR::default();
        query_feature_struct(instance, physical_device, &mut feature);
        for (supported, name) in [
            (feature.robust_buffer_access2, "robustBufferAccess2"),
            (feature.robust_image_access2, "robustImageAccess2"),
            (feature.null_descriptor, "nullDescriptor"),
        ] {
            if supported == 0 {
                missing.push(name);
            }
        }
    }
    require_extension_feature!(
        "VK_KHR_pipeline_binary",
        vk::PhysicalDevicePipelineBinaryFeaturesKHR,
        pipeline_binaries,
        "pipelineBinaries"
    );
    require_extension_feature!(
        "VK_KHR_fragment_shading_rate",
        vk::PhysicalDeviceFragmentShadingRateFeaturesKHR,
        pipeline_fragment_shading_rate,
        "pipelineFragmentShadingRate"
    );
    require_extension_feature!(
        "VK_KHR_shader_clock",
        vk::PhysicalDeviceShaderClockFeaturesKHR,
        shader_subgroup_clock,
        "shaderSubgroupClock"
    );
    require_extension_feature!(
        "VK_KHR_workgroup_memory_explicit_layout",
        vk::PhysicalDeviceWorkgroupMemoryExplicitLayoutFeaturesKHR,
        workgroup_memory_explicit_layout,
        "workgroupMemoryExplicitLayout"
    );
    require_extension_feature!(
        "VK_KHR_compute_shader_derivatives",
        vk::PhysicalDeviceComputeShaderDerivativesFeaturesKHR,
        compute_derivative_group_linear,
        "computeDerivativeGroupLinear"
    );
    require_extension_feature!(
        "VK_KHR_maintenance7",
        vk::PhysicalDeviceMaintenance7FeaturesKHR,
        maintenance7,
        "maintenance7"
    );
    require_extension_feature!(
        "VK_KHR_maintenance8",
        vk::PhysicalDeviceMaintenance8FeaturesKHR,
        maintenance8,
        "maintenance8"
    );
    require_extension_feature!(
        "VK_KHR_maintenance9",
        vk::PhysicalDeviceMaintenance9FeaturesKHR,
        maintenance9,
        "maintenance9"
    );
    require_extension_feature!(
        "VK_KHR_depth_clamp_zero_one",
        vk::PhysicalDeviceDepthClampZeroOneFeaturesKHR,
        depth_clamp_zero_one,
        "depthClampZeroOne"
    );
    require_extension_feature!(
        "VK_KHR_copy_memory_indirect",
        vk::PhysicalDeviceCopyMemoryIndirectFeaturesKHR,
        indirect_memory_copy,
        "indirectMemoryCopy"
    );
    require_extension_feature!(
        "VK_KHR_shader_untyped_pointers",
        vk::PhysicalDeviceShaderUntypedPointersFeaturesKHR,
        shader_untyped_pointers,
        "shaderUntypedPointers"
    );
    require_extension_feature!(
        "VK_KHR_present_mode_fifo_latest_ready",
        vk::PhysicalDevicePresentModeFifoLatestReadyFeaturesKHR,
        present_mode_fifo_latest_ready,
        "presentModeFifoLatestReady"
    );
    require_extension_feature!(
        "VK_KHR_present_id2",
        vk::PhysicalDevicePresentId2FeaturesKHR,
        present_id2,
        "presentId2"
    );
    require_extension_feature!(
        "VK_KHR_present_wait2",
        vk::PhysicalDevicePresentWait2FeaturesKHR,
        present_wait2,
        "presentWait2"
    );
    require_extension_feature!(
        "VK_KHR_swapchain_maintenance1",
        vk::PhysicalDeviceSwapchainMaintenance1FeaturesKHR,
        swapchain_maintenance1,
        "swapchainMaintenance1"
    );
    require_extension_feature!(
        "VK_KHR_cooperative_matrix",
        vk::PhysicalDeviceCooperativeMatrixFeaturesKHR,
        cooperative_matrix,
        "cooperativeMatrix"
    );
    missing
}

fn query_feature_struct<T>(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    feature: &mut T,
) where
    T: vk::Cast<Target = T> + vk::ExtendsPhysicalDeviceFeatures2,
{
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(feature)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }
}

pub(super) fn extension_available(available: &[String], required: &str) -> bool {
    available.iter().any(|extension| extension == required)
}
