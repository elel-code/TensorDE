//! Exact `VP_KHR_roadmap_2026` revision 11 baseline requirements.
//!
//! The source contract is Khronos' `VP_KHR_roadmap.json`, profile version 1,
//! API version 1.4.328, revision 11 dated 2026-01-28.

use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

pub(crate) const ROADMAP_2026_API_VERSION: Version = Version::new(1, 4, 328);

pub(crate) const ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS: &[&str] = &[
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Roadmap2026CoreRequirementProbe {
    pub missing_features: Vec<&'static str>,
    pub missing_properties: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Roadmap2026DeviceRequirementProbe {
    pub api_version_ready: bool,
    pub missing_device_extensions: Vec<&'static str>,
    pub missing_core_features: Vec<&'static str>,
    pub missing_properties: Vec<&'static str>,
    pub missing_extension_features: Vec<&'static str>,
}

impl Roadmap2026DeviceRequirementProbe {
    pub fn ready(&self) -> bool {
        self.api_version_ready
            && self.missing_device_extensions.is_empty()
            && self.missing_core_features.is_empty()
            && self.missing_properties.is_empty()
            && self.missing_extension_features.is_empty()
    }
}

pub(crate) fn roadmap_2026_api_version_ready(api_version: u32) -> bool {
    api_version >= u32::from(ROADMAP_2026_API_VERSION)
}

pub(crate) fn query_roadmap_2026_device_requirements(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    api_version: u32,
    device_extensions: &[String],
) -> Roadmap2026DeviceRequirementProbe {
    let api_version_ready = roadmap_2026_api_version_ready(api_version);
    let core = api_version_ready
        .then(|| query_roadmap_2026_core_requirements(instance, physical_device))
        .unwrap_or(Roadmap2026CoreRequirementProbe {
            missing_features: Vec::new(),
            missing_properties: Vec::new(),
        });
    Roadmap2026DeviceRequirementProbe {
        api_version_ready,
        missing_device_extensions: ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS
            .iter()
            .copied()
            .filter(|required| !extension_available(device_extensions, required))
            .collect(),
        missing_core_features: core.missing_features,
        missing_properties: core.missing_properties,
        missing_extension_features: query_roadmap_2026_extension_features(
            instance,
            physical_device,
            device_extensions,
        ),
    }
}

fn query_roadmap_2026_extension_features(
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

fn extension_available(available: &[String], required: &str) -> bool {
    available.iter().any(|extension| extension == required)
}

pub(crate) fn query_roadmap_2026_core_requirements(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> Roadmap2026CoreRequirementProbe {
    let mut vulkan11_features = vk::PhysicalDeviceVulkan11Features::default();
    let mut vulkan12_features = vk::PhysicalDeviceVulkan12Features::default();
    let mut vulkan13_features = vk::PhysicalDeviceVulkan13Features::default();
    let mut vulkan14_features = vk::PhysicalDeviceVulkan14Features::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut vulkan11_features)
        .push_next(&mut vulkan12_features)
        .push_next(&mut vulkan13_features)
        .push_next(&mut vulkan14_features)
        .build();
    unsafe {
        instance.get_physical_device_features2(physical_device, &mut features2);
    }

    let mut missing_features = Vec::new();
    macro_rules! require_feature {
        ($value:expr, $name:literal) => {
            if $value == 0 {
                missing_features.push($name);
            }
        };
    }
    let core = features2.features;
    require_feature!(core.robust_buffer_access, "robustBufferAccess");
    require_feature!(core.full_draw_index_uint32, "fullDrawIndexUint32");
    require_feature!(core.image_cube_array, "imageCubeArray");
    require_feature!(core.independent_blend, "independentBlend");
    require_feature!(core.sample_rate_shading, "sampleRateShading");
    require_feature!(
        core.draw_indirect_first_instance,
        "drawIndirectFirstInstance"
    );
    require_feature!(core.depth_clamp, "depthClamp");
    require_feature!(core.depth_bias_clamp, "depthBiasClamp");
    require_feature!(core.sampler_anisotropy, "samplerAnisotropy");
    require_feature!(core.occlusion_query_precise, "occlusionQueryPrecise");
    require_feature!(core.fragment_stores_and_atomics, "fragmentStoresAndAtomics");
    require_feature!(
        core.shader_storage_image_extended_formats,
        "shaderStorageImageExtendedFormats"
    );
    require_feature!(
        core.shader_uniform_buffer_array_dynamic_indexing,
        "shaderUniformBufferArrayDynamicIndexing"
    );
    require_feature!(
        core.shader_sampled_image_array_dynamic_indexing,
        "shaderSampledImageArrayDynamicIndexing"
    );
    require_feature!(
        core.shader_storage_buffer_array_dynamic_indexing,
        "shaderStorageBufferArrayDynamicIndexing"
    );
    require_feature!(
        core.shader_storage_image_array_dynamic_indexing,
        "shaderStorageImageArrayDynamicIndexing"
    );
    require_feature!(core.multi_draw_indirect, "multiDrawIndirect");
    require_feature!(core.shader_int16, "shaderInt16");
    require_feature!(
        core.shader_image_gather_extended,
        "shaderImageGatherExtended"
    );

    require_feature!(vulkan11_features.multiview, "multiview");
    require_feature!(
        vulkan11_features.sampler_ycbcr_conversion,
        "samplerYcbcrConversion"
    );
    require_feature!(
        vulkan11_features.shader_draw_parameters,
        "shaderDrawParameters"
    );
    require_feature!(
        vulkan11_features.storage_buffer_16bit_access,
        "storageBuffer16BitAccess"
    );

    for (supported, name) in [
        (
            vulkan12_features.uniform_buffer_standard_layout,
            "uniformBufferStandardLayout",
        ),
        (
            vulkan12_features.subgroup_broadcast_dynamic_id,
            "subgroupBroadcastDynamicId",
        ),
        (
            vulkan12_features.imageless_framebuffer,
            "imagelessFramebuffer",
        ),
        (
            vulkan12_features.separate_depth_stencil_layouts,
            "separateDepthStencilLayouts",
        ),
        (vulkan12_features.host_query_reset, "hostQueryReset"),
        (vulkan12_features.timeline_semaphore, "timelineSemaphore"),
        (
            vulkan12_features.shader_subgroup_extended_types,
            "shaderSubgroupExtendedTypes",
        ),
        (vulkan12_features.vulkan_memory_model, "vulkanMemoryModel"),
        (
            vulkan12_features.vulkan_memory_model_device_scope,
            "vulkanMemoryModelDeviceScope",
        ),
        (
            vulkan12_features.buffer_device_address,
            "bufferDeviceAddress",
        ),
        (
            vulkan12_features.sampler_mirror_clamp_to_edge,
            "samplerMirrorClampToEdge",
        ),
        (vulkan12_features.descriptor_indexing, "descriptorIndexing"),
        (
            vulkan12_features.shader_uniform_texel_buffer_array_dynamic_indexing,
            "shaderUniformTexelBufferArrayDynamicIndexing",
        ),
        (
            vulkan12_features.shader_storage_texel_buffer_array_dynamic_indexing,
            "shaderStorageTexelBufferArrayDynamicIndexing",
        ),
        (
            vulkan12_features.shader_uniform_buffer_array_non_uniform_indexing,
            "shaderUniformBufferArrayNonUniformIndexing",
        ),
        (
            vulkan12_features.shader_sampled_image_array_non_uniform_indexing,
            "shaderSampledImageArrayNonUniformIndexing",
        ),
        (
            vulkan12_features.shader_storage_buffer_array_non_uniform_indexing,
            "shaderStorageBufferArrayNonUniformIndexing",
        ),
        (
            vulkan12_features.shader_storage_image_array_non_uniform_indexing,
            "shaderStorageImageArrayNonUniformIndexing",
        ),
        (
            vulkan12_features.shader_uniform_texel_buffer_array_non_uniform_indexing,
            "shaderUniformTexelBufferArrayNonUniformIndexing",
        ),
        (
            vulkan12_features.shader_storage_texel_buffer_array_non_uniform_indexing,
            "shaderStorageTexelBufferArrayNonUniformIndexing",
        ),
        (
            vulkan12_features.descriptor_binding_sampled_image_update_after_bind,
            "descriptorBindingSampledImageUpdateAfterBind",
        ),
        (
            vulkan12_features.descriptor_binding_storage_image_update_after_bind,
            "descriptorBindingStorageImageUpdateAfterBind",
        ),
        (
            vulkan12_features.descriptor_binding_storage_buffer_update_after_bind,
            "descriptorBindingStorageBufferUpdateAfterBind",
        ),
        (
            vulkan12_features.descriptor_binding_uniform_texel_buffer_update_after_bind,
            "descriptorBindingUniformTexelBufferUpdateAfterBind",
        ),
        (
            vulkan12_features.descriptor_binding_storage_texel_buffer_update_after_bind,
            "descriptorBindingStorageTexelBufferUpdateAfterBind",
        ),
        (
            vulkan12_features.descriptor_binding_update_unused_while_pending,
            "descriptorBindingUpdateUnusedWhilePending",
        ),
        (
            vulkan12_features.descriptor_binding_partially_bound,
            "descriptorBindingPartiallyBound",
        ),
        (
            vulkan12_features.descriptor_binding_variable_descriptor_count,
            "descriptorBindingVariableDescriptorCount",
        ),
        (
            vulkan12_features.runtime_descriptor_array,
            "runtimeDescriptorArray",
        ),
        (vulkan12_features.scalar_block_layout, "scalarBlockLayout"),
        (vulkan12_features.shader_int8, "shaderInt8"),
        (vulkan12_features.shader_float16, "shaderFloat16"),
        (
            vulkan12_features.storage_buffer_8bit_access,
            "storageBuffer8BitAccess",
        ),
    ] {
        if supported == 0 {
            missing_features.push(name);
        }
    }

    for (supported, name) in [
        (vulkan13_features.robust_image_access, "robustImageAccess"),
        (
            vulkan13_features.shader_terminate_invocation,
            "shaderTerminateInvocation",
        ),
        (
            vulkan13_features.shader_zero_initialize_workgroup_memory,
            "shaderZeroInitializeWorkgroupMemory",
        ),
        (vulkan13_features.synchronization2, "synchronization2"),
        (
            vulkan13_features.shader_integer_dot_product,
            "shaderIntegerDotProduct",
        ),
        (vulkan13_features.maintenance4, "maintenance4"),
        (
            vulkan13_features.pipeline_creation_cache_control,
            "pipelineCreationCacheControl",
        ),
        (
            vulkan13_features.subgroup_size_control,
            "subgroupSizeControl",
        ),
        (
            vulkan13_features.compute_full_subgroups,
            "computeFullSubgroups",
        ),
        (
            vulkan13_features.shader_demote_to_helper_invocation,
            "shaderDemoteToHelperInvocation",
        ),
        (vulkan13_features.inline_uniform_block, "inlineUniformBlock"),
        (vulkan13_features.dynamic_rendering, "dynamicRendering"),
        (
            vulkan13_features.descriptor_binding_inline_uniform_block_update_after_bind,
            "descriptorBindingInlineUniformBlockUpdateAfterBind",
        ),
    ] {
        if supported == 0 {
            missing_features.push(name);
        }
    }

    for (supported, name) in [
        (
            vulkan14_features.shader_subgroup_rotate,
            "shaderSubgroupRotate",
        ),
        (vulkan14_features.shader_expect_assume, "shaderExpectAssume"),
        (
            vulkan14_features.shader_float_controls2,
            "shaderFloatControls2",
        ),
        (vulkan14_features.rectangular_lines, "rectangularLines"),
        (vulkan14_features.bresenham_lines, "bresenhamLines"),
        (vulkan14_features.smooth_lines, "smoothLines"),
        (
            vulkan14_features.stippled_rectangular_lines,
            "stippledRectangularLines",
        ),
        (
            vulkan14_features.stippled_bresenham_lines,
            "stippledBresenhamLines",
        ),
        (
            vulkan14_features.stippled_smooth_lines,
            "stippledSmoothLines",
        ),
        (
            vulkan14_features.vertex_attribute_instance_rate_divisor,
            "vertexAttributeInstanceRateDivisor",
        ),
        (vulkan14_features.index_type_uint8, "indexTypeUint8"),
        (
            vulkan14_features.dynamic_rendering_local_read,
            "dynamicRenderingLocalRead",
        ),
        (vulkan14_features.maintenance5, "maintenance5"),
        (vulkan14_features.host_image_copy, "hostImageCopy"),
        (vulkan14_features.push_descriptor, "pushDescriptor"),
    ] {
        if supported == 0 {
            missing_features.push(name);
        }
    }

    let mut vulkan11_properties = vk::PhysicalDeviceVulkan11Properties::default();
    let mut vulkan12_properties = vk::PhysicalDeviceVulkan12Properties::default();
    let mut vulkan13_properties = vk::PhysicalDeviceVulkan13Properties::default();
    let mut properties2 = vk::PhysicalDeviceProperties2::builder()
        .push_next(&mut vulkan11_properties)
        .push_next(&mut vulkan12_properties)
        .push_next(&mut vulkan13_properties)
        .build();
    unsafe {
        instance.get_physical_device_properties2(physical_device, &mut properties2);
    }
    let limits = properties2.properties.limits;
    let mut missing_properties = Vec::new();
    macro_rules! require_min {
        ($value:expr, $required:expr, $name:literal) => {
            if $value < $required {
                missing_properties.push($name);
            }
        };
    }
    macro_rules! require_max {
        ($value:expr, $required:expr, $name:literal) => {
            if $value > $required {
                missing_properties.push($name);
            }
        };
    }
    macro_rules! require_property {
        ($value:expr, $name:literal) => {
            if $value == 0 {
                missing_properties.push($name);
            }
        };
    }

    require_min!(limits.max_image_dimension_1d, 8192, "maxImageDimension1D");
    require_min!(limits.max_image_dimension_2d, 8192, "maxImageDimension2D");
    require_min!(
        limits.max_image_dimension_cube,
        8192,
        "maxImageDimensionCube"
    );
    require_min!(limits.max_image_array_layers, 2048, "maxImageArrayLayers");
    require_min!(
        limits.max_uniform_buffer_range,
        65536,
        "maxUniformBufferRange"
    );
    require_max!(
        limits.buffer_image_granularity,
        4096,
        "bufferImageGranularity"
    );
    require_min!(
        limits.max_per_stage_descriptor_samplers,
        64,
        "maxPerStageDescriptorSamplers"
    );
    require_min!(
        limits.max_per_stage_descriptor_uniform_buffers,
        200,
        "maxPerStageDescriptorUniformBuffers"
    );
    require_min!(
        limits.max_per_stage_descriptor_storage_buffers,
        200,
        "maxPerStageDescriptorStorageBuffers"
    );
    require_min!(
        limits.max_per_stage_descriptor_sampled_images,
        200,
        "maxPerStageDescriptorSampledImages"
    );
    require_min!(
        limits.max_per_stage_descriptor_storage_images,
        16,
        "maxPerStageDescriptorStorageImages"
    );
    require_min!(
        limits.max_per_stage_descriptor_input_attachments,
        8,
        "maxPerStageDescriptorInputAttachments"
    );
    require_min!(limits.max_per_stage_resources, 200, "maxPerStageResources");
    require_min!(
        limits.max_descriptor_set_samplers,
        576,
        "maxDescriptorSetSamplers"
    );
    require_min!(
        limits.max_descriptor_set_uniform_buffers,
        1800,
        "maxDescriptorSetUniformBuffers"
    );
    require_min!(
        limits.max_descriptor_set_storage_buffers,
        1800,
        "maxDescriptorSetStorageBuffers"
    );
    require_min!(
        limits.max_descriptor_set_sampled_images,
        1800,
        "maxDescriptorSetSampledImages"
    );
    require_min!(
        limits.max_descriptor_set_storage_images,
        144,
        "maxDescriptorSetStorageImages"
    );
    require_min!(
        limits.max_descriptor_set_input_attachments,
        8,
        "maxDescriptorSetInputAttachments"
    );
    require_min!(
        limits.max_fragment_combined_output_resources,
        16,
        "maxFragmentCombinedOutputResources"
    );
    require_min!(
        limits.max_compute_work_group_invocations,
        256,
        "maxComputeWorkGroupInvocations"
    );
    for (value, required, name) in [
        (
            limits.max_compute_work_group_size[0],
            256,
            "maxComputeWorkGroupSize[0]",
        ),
        (
            limits.max_compute_work_group_size[1],
            256,
            "maxComputeWorkGroupSize[1]",
        ),
        (
            limits.max_compute_work_group_size[2],
            64,
            "maxComputeWorkGroupSize[2]",
        ),
    ] {
        if value < required {
            missing_properties.push(name);
        }
    }
    require_min!(limits.sub_texel_precision_bits, 8, "subTexelPrecisionBits");
    require_min!(limits.mipmap_precision_bits, 6, "mipmapPrecisionBits");
    if limits.max_sampler_lod_bias < 14.0 {
        missing_properties.push("maxSamplerLodBias");
    }
    require_property!(limits.standard_sample_locations, "standardSampleLocations");
    require_min!(limits.max_color_attachments, 8, "maxColorAttachments");
    require_property!(
        limits.timestamp_compute_and_graphics,
        "timestampComputeAndGraphics"
    );
    require_min!(
        limits.max_bound_descriptor_sets,
        7,
        "maxBoundDescriptorSets"
    );
    require_min!(
        limits.max_vertex_output_components,
        124,
        "maxVertexOutputComponents"
    );
    require_min!(
        limits.max_tessellation_control_per_vertex_input_components,
        128,
        "maxTessellationControlPerVertexInputComponents"
    );
    require_min!(
        limits.max_tessellation_control_per_vertex_output_components,
        128,
        "maxTessellationControlPerVertexOutputComponents"
    );
    require_min!(
        limits.max_tessellation_control_total_output_components,
        4096,
        "maxTessellationControlTotalOutputComponents"
    );
    require_min!(
        limits.max_tessellation_evaluation_input_components,
        128,
        "maxTessellationEvaluationInputComponents"
    );
    require_min!(
        limits.max_tessellation_evaluation_output_components,
        128,
        "maxTessellationEvaluationOutputComponents"
    );
    require_min!(
        limits.max_geometry_output_components,
        128,
        "maxGeometryOutputComponents"
    );
    require_min!(
        limits.max_fragment_input_components,
        112,
        "maxFragmentInputComponents"
    );
    require_min!(
        limits.max_fragment_output_attachments,
        8,
        "maxFragmentOutputAttachments"
    );
    require_min!(
        limits.max_compute_shared_memory_size,
        32768,
        "maxComputeSharedMemorySize"
    );
    require_min!(limits.sub_pixel_precision_bits, 8, "subPixelPrecisionBits");
    require_min!(
        limits.max_viewport_dimensions[0],
        8192,
        "maxViewportDimensions[0]"
    );
    require_min!(
        limits.max_viewport_dimensions[1],
        8192,
        "maxViewportDimensions[1]"
    );
    require_min!(limits.max_framebuffer_width, 8192, "maxFramebufferWidth");
    require_min!(limits.max_framebuffer_height, 8192, "maxFramebufferHeight");

    require_min!(
        vulkan11_properties.max_multiview_view_count,
        6,
        "maxMultiviewViewCount"
    );
    require_min!(
        vulkan11_properties.max_multiview_instance_index,
        134217727,
        "maxMultiviewInstanceIndex"
    );
    require_min!(vulkan11_properties.subgroup_size, 4, "subgroupSize");
    if !vulkan11_properties
        .subgroup_supported_stages
        .contains(vk::ShaderStageFlags::COMPUTE | vk::ShaderStageFlags::FRAGMENT)
    {
        missing_properties.push("subgroupSupportedStages");
    }
    let required_subgroup_operations = vk::SubgroupFeatureFlags::BASIC
        | vk::SubgroupFeatureFlags::VOTE
        | vk::SubgroupFeatureFlags::ARITHMETIC
        | vk::SubgroupFeatureFlags::BALLOT
        | vk::SubgroupFeatureFlags::SHUFFLE
        | vk::SubgroupFeatureFlags::SHUFFLE_RELATIVE
        | vk::SubgroupFeatureFlags::QUAD;
    if !vulkan11_properties
        .subgroup_supported_operations
        .contains(required_subgroup_operations)
    {
        missing_properties.push("subgroupSupportedOperations");
    }

    require_min!(
        vulkan12_properties.max_timeline_semaphore_value_difference,
        2147483647,
        "maxTimelineSemaphoreValueDifference"
    );
    require_property!(
        vulkan12_properties.shader_signed_zero_inf_nan_preserve_float16,
        "shaderSignedZeroInfNanPreserveFloat16"
    );
    require_property!(
        vulkan12_properties.shader_signed_zero_inf_nan_preserve_float32,
        "shaderSignedZeroInfNanPreserveFloat32"
    );
    require_property!(
        vulkan12_properties.shader_rounding_mode_rte_float16,
        "shaderRoundingModeRTEFloat16"
    );
    require_property!(
        vulkan12_properties.shader_rounding_mode_rte_float32,
        "shaderRoundingModeRTEFloat32"
    );
    for (value, required, name) in [
        (
            vulkan12_properties.max_per_stage_descriptor_update_after_bind_samplers,
            500000,
            "maxPerStageDescriptorUpdateAfterBindSamplers",
        ),
        (
            vulkan12_properties.max_per_stage_descriptor_update_after_bind_uniform_buffers,
            12,
            "maxPerStageDescriptorUpdateAfterBindUniformBuffers",
        ),
        (
            vulkan12_properties.max_per_stage_descriptor_update_after_bind_storage_buffers,
            500000,
            "maxPerStageDescriptorUpdateAfterBindStorageBuffers",
        ),
        (
            vulkan12_properties.max_per_stage_descriptor_update_after_bind_sampled_images,
            500000,
            "maxPerStageDescriptorUpdateAfterBindSampledImages",
        ),
        (
            vulkan12_properties.max_per_stage_descriptor_update_after_bind_storage_images,
            500000,
            "maxPerStageDescriptorUpdateAfterBindStorageImages",
        ),
        (
            vulkan12_properties.max_per_stage_descriptor_update_after_bind_input_attachments,
            7,
            "maxPerStageDescriptorUpdateAfterBindInputAttachments",
        ),
        (
            vulkan12_properties.max_per_stage_update_after_bind_resources,
            500000,
            "maxPerStageUpdateAfterBindResources",
        ),
        (
            vulkan12_properties.max_descriptor_set_update_after_bind_samplers,
            500000,
            "maxDescriptorSetUpdateAfterBindSamplers",
        ),
        (
            vulkan12_properties.max_descriptor_set_update_after_bind_uniform_buffers,
            72,
            "maxDescriptorSetUpdateAfterBindUniformBuffers",
        ),
        (
            vulkan12_properties.max_descriptor_set_update_after_bind_uniform_buffers_dynamic,
            8,
            "maxDescriptorSetUpdateAfterBindUniformBuffersDynamic",
        ),
        (
            vulkan12_properties.max_descriptor_set_update_after_bind_storage_buffers,
            500000,
            "maxDescriptorSetUpdateAfterBindStorageBuffers",
        ),
        (
            vulkan12_properties.max_descriptor_set_update_after_bind_storage_buffers_dynamic,
            4,
            "maxDescriptorSetUpdateAfterBindStorageBuffersDynamic",
        ),
        (
            vulkan12_properties.max_descriptor_set_update_after_bind_sampled_images,
            500000,
            "maxDescriptorSetUpdateAfterBindSampledImages",
        ),
        (
            vulkan12_properties.max_descriptor_set_update_after_bind_storage_images,
            500000,
            "maxDescriptorSetUpdateAfterBindStorageImages",
        ),
        (
            vulkan12_properties.max_descriptor_set_update_after_bind_input_attachments,
            7,
            "maxDescriptorSetUpdateAfterBindInputAttachments",
        ),
    ] {
        if value < required {
            missing_properties.push(name);
        }
    }
    for (value, required, name) in [
        (
            vulkan13_properties.max_buffer_size,
            1073741824,
            "maxBufferSize",
        ),
        (
            u64::from(vulkan13_properties.max_inline_uniform_block_size),
            256,
            "maxInlineUniformBlockSize",
        ),
        (
            u64::from(vulkan13_properties.max_per_stage_descriptor_inline_uniform_blocks),
            4,
            "maxPerStageDescriptorInlineUniformBlocks",
        ),
        (
            u64::from(
                vulkan13_properties
                    .max_per_stage_descriptor_update_after_bind_inline_uniform_blocks,
            ),
            4,
            "maxPerStageDescriptorUpdateAfterBindInlineUniformBlocks",
        ),
        (
            u64::from(vulkan13_properties.max_descriptor_set_inline_uniform_blocks),
            4,
            "maxDescriptorSetInlineUniformBlocks",
        ),
        (
            u64::from(
                vulkan13_properties.max_descriptor_set_update_after_bind_inline_uniform_blocks,
            ),
            4,
            "maxDescriptorSetUpdateAfterBindInlineUniformBlocks",
        ),
        (
            u64::from(vulkan13_properties.max_inline_uniform_total_size),
            256,
            "maxInlineUniformTotalSize",
        ),
    ] {
        if value < required {
            missing_properties.push(name);
        }
    }

    Roadmap2026CoreRequirementProbe {
        missing_features,
        missing_properties,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roadmap_2026_api_floor_is_the_profile_version_not_vulkan_1_4_zero() {
        assert!(!roadmap_2026_api_version_ready(u32::from(Version::V1_4_0)));
        assert!(!roadmap_2026_api_version_ready(u32::from(Version::new(
            1, 4, 327
        ))));
        assert!(roadmap_2026_api_version_ready(u32::from(
            ROADMAP_2026_API_VERSION
        )));
    }

    #[test]
    fn roadmap_2026_required_extensions_include_inherited_profile_capabilities() {
        assert!(ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS.contains(&"VK_KHR_global_priority"));
        assert!(ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS.contains(&"VK_KHR_shader_quad_control"));
        assert!(
            ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS
                .contains(&"VK_KHR_workgroup_memory_explicit_layout")
        );
        assert!(!ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS.contains(&"VK_KHR_maintenance10"));
    }
}
