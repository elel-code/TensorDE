use crate::renderer::native_vulkan::{
    NativeVulkanAv1SequenceHeaderSnapshot, NativeVulkanH264ParameterSetSnapshot,
    NativeVulkanH265ParameterSetSnapshot, NativeVulkanVideoSessionCodec,
};
use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::instance::{
    native_vulkan_vulkanalia_create_instance, native_vulkan_vulkanalia_destroy_instance,
};
use super::video_bitstream_buffer::{
    NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot,
    native_vulkan_vulkanalia_create_video_session_bitstream_buffer,
    native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer,
    native_vulkan_vulkanalia_smoke_create_video_session_bitstream_buffer,
};
use super::video_codec::{
    native_vulkan_vulkanalia_video_session_codec_name as vulkanalia_video_session_codec_name,
    native_vulkan_vulkanalia_video_session_codec_operation as vulkanalia_video_session_codec_operation,
    native_vulkan_vulkanalia_video_session_label as vulkanalia_video_session_label,
};
use super::video_command_pool::{
    native_vulkan_vulkanalia_create_decode_command_buffer,
    native_vulkan_vulkanalia_destroy_decode_command_buffer,
};
use super::video_decode_commands::{
    native_vulkan_vulkanalia_record_av1_decode_command_buffer,
    native_vulkan_vulkanalia_record_h264_decode_command_buffer,
    native_vulkan_vulkanalia_record_h265_decode_command_buffer,
    native_vulkan_vulkanalia_submit_decode_command_buffer2,
};
use super::video_decode_payload::{
    native_vulkan_vulkanalia_av1_decode_payloads, native_vulkan_vulkanalia_h264_decode_payloads,
    native_vulkan_vulkanalia_h265_decode_payloads,
};
use super::video_decode_submit::NativeVulkanVulkanaliaDecodeImageViewBindings;
use super::video_decode_submit_av1::{
    NativeVulkanVulkanaliaAv1ReadyPrefixCommandFrameSnapshot,
    NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
    native_vulkan_vulkanalia_av1_ready_prefix_decode_submit_plan,
};
use super::video_decode_submit_h264::{
    NativeVulkanVulkanaliaH264ParameterIds,
    NativeVulkanVulkanaliaH264ReadyPrefixCommandFrameSnapshot,
    NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput,
    native_vulkan_vulkanalia_h264_ready_prefix_decode_submit_plan,
};
use super::video_decode_submit_h265::{
    NativeVulkanVulkanaliaH265ParameterIds,
    NativeVulkanVulkanaliaH265ReadyPrefixCommandFrameSnapshot,
    NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
    native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan,
};
use super::video_device::{
    NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    NativeVulkanVulkanaliaVideoPhysicalDeviceSelection,
    native_vulkan_vulkanalia_create_video_decode_device,
    native_vulkan_vulkanalia_destroy_video_decode_device,
    native_vulkan_vulkanalia_select_video_decode_physical_device,
};
use super::video_format_probe::native_vulkan_vulkanalia_video_format_probe;
use super::video_profile_labels::{
    video_capability_flag_labels, video_decode_capability_flag_labels,
};
use super::video_session::{
    NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    native_vulkan_vulkanalia_bind_video_session_memory_resources,
    native_vulkan_vulkanalia_create_video_session, native_vulkan_vulkanalia_destroy_video_session,
    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources,
    native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe,
};
use super::video_session_capabilities::{
    VulkanaliaVideoSessionCapabilityQuery,
    native_vulkan_vulkanalia_video_format_probe_includes_format as video_format_probe_includes_format,
    native_vulkan_vulkanalia_video_session_effective_format_probe_profile,
    native_vulkan_vulkanalia_video_session_effective_picture_format,
    native_vulkan_vulkanalia_video_session_effective_profile_label,
    native_vulkan_vulkanalia_video_session_extent_supported,
    native_vulkan_vulkanalia_video_session_max_active_reference_pictures,
    native_vulkan_vulkanalia_video_session_max_dpb_slots,
    with_native_vulkan_vulkanalia_video_session_capabilities,
};
use super::video_session_images::{
    NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot,
    native_vulkan_vulkanalia_create_video_session_resource_image,
    native_vulkan_vulkanalia_destroy_video_session_resource_image,
    native_vulkan_vulkanalia_smoke_create_video_session_resource_image,
};
use super::video_session_parameters::{
    NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot,
    native_vulkan_vulkanalia_destroy_video_session_parameters,
    native_vulkan_vulkanalia_smoke_create_empty_video_session_parameters,
};
use super::video_session_parameters_av1::{
    native_vulkan_vulkanalia_create_av1_video_session_parameters,
    native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters,
};
use super::video_session_parameters_h264::{
    native_vulkan_vulkanalia_create_h264_video_session_parameters,
    native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters,
};
use super::video_session_parameters_h265::{
    native_vulkan_vulkanalia_create_h265_video_session_parameters,
    native_vulkan_vulkanalia_smoke_create_h265_video_session_parameters,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
    pub allocate_video_images: bool,
    pub allocate_bitstream_buffer: bool,
    pub bitstream_buffer_size: u64,
    pub create_empty_session_parameters: bool,
    pub create_session_parameters: bool,
    pub h264_parameter_sets: Option<NativeVulkanH264ParameterSetSnapshot>,
    pub h265_parameter_sets: Option<NativeVulkanH265ParameterSetSnapshot>,
    pub av1_sequence_header: Option<NativeVulkanAv1SequenceHeaderSnapshot>,
    pub h264_ready_prefix_decode: Option<NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput>,
    pub h265_ready_prefix_decode: Option<NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput>,
    pub av1_ready_prefix_decode: Option<NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput>,
}

impl Default for NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
    fn default() -> Self {
        Self {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            width: 3840,
            height: 2160,
            allocate_video_images: false,
            allocate_bitstream_buffer: false,
            bitstream_buffer_size: 8 * 1024 * 1024,
            create_empty_session_parameters: false,
            create_session_parameters: false,
            h264_parameter_sets: None,
            h265_parameter_sets: None,
            av1_sequence_header: None,
            h264_ready_prefix_decode: None,
            h265_ready_prefix_decode: None,
            av1_ready_prefix_decode: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot {
    pub binding: &'static str,
    pub loader: String,
    pub requested_api_version: String,
    pub requested_codec: NativeVulkanVideoSessionCodec,
    pub requested_extent: (u32, u32),
    pub selected_physical_device_index: usize,
    pub selected_physical_device_name: String,
    pub selected_physical_device_type: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub api_version: String,
    pub driver_version: u32,
    pub selected_queue_family_index: u32,
    pub selected_queue_count: u32,
    pub selected_queue_flags: Vec<&'static str>,
    pub enabled_device_extensions: Vec<&'static str>,
    pub synchronization2_enabled: bool,
    pub dynamic_rendering_enabled: bool,
    pub sampler_ycbcr_conversion_enabled: bool,
    pub video_maintenance1_enabled: bool,
    pub video_maintenance2_enabled: bool,
    pub inline_session_parameters_enabled: bool,
    pub inline_session_parameter_codecs: Vec<&'static str>,
    pub ffmpeg_submit_model: &'static str,
    pub video_codec_operation: Vec<&'static str>,
    pub profile: &'static str,
    pub format_probe_profile: &'static str,
    pub picture_format: String,
    pub reference_picture_format: String,
    pub target_picture_dpb_supported: bool,
    pub target_picture_sampled_output_supported: bool,
    pub target_resource_plan: NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    pub capability_flags: Vec<&'static str>,
    pub decode_capability_flags: Vec<&'static str>,
    pub min_bitstream_buffer_offset_alignment: u64,
    pub min_bitstream_buffer_size_alignment: u64,
    pub picture_access_granularity: (u32, u32),
    pub min_coded_extent: (u32, u32),
    pub max_coded_extent: (u32, u32),
    pub requested_extent_supported: bool,
    pub driver_max_dpb_slots: u32,
    pub driver_max_active_reference_pictures: u32,
    pub session_max_dpb_slots: u32,
    pub session_max_active_reference_pictures: u32,
    pub codec_max_level: Option<&'static str>,
    pub codec_max_level_raw: Option<i32>,
    pub std_header_version_name: String,
    pub std_header_version_spec_version: u32,
    pub memory_binding: NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    pub resource_image_requested: bool,
    pub resource_image: Option<NativeVulkanVulkanaliaVideoSessionResourceImageSmokeSnapshot>,
    pub bitstream_buffer_requested: bool,
    pub bitstream_buffer: Option<NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot>,
    pub session_parameters_requested: bool,
    pub session_parameters: Option<NativeVulkanVulkanaliaVideoSessionParametersSmokeSnapshot>,
    pub h264_ready_prefix_decode_requested: bool,
    pub h264_ready_prefix_decode: Option<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot>,
    pub h265_ready_prefix_decode_requested: bool,
    pub h265_ready_prefix_decode: Option<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot>,
    pub av1_ready_prefix_decode_requested: bool,
    pub av1_ready_prefix_decode: Option<NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot>,
}

pub fn probe_native_vulkan_vulkanalia_video_session_bind(
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let vulkan = native_vulkan_vulkanalia_create_instance()?;
    let result = probe_native_vulkan_vulkanalia_video_session_bind_inner(
        &vulkan.instance,
        vulkan.loader_name,
        options,
    );
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn probe_native_vulkan_vulkanalia_video_session_bind_inner(
    instance: &Instance,
    loader_name: &'static str,
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let selection =
        native_vulkan_vulkanalia_select_video_decode_physical_device(instance, options.codec)?;
    let requested_extent = vk::Extent2D {
        width: options.width,
        height: options.height,
    };
    let h264_parameter_sets = options.h264_parameter_sets.clone();
    let av1_sequence_header = options.av1_sequence_header.clone();
    let picture_format = native_vulkan_vulkanalia_video_session_effective_picture_format(
        options.codec,
        av1_sequence_header.as_ref(),
    );
    let picture_format_label = format!("{picture_format:?}");
    let video_format_capabilities = native_vulkan_vulkanalia_video_format_probe(
        instance,
        selection.physical_device,
        &selection.device_extensions,
        true,
    );
    let format_probe_profile =
        native_vulkan_vulkanalia_video_session_effective_format_probe_profile(
            options.codec,
            h264_parameter_sets.as_ref(),
            av1_sequence_header.as_ref(),
        )?;
    let target_resource_plan =
        native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe(
            &video_format_capabilities,
        )
        .into_iter()
        .find(|plan| {
            plan.codec == vulkanalia_video_session_codec_name(options.codec)
                && plan.profile == format_probe_profile
        })
        .ok_or_else(|| {
            format!(
                "missing Vulkanalia video format resource plan for {} {}",
                vulkanalia_video_session_codec_name(options.codec),
                format_probe_profile
            )
        })?;
    let target_picture_sampled_output_supported = video_format_probe_includes_format(
        &video_format_capabilities.decode_output_sampled_formats,
        vulkanalia_video_session_codec_name(options.codec),
        format_probe_profile,
        &picture_format_label,
    );
    let target_picture_dpb_supported = video_format_probe_includes_format(
        &video_format_capabilities.dpb_formats,
        vulkanalia_video_session_codec_name(options.codec),
        format_probe_profile,
        &picture_format_label,
    );
    if !target_picture_sampled_output_supported || !target_picture_dpb_supported {
        return Err(format!(
            "{} lacks {picture_format_label} decode sampled-output/DPB support in Vulkanalia probe",
            vulkanalia_video_session_label(options.codec),
        ));
    }

    let video_decode_device = native_vulkan_vulkanalia_create_video_decode_device(
        instance,
        selection.physical_device,
        selection.queue_family_index,
        options.codec,
        &selection.device_extensions,
        vulkanalia_video_session_decode_submit_requested(&options),
    )?;

    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let result = with_native_vulkan_vulkanalia_video_session_capabilities(
        instance,
        selection.physical_device,
        options.codec,
        h264_parameter_sets.as_ref(),
        av1_sequence_header.as_ref(),
        |profile_info, queried| {
            smoke_bind_vulkanalia_video_session_profile(
                instance,
                &video_decode_device.device,
                video_decode_device.queue,
                &memory_properties,
                &selection,
                loader_name,
                options,
                requested_extent,
                picture_format,
                target_picture_dpb_supported,
                target_picture_sampled_output_supported,
                target_resource_plan,
                video_decode_device.enabled_device_extensions.clone(),
                video_decode_device.feature_selection,
                profile_info,
                queried,
            )
        },
    );

    native_vulkan_vulkanalia_destroy_video_decode_device(video_decode_device);
    result
}

fn vulkanalia_video_session_decode_submit_requested(
    options: &NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> bool {
    options.h264_ready_prefix_decode.is_some()
        || options.h265_ready_prefix_decode.is_some()
        || options.av1_ready_prefix_decode.is_some()
}

fn smoke_bind_vulkanalia_video_session_profile(
    instance: &Instance,
    device: &Device,
    queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    selection: &NativeVulkanVulkanaliaVideoPhysicalDeviceSelection,
    loader_name: &'static str,
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
    requested_extent: vk::Extent2D,
    picture_format: vk::Format,
    target_picture_dpb_supported: bool,
    target_picture_sampled_output_supported: bool,
    target_resource_plan: NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    enabled_device_extensions: Vec<&'static str>,
    feature_selection: NativeVulkanVulkanaliaVideoDeviceFeatureSelection,
    profile_info: &vk::VideoProfileInfoKHR,
    queried: VulkanaliaVideoSessionCapabilityQuery,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let capabilities = queried.capabilities;
    let effective_profile_label = native_vulkan_vulkanalia_video_session_effective_profile_label(
        options.codec,
        options.h264_parameter_sets.as_ref(),
        options.av1_sequence_header.as_ref(),
    )?;
    let requested_extent_supported =
        native_vulkan_vulkanalia_video_session_extent_supported(requested_extent, capabilities);
    if !requested_extent_supported {
        return Err(format!(
            "requested Vulkanalia video extent {}x{} is outside ({}, {})..({}, {}) or is not aligned to ({}, {})",
            requested_extent.width,
            requested_extent.height,
            capabilities.min_coded_extent.width,
            capabilities.min_coded_extent.height,
            capabilities.max_coded_extent.width,
            capabilities.max_coded_extent.height,
            capabilities.picture_access_granularity.width,
            capabilities.picture_access_granularity.height,
        ));
    }

    let session_max_dpb_slots =
        native_vulkan_vulkanalia_video_session_max_dpb_slots(capabilities.max_dpb_slots);
    let session_max_active_reference_pictures =
        native_vulkan_vulkanalia_video_session_max_active_reference_pictures(
            capabilities.max_active_reference_pictures,
            session_max_dpb_slots,
        );
    let create_info = vk::VideoSessionCreateInfoKHR::builder()
        .queue_family_index(selection.queue_family_index)
        .video_profile(profile_info)
        .picture_format(picture_format)
        .reference_picture_format(picture_format)
        .max_coded_extent(requested_extent)
        .max_dpb_slots(session_max_dpb_slots)
        .max_active_reference_pictures(session_max_active_reference_pictures)
        .std_header_version(&capabilities.std_header_version)
        .build();
    let session = native_vulkan_vulkanalia_create_video_session(device, &create_info)?;
    let mut memory_resources = None;
    let result = (|| -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
        let resources = native_vulkan_vulkanalia_bind_video_session_memory_resources(
            device,
            memory_properties,
            session,
        )?;
        let memory_binding = resources.snapshot.clone();
        memory_resources = Some(resources);
        let resource_image = if options.allocate_video_images {
            Some(
                native_vulkan_vulkanalia_smoke_create_video_session_resource_image(
                    instance,
                    device,
                    memory_properties,
                    selection.physical_device,
                    profile_info,
                    requested_extent,
                    session_max_dpb_slots.max(1),
                    picture_format,
                    queried.decode_capability_flags,
                    &[selection.queue_family_index],
                )?,
            )
        } else {
            None
        };
        let bitstream_buffer = if options.allocate_bitstream_buffer {
            Some(
                native_vulkan_vulkanalia_smoke_create_video_session_bitstream_buffer(
                    device,
                    memory_properties,
                    profile_info,
                    options.bitstream_buffer_size,
                    capabilities.min_bitstream_buffer_size_alignment,
                    None,
                    false,
                )?,
            )
        } else {
            None
        };
        let session_parameters = if options.create_session_parameters {
            Some(match options.codec {
                NativeVulkanVideoSessionCodec::H264High8 => {
                    let parameter_sets = options.h264_parameter_sets.as_ref().ok_or_else(|| {
                        "Vulkanalia real H.264 session parameters require parsed H.264 parameter sets"
                            .to_owned()
                    })?;
                    native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters(
                        device,
                        session,
                        options.codec,
                        parameter_sets,
                    )
                }
                NativeVulkanVideoSessionCodec::H265Main8
                | NativeVulkanVideoSessionCodec::H265Main10 => {
                    let parameter_sets = options.h265_parameter_sets.as_ref().ok_or_else(|| {
                        "Vulkanalia real H.265 session parameters require parsed H.265 parameter sets"
                            .to_owned()
                    })?;
                    native_vulkan_vulkanalia_smoke_create_h265_video_session_parameters(
                        device,
                        session,
                        options.codec,
                        parameter_sets,
                    )
                }
                NativeVulkanVideoSessionCodec::Av1Main8
                | NativeVulkanVideoSessionCodec::Av1Main10 => {
                    let sequence_header = options.av1_sequence_header.as_ref().ok_or_else(|| {
                        "Vulkanalia real AV1 session parameters require parsed AV1 sequence header"
                            .to_owned()
                    })?;
                    native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters(
                        device,
                        session,
                        options.codec,
                        sequence_header,
                    )
                }
            })
        } else if options.create_empty_session_parameters {
            Some(
                native_vulkan_vulkanalia_smoke_create_empty_video_session_parameters(
                    device,
                    session,
                    options.codec,
                ),
            )
        } else {
            None
        };
        let h264_ready_prefix_decode =
            if let Some(input) = options.h264_ready_prefix_decode.as_ref() {
                Some(
                    native_vulkan_vulkanalia_record_h264_ready_prefix_decode_smoke(
                        instance,
                        device,
                        queue,
                        memory_properties,
                        selection,
                        profile_info,
                        requested_extent,
                        picture_format,
                        queried.decode_capability_flags,
                        capabilities,
                        session,
                        options.codec,
                        session_max_dpb_slots.max(1),
                        options.bitstream_buffer_size,
                        input,
                    )?,
                )
            } else {
                None
            };
        let h265_ready_prefix_decode =
            if let Some(input) = options.h265_ready_prefix_decode.as_ref() {
                Some(
                    native_vulkan_vulkanalia_record_h265_ready_prefix_decode_smoke(
                        instance,
                        device,
                        queue,
                        memory_properties,
                        selection,
                        profile_info,
                        requested_extent,
                        picture_format,
                        queried.decode_capability_flags,
                        capabilities,
                        session,
                        options.codec,
                        session_max_dpb_slots.max(1),
                        options.bitstream_buffer_size,
                        input,
                    )?,
                )
            } else {
                None
            };
        let av1_ready_prefix_decode = if let Some(input) = options.av1_ready_prefix_decode.as_ref()
        {
            Some(
                native_vulkan_vulkanalia_record_av1_ready_prefix_decode_smoke(
                    instance,
                    device,
                    queue,
                    memory_properties,
                    selection,
                    profile_info,
                    requested_extent,
                    picture_format,
                    queried.decode_capability_flags,
                    capabilities,
                    session,
                    options.codec,
                    session_max_dpb_slots.max(1),
                    options.bitstream_buffer_size,
                    input,
                )?,
            )
        } else {
            None
        };

        Ok(NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot {
            binding: "vulkanalia",
            loader: loader_name.to_owned(),
            requested_api_version: Version::V1_4_0.to_string(),
            requested_codec: options.codec,
            requested_extent: (requested_extent.width, requested_extent.height),
            selected_physical_device_index: selection.physical_device_index,
            selected_physical_device_name: selection
                .properties
                .device_name
                .to_string_lossy()
                .into_owned(),
            selected_physical_device_type: format!("{:?}", selection.properties.device_type),
            vendor_id: selection.properties.vendor_id,
            device_id: selection.properties.device_id,
            api_version: Version::from(selection.properties.api_version).to_string(),
            driver_version: selection.properties.driver_version,
            selected_queue_family_index: selection.queue_family_index,
            selected_queue_count: selection.queue_count,
            selected_queue_flags: queue_flag_labels(selection.queue_flags),
            enabled_device_extensions,
            synchronization2_enabled: feature_selection.synchronization2_enabled,
            dynamic_rendering_enabled: feature_selection.dynamic_rendering_enabled,
            sampler_ycbcr_conversion_enabled: feature_selection.sampler_ycbcr_conversion_enabled,
            video_maintenance1_enabled: feature_selection.video_maintenance1_enabled,
            video_maintenance2_enabled: feature_selection.video_maintenance2_enabled,
            inline_session_parameters_enabled: feature_selection.inline_session_parameters_enabled,
            inline_session_parameter_codecs: feature_selection.inline_session_parameter_codecs(),
            ffmpeg_submit_model: "references/ffmpeg/libavutil/vulkan.c: VkSubmitInfo2 + QueueSubmit2",
            video_codec_operation: video_codec_operation_labels(
                vulkanalia_video_session_codec_operation(options.codec),
            ),
            profile: effective_profile_label,
            format_probe_profile:
                native_vulkan_vulkanalia_video_session_effective_format_probe_profile(
                    options.codec,
                    options.h264_parameter_sets.as_ref(),
                    options.av1_sequence_header.as_ref(),
                )?,
            picture_format: format!("{picture_format:?}"),
            reference_picture_format: format!("{picture_format:?}"),
            target_picture_dpb_supported,
            target_picture_sampled_output_supported,
            target_resource_plan,
            capability_flags: video_capability_flag_labels(capabilities.flags),
            decode_capability_flags: video_decode_capability_flag_labels(
                queried.decode_capability_flags,
            ),
            min_bitstream_buffer_offset_alignment: capabilities
                .min_bitstream_buffer_offset_alignment,
            min_bitstream_buffer_size_alignment: capabilities.min_bitstream_buffer_size_alignment,
            picture_access_granularity: (
                capabilities.picture_access_granularity.width,
                capabilities.picture_access_granularity.height,
            ),
            min_coded_extent: (
                capabilities.min_coded_extent.width,
                capabilities.min_coded_extent.height,
            ),
            max_coded_extent: (
                capabilities.max_coded_extent.width,
                capabilities.max_coded_extent.height,
            ),
            requested_extent_supported,
            driver_max_dpb_slots: capabilities.max_dpb_slots,
            driver_max_active_reference_pictures: capabilities.max_active_reference_pictures,
            session_max_dpb_slots,
            session_max_active_reference_pictures,
            codec_max_level: queried.codec_max_level,
            codec_max_level_raw: queried.codec_max_level_raw,
            std_header_version_name: capabilities
                .std_header_version
                .extension_name
                .to_string_lossy()
                .into_owned(),
            std_header_version_spec_version: capabilities.std_header_version.spec_version,
            memory_binding,
            resource_image_requested: options.allocate_video_images,
            resource_image,
            bitstream_buffer_requested: options.allocate_bitstream_buffer,
            bitstream_buffer,
            session_parameters_requested: options.create_empty_session_parameters
                || options.create_session_parameters
                || options.h264_ready_prefix_decode.is_some()
                || options.h265_ready_prefix_decode.is_some()
                || options.av1_ready_prefix_decode.is_some(),
            session_parameters,
            h264_ready_prefix_decode_requested: options.h264_ready_prefix_decode.is_some(),
            h264_ready_prefix_decode,
            h265_ready_prefix_decode_requested: options.h265_ready_prefix_decode.is_some(),
            h265_ready_prefix_decode,
            av1_ready_prefix_decode_requested: options.av1_ready_prefix_decode.is_some(),
            av1_ready_prefix_decode,
        })
    })();

    if let Some(resources) = memory_resources.take() {
        native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources(device, resources);
    }
    native_vulkan_vulkanalia_destroy_video_session(device, session);

    result
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_record_h265_ready_prefix_decode_smoke(
    instance: &Instance,
    device: &Device,
    queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    selection: &NativeVulkanVulkanaliaVideoPhysicalDeviceSelection,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    picture_format: vk::Format,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    requested_bitstream_buffer_size: u64,
    input: &NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
) -> Result<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err("Vulkanalia H.265 ready-prefix decode smoke requires an H.265 codec".into());
    }
    let image = native_vulkan_vulkanalia_create_video_session_resource_image(
        instance,
        device,
        memory_properties,
        selection.physical_device,
        profile_info,
        extent,
        array_layers,
        picture_format,
        decode_capability_flags,
        &[selection.queue_family_index],
    )?;
    let mut image = Some(image);

    let result = native_vulkan_vulkanalia_record_h265_ready_prefix_decode_into_image(
        device,
        queue,
        memory_properties,
        selection.queue_family_index,
        profile_info,
        extent,
        capabilities,
        session,
        codec,
        array_layers,
        requested_bitstream_buffer_size,
        input,
        image
            .as_ref()
            .expect("Vulkanalia H.265 decode image is alive during smoke"),
        None,
    );

    if let Some(image) = image.take() {
        native_vulkan_vulkanalia_destroy_video_session_resource_image(device, image);
    }

    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_record_h265_ready_prefix_decode_into_image(
    device: &Device,
    queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    queue_family_index: u32,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    requested_bitstream_buffer_size: u64,
    input: &NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    mut after_frame_submitted: Option<&mut dyn FnMut(u32, u32) -> Result<(), String>>,
) -> Result<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10
    ) {
        return Err("Vulkanalia H.265 ready-prefix decode smoke requires an H.265 codec".into());
    }
    if input.requested_frame_count == 0 {
        return Err(
            "Vulkanalia H.265 ready-prefix decode smoke requires at least one frame".to_owned(),
        );
    }
    if input.frames.len() < input.requested_frame_count as usize {
        return Err(format!(
            "Vulkanalia H.265 ready-prefix input has {} frames but {} were requested",
            input.frames.len(),
            input.requested_frame_count
        ));
    }
    let frames = &input.frames[..input.requested_frame_count as usize];
    for frame in frames {
        if frame.entry.planned_output_slot >= array_layers {
            return Err(format!(
                "Vulkanalia H.265 ready-prefix planned output slot {} exceeds image layers {array_layers}",
                frame.entry.planned_output_slot
            ));
        }
        for reference in &frame.entry.references {
            if let Some(dpb_slot) = reference.dpb_slot {
                if dpb_slot >= array_layers {
                    return Err(format!(
                        "Vulkanalia H.265 ready-prefix reference slot {dpb_slot} exceeds image layers {array_layers}"
                    ));
                }
            }
        }
    }

    let (bitstream_payload, frame_bitstreams) = native_vulkan_vulkanalia_h265_decode_payloads(
        frames,
        capabilities.min_bitstream_buffer_offset_alignment,
        capabilities.min_bitstream_buffer_size_alignment,
    )?;
    let bitstream_buffer_size = requested_bitstream_buffer_size.max(bitstream_payload.len() as u64);
    let bitstream_buffer = native_vulkan_vulkanalia_create_video_session_bitstream_buffer(
        device,
        memory_properties,
        profile_info,
        bitstream_buffer_size,
        capabilities.min_bitstream_buffer_size_alignment,
        Some(&bitstream_payload),
        false,
    )?;
    let mut bitstream_buffer = Some(bitstream_buffer);
    let session_parameters = native_vulkan_vulkanalia_create_h265_video_session_parameters(
        device,
        session,
        codec,
        &input.parameter_sets,
    )?;
    let mut session_parameters = Some(session_parameters);
    let command_buffer =
        native_vulkan_vulkanalia_create_decode_command_buffer(device, queue_family_index)?;
    let mut command_buffer = Some(command_buffer);

    let result =
        (|| -> Result<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot, String> {
            let parameter_ids =
                NativeVulkanVulkanaliaH265ParameterIds::from_parameter_sets(&input.parameter_sets)?;
            let image_ref = image;
            let session_parameters_ref = session_parameters
                .as_ref()
                .expect("Vulkanalia H.265 session parameters are alive during smoke");
            let bitstream_buffer_ref = bitstream_buffer
                .as_ref()
                .expect("Vulkanalia bitstream buffer is alive during smoke");
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia command buffer is alive during smoke");
            let mut initialized_slots = vec![false; array_layers as usize];
            let mut frame_snapshots = Vec::with_capacity(frames.len());
            let mut command_buffer_recorded = true;
            let mut submitted = true;
            let mut uses_synchronization2 = true;
            let mut uses_submit2 = true;
            let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_decode.c";

            for (frame_index, (frame, frame_bitstream)) in
                frames.iter().zip(frame_bitstreams.iter()).enumerate()
            {
                let frame_index_u32 = u32::try_from(frame_index)
                    .map_err(|_| "Vulkanalia H.265 frame index exceeds u32".to_owned())?;
                let reset_control_recorded = frame.first_slice.idr || frame.first_slice.irap;
                let plan = native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan(
                    extent,
                    parameter_ids,
                    &frame.entry,
                    &frame.first_slice,
                    frame_bitstream.src_buffer_offset,
                    frame_bitstream.src_buffer_range,
                    vec![frame.slice_segment_offset],
                    reset_control_recorded,
                )?;
                ffmpeg_reference = plan.common.ffmpeg_reference;
                let image_views =
                    native_vulkan_vulkanalia_h265_decode_image_view_bindings(image_ref, &plan)?;
                let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
                let transition_dst_from_undefined = !initialized_slots[dst_slot];
                let record_plan = unsafe {
                    native_vulkan_vulkanalia_record_h265_decode_command_buffer(
                        device,
                        command_buffer_ref.command_buffer,
                        image_ref.image,
                        &plan,
                        session,
                        session_parameters_ref.parameters,
                        bitstream_buffer_ref.buffer,
                        &image_views,
                        frame_index > 0,
                        transition_dst_from_undefined,
                    )
                }?;
                let submit_plan = unsafe {
                    native_vulkan_vulkanalia_submit_decode_command_buffer2(
                        device,
                        queue,
                        command_buffer_ref.command_buffer,
                        vk::Fence::default(),
                        true,
                    )
                }?;
                initialized_slots[dst_slot] = true;
                command_buffer_recorded &=
                    record_plan.command_order.contains(&"vkEndCommandBuffer");
                submitted &= submit_plan.command_order.contains(&"queue_submit2");
                uses_synchronization2 &= record_plan.uses_synchronization2;
                uses_submit2 &= submit_plan.uses_submit2;

                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    after_frame_submitted(
                        frame_index_u32,
                        plan.common.dst_picture_resource.base_array_layer,
                    )?;
                }

                frame_snapshots.push(NativeVulkanVulkanaliaH265ReadyPrefixCommandFrameSnapshot {
                    frame_index: frame_index_u32,
                    access_unit_index: frame.entry.access_unit_index,
                    src_buffer_offset: plan.common.src_buffer_offset,
                    src_buffer_range: plan.common.src_buffer_range,
                    dst_base_array_layer: plan.common.dst_picture_resource.base_array_layer,
                    setup_slot_index: plan.common.setup_reference_slot.slot_index,
                    begin_reference_slot_count: plan.common.begin_reference_slots.len() as u32,
                    decode_reference_slot_count: plan.common.decode_reference_slots.len() as u32,
                    reset_control_recorded,
                    slice_segment_count: plan.picture.slice_segment_offsets.len() as u32,
                    slice_segment_offsets: plan.picture.slice_segment_offsets,
                });
            }
            let last_frame = frame_snapshots
                .last()
                .cloned()
                .ok_or_else(|| "Vulkanalia H.265 submitted no ready-prefix frames".to_owned())?;

            Ok(NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot {
                requested_frame_count: input.requested_frame_count,
                recorded_frame_count: frame_snapshots.len() as u32,
                submitted_frame_count: frame_snapshots.len() as u32,
                ffmpeg_reference,
                command_buffer_recorded,
                submitted,
                uses_synchronization2,
                uses_submit2,
                queue_family_index,
                src_buffer_total_bytes: bitstream_payload.len() as u64,
                src_buffer_offset: last_frame.src_buffer_offset,
                src_buffer_range: last_frame.src_buffer_range,
                dst_base_array_layer: last_frame.dst_base_array_layer,
                setup_slot_index: last_frame.setup_slot_index,
                begin_reference_slot_count: last_frame.begin_reference_slot_count,
                decode_reference_slot_count: last_frame.decode_reference_slot_count,
                reset_control_recorded: last_frame.reset_control_recorded,
                slice_segment_count: last_frame.slice_segment_count,
                slice_segment_offsets: last_frame.slice_segment_offsets.clone(),
                frames: frame_snapshots,
            })
        })();

    if let Some(command_buffer) = command_buffer.take() {
        native_vulkan_vulkanalia_destroy_decode_command_buffer(device, command_buffer);
    }
    if let Some(session_parameters) = session_parameters.take() {
        native_vulkan_vulkanalia_destroy_video_session_parameters(device, session_parameters);
    }
    if let Some(bitstream_buffer) = bitstream_buffer.take() {
        native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer(device, bitstream_buffer);
    }

    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_record_h264_ready_prefix_decode_into_image(
    device: &Device,
    queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    queue_family_index: u32,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    requested_bitstream_buffer_size: u64,
    input: &NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput,
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    mut after_frame_submitted: Option<&mut dyn FnMut(u32, u32) -> Result<(), String>>,
) -> Result<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot, String> {
    if codec != NativeVulkanVideoSessionCodec::H264High8 {
        return Err("Vulkanalia H.264 ready-prefix decode smoke requires H.264 high-8".into());
    }
    if input.requested_frame_count == 0 {
        return Err(
            "Vulkanalia H.264 ready-prefix decode smoke requires at least one frame".to_owned(),
        );
    }
    if input.frames.len() < input.requested_frame_count as usize {
        return Err(format!(
            "Vulkanalia H.264 ready-prefix input has {} frames but {} were requested",
            input.frames.len(),
            input.requested_frame_count
        ));
    }
    let frames = &input.frames[..input.requested_frame_count as usize];
    for frame in frames {
        if frame.entry.planned_output_slot >= array_layers {
            return Err(format!(
                "Vulkanalia H.264 ready-prefix planned output slot {} exceeds image layers {array_layers}",
                frame.entry.planned_output_slot
            ));
        }
        for reference in &frame.entry.references {
            if let Some(dpb_slot) = reference.dpb_slot
                && dpb_slot >= array_layers
            {
                return Err(format!(
                    "Vulkanalia H.264 ready-prefix reference slot {dpb_slot} exceeds image layers {array_layers}"
                ));
            }
        }
    }

    let (bitstream_payload, frame_bitstreams) = native_vulkan_vulkanalia_h264_decode_payloads(
        frames,
        capabilities.min_bitstream_buffer_offset_alignment,
        capabilities.min_bitstream_buffer_size_alignment,
    )?;
    let bitstream_buffer_size = requested_bitstream_buffer_size.max(bitstream_payload.len() as u64);
    let bitstream_buffer = native_vulkan_vulkanalia_create_video_session_bitstream_buffer(
        device,
        memory_properties,
        profile_info,
        bitstream_buffer_size,
        capabilities.min_bitstream_buffer_size_alignment,
        Some(&bitstream_payload),
        false,
    )?;
    let mut bitstream_buffer = Some(bitstream_buffer);
    let session_parameters = native_vulkan_vulkanalia_create_h264_video_session_parameters(
        device,
        session,
        codec,
        &input.parameter_sets,
    )?;
    let mut session_parameters = Some(session_parameters);
    let command_buffer =
        native_vulkan_vulkanalia_create_decode_command_buffer(device, queue_family_index)?;
    let mut command_buffer = Some(command_buffer);

    let result =
        (|| -> Result<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot, String> {
            let parameter_ids =
                NativeVulkanVulkanaliaH264ParameterIds::from_parameter_sets(&input.parameter_sets)?;
            let session_parameters_ref = session_parameters
                .as_ref()
                .expect("Vulkanalia H.264 session parameters are alive during smoke");
            let bitstream_buffer_ref = bitstream_buffer
                .as_ref()
                .expect("Vulkanalia bitstream buffer is alive during smoke");
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia command buffer is alive during smoke");
            let mut initialized_slots = vec![false; array_layers as usize];
            let mut frame_snapshots = Vec::with_capacity(frames.len());
            let mut command_buffer_recorded = true;
            let mut submitted = true;
            let mut uses_synchronization2 = true;
            let mut uses_submit2 = true;
            let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_decode.c";

            for (frame_index, (frame, frame_bitstream)) in
                frames.iter().zip(frame_bitstreams.iter()).enumerate()
            {
                let frame_index_u32 = u32::try_from(frame_index)
                    .map_err(|_| "Vulkanalia H.264 frame index exceeds u32".to_owned())?;
                let reset_control_recorded = frame.first_slice.idr;
                let plan = native_vulkan_vulkanalia_h264_ready_prefix_decode_submit_plan(
                    extent,
                    parameter_ids,
                    &frame.entry,
                    &frame.first_slice,
                    frame_bitstream.src_buffer_offset,
                    frame_bitstream.src_buffer_range,
                    frame.slice_offsets.clone(),
                    reset_control_recorded,
                )?;
                ffmpeg_reference = plan.common.ffmpeg_reference;
                let image_views =
                    native_vulkan_vulkanalia_h264_decode_image_view_bindings(image, &plan)?;
                let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
                let transition_dst_from_undefined = !initialized_slots[dst_slot];
                let record_plan = unsafe {
                    native_vulkan_vulkanalia_record_h264_decode_command_buffer(
                        device,
                        command_buffer_ref.command_buffer,
                        image.image,
                        &plan,
                        session,
                        session_parameters_ref.parameters,
                        bitstream_buffer_ref.buffer,
                        &image_views,
                        frame_index > 0,
                        transition_dst_from_undefined,
                    )
                }?;
                let submit_plan = unsafe {
                    native_vulkan_vulkanalia_submit_decode_command_buffer2(
                        device,
                        queue,
                        command_buffer_ref.command_buffer,
                        vk::Fence::default(),
                        true,
                    )
                }?;
                initialized_slots[dst_slot] = true;
                command_buffer_recorded &=
                    record_plan.command_order.contains(&"vkEndCommandBuffer");
                submitted &= submit_plan.command_order.contains(&"queue_submit2");
                uses_synchronization2 &= record_plan.uses_synchronization2;
                uses_submit2 &= submit_plan.uses_submit2;

                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    after_frame_submitted(
                        frame_index_u32,
                        plan.common.dst_picture_resource.base_array_layer,
                    )?;
                }

                frame_snapshots.push(NativeVulkanVulkanaliaH264ReadyPrefixCommandFrameSnapshot {
                    frame_index: frame_index_u32,
                    access_unit_index: frame.entry.access_unit_index,
                    src_buffer_offset: plan.common.src_buffer_offset,
                    src_buffer_range: plan.common.src_buffer_range,
                    dst_base_array_layer: plan.common.dst_picture_resource.base_array_layer,
                    setup_slot_index: plan.common.setup_reference_slot.slot_index,
                    begin_reference_slot_count: plan.common.begin_reference_slots.len() as u32,
                    decode_reference_slot_count: plan.common.decode_reference_slots.len() as u32,
                    reset_control_recorded,
                    slice_segment_count: plan.picture.slice_offsets.len() as u32,
                    slice_segment_offsets: plan.picture.slice_offsets,
                });
            }
            let last_frame = frame_snapshots
                .last()
                .cloned()
                .ok_or_else(|| "Vulkanalia H.264 submitted no ready-prefix frames".to_owned())?;

            Ok(NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot {
                requested_frame_count: input.requested_frame_count,
                recorded_frame_count: frame_snapshots.len() as u32,
                submitted_frame_count: frame_snapshots.len() as u32,
                ffmpeg_reference,
                command_buffer_recorded,
                submitted,
                uses_synchronization2,
                uses_submit2,
                queue_family_index,
                src_buffer_total_bytes: bitstream_payload.len() as u64,
                src_buffer_offset: last_frame.src_buffer_offset,
                src_buffer_range: last_frame.src_buffer_range,
                dst_base_array_layer: last_frame.dst_base_array_layer,
                setup_slot_index: last_frame.setup_slot_index,
                begin_reference_slot_count: last_frame.begin_reference_slot_count,
                decode_reference_slot_count: last_frame.decode_reference_slot_count,
                reset_control_recorded: last_frame.reset_control_recorded,
                slice_segment_count: last_frame.slice_segment_count,
                slice_segment_offsets: last_frame.slice_segment_offsets.clone(),
                frames: frame_snapshots,
            })
        })();

    if let Some(command_buffer) = command_buffer.take() {
        native_vulkan_vulkanalia_destroy_decode_command_buffer(device, command_buffer);
    }
    if let Some(session_parameters) = session_parameters.take() {
        native_vulkan_vulkanalia_destroy_video_session_parameters(device, session_parameters);
    }
    if let Some(bitstream_buffer) = bitstream_buffer.take() {
        native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer(device, bitstream_buffer);
    }

    result
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_record_h264_ready_prefix_decode_smoke(
    instance: &Instance,
    device: &Device,
    queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    selection: &NativeVulkanVulkanaliaVideoPhysicalDeviceSelection,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    picture_format: vk::Format,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    requested_bitstream_buffer_size: u64,
    input: &NativeVulkanVulkanaliaH264ReadyPrefixDecodeInput,
) -> Result<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot, String> {
    if codec != NativeVulkanVideoSessionCodec::H264High8 {
        return Err("Vulkanalia H.264 ready-prefix decode smoke requires H.264 high-8".into());
    }
    if input.requested_frame_count == 0 {
        return Err(
            "Vulkanalia H.264 ready-prefix decode smoke requires at least one frame".to_owned(),
        );
    }
    if input.frames.len() < input.requested_frame_count as usize {
        return Err(format!(
            "Vulkanalia H.264 ready-prefix input has {} frames but {} were requested",
            input.frames.len(),
            input.requested_frame_count
        ));
    }
    let frames = &input.frames[..input.requested_frame_count as usize];
    for frame in frames {
        if frame.entry.planned_output_slot >= array_layers {
            return Err(format!(
                "Vulkanalia H.264 ready-prefix planned output slot {} exceeds image layers {array_layers}",
                frame.entry.planned_output_slot
            ));
        }
        for reference in &frame.entry.references {
            if let Some(dpb_slot) = reference.dpb_slot
                && dpb_slot >= array_layers
            {
                return Err(format!(
                    "Vulkanalia H.264 ready-prefix reference slot {dpb_slot} exceeds image layers {array_layers}"
                ));
            }
        }
    }

    let (bitstream_payload, frame_bitstreams) = native_vulkan_vulkanalia_h264_decode_payloads(
        frames,
        capabilities.min_bitstream_buffer_offset_alignment,
        capabilities.min_bitstream_buffer_size_alignment,
    )?;
    let bitstream_buffer_size = requested_bitstream_buffer_size.max(bitstream_payload.len() as u64);
    let image = native_vulkan_vulkanalia_create_video_session_resource_image(
        instance,
        device,
        memory_properties,
        selection.physical_device,
        profile_info,
        extent,
        array_layers,
        picture_format,
        decode_capability_flags,
        &[selection.queue_family_index],
    )?;
    let mut image = Some(image);
    let bitstream_buffer = native_vulkan_vulkanalia_create_video_session_bitstream_buffer(
        device,
        memory_properties,
        profile_info,
        bitstream_buffer_size,
        capabilities.min_bitstream_buffer_size_alignment,
        Some(&bitstream_payload),
        false,
    )?;
    let mut bitstream_buffer = Some(bitstream_buffer);
    let session_parameters = native_vulkan_vulkanalia_create_h264_video_session_parameters(
        device,
        session,
        codec,
        &input.parameter_sets,
    )?;
    let mut session_parameters = Some(session_parameters);
    let command_buffer = native_vulkan_vulkanalia_create_decode_command_buffer(
        device,
        selection.queue_family_index,
    )?;
    let mut command_buffer = Some(command_buffer);

    let result =
        (|| -> Result<NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot, String> {
            let parameter_ids =
                NativeVulkanVulkanaliaH264ParameterIds::from_parameter_sets(&input.parameter_sets)?;
            let image_ref = image
                .as_ref()
                .expect("Vulkanalia H.264 decode image is alive during smoke");
            let session_parameters_ref = session_parameters
                .as_ref()
                .expect("Vulkanalia H.264 session parameters are alive during smoke");
            let bitstream_buffer_ref = bitstream_buffer
                .as_ref()
                .expect("Vulkanalia bitstream buffer is alive during smoke");
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia command buffer is alive during smoke");
            let mut initialized_slots = vec![false; array_layers as usize];
            let mut frame_snapshots = Vec::with_capacity(frames.len());
            let mut command_buffer_recorded = true;
            let mut submitted = true;
            let mut uses_synchronization2 = true;
            let mut uses_submit2 = true;
            let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_decode.c";

            for (frame_index, (frame, frame_bitstream)) in
                frames.iter().zip(frame_bitstreams.iter()).enumerate()
            {
                let reset_control_recorded = frame.first_slice.idr;
                let plan = native_vulkan_vulkanalia_h264_ready_prefix_decode_submit_plan(
                    extent,
                    parameter_ids,
                    &frame.entry,
                    &frame.first_slice,
                    frame_bitstream.src_buffer_offset,
                    frame_bitstream.src_buffer_range,
                    frame.slice_offsets.clone(),
                    reset_control_recorded,
                )?;
                ffmpeg_reference = plan.common.ffmpeg_reference;
                let image_views =
                    native_vulkan_vulkanalia_h264_decode_image_view_bindings(image_ref, &plan)?;
                let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
                let transition_dst_from_undefined = !initialized_slots[dst_slot];
                let record_plan = unsafe {
                    native_vulkan_vulkanalia_record_h264_decode_command_buffer(
                        device,
                        command_buffer_ref.command_buffer,
                        image_ref.image,
                        &plan,
                        session,
                        session_parameters_ref.parameters,
                        bitstream_buffer_ref.buffer,
                        &image_views,
                        frame_index > 0,
                        transition_dst_from_undefined,
                    )
                }?;
                let submit_plan = unsafe {
                    native_vulkan_vulkanalia_submit_decode_command_buffer2(
                        device,
                        queue,
                        command_buffer_ref.command_buffer,
                        vk::Fence::default(),
                        true,
                    )
                }?;
                initialized_slots[dst_slot] = true;
                command_buffer_recorded &=
                    record_plan.command_order.contains(&"vkEndCommandBuffer");
                submitted &= submit_plan.command_order.contains(&"queue_submit2");
                uses_synchronization2 &= record_plan.uses_synchronization2;
                uses_submit2 &= submit_plan.uses_submit2;

                frame_snapshots.push(NativeVulkanVulkanaliaH264ReadyPrefixCommandFrameSnapshot {
                    frame_index: u32::try_from(frame_index)
                        .map_err(|_| "Vulkanalia H.264 frame index exceeds u32".to_owned())?,
                    access_unit_index: frame.entry.access_unit_index,
                    src_buffer_offset: plan.common.src_buffer_offset,
                    src_buffer_range: plan.common.src_buffer_range,
                    dst_base_array_layer: plan.common.dst_picture_resource.base_array_layer,
                    setup_slot_index: plan.common.setup_reference_slot.slot_index,
                    begin_reference_slot_count: plan.common.begin_reference_slots.len() as u32,
                    decode_reference_slot_count: plan.common.decode_reference_slots.len() as u32,
                    reset_control_recorded,
                    slice_segment_count: plan.picture.slice_offsets.len() as u32,
                    slice_segment_offsets: plan.picture.slice_offsets,
                });
            }
            let last_frame = frame_snapshots
                .last()
                .cloned()
                .ok_or_else(|| "Vulkanalia H.264 submitted no ready-prefix frames".to_owned())?;

            Ok(NativeVulkanVulkanaliaH264ReadyPrefixCommandSmokeSnapshot {
                requested_frame_count: input.requested_frame_count,
                recorded_frame_count: frame_snapshots.len() as u32,
                submitted_frame_count: frame_snapshots.len() as u32,
                ffmpeg_reference,
                command_buffer_recorded,
                submitted,
                uses_synchronization2,
                uses_submit2,
                queue_family_index: selection.queue_family_index,
                src_buffer_total_bytes: bitstream_payload.len() as u64,
                src_buffer_offset: last_frame.src_buffer_offset,
                src_buffer_range: last_frame.src_buffer_range,
                dst_base_array_layer: last_frame.dst_base_array_layer,
                setup_slot_index: last_frame.setup_slot_index,
                begin_reference_slot_count: last_frame.begin_reference_slot_count,
                decode_reference_slot_count: last_frame.decode_reference_slot_count,
                reset_control_recorded: last_frame.reset_control_recorded,
                slice_segment_count: last_frame.slice_segment_count,
                slice_segment_offsets: last_frame.slice_segment_offsets.clone(),
                frames: frame_snapshots,
            })
        })();

    if let Some(command_buffer) = command_buffer.take() {
        native_vulkan_vulkanalia_destroy_decode_command_buffer(device, command_buffer);
    }
    if let Some(session_parameters) = session_parameters.take() {
        native_vulkan_vulkanalia_destroy_video_session_parameters(device, session_parameters);
    }
    if let Some(bitstream_buffer) = bitstream_buffer.take() {
        native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer(device, bitstream_buffer);
    }
    if let Some(image) = image.take() {
        native_vulkan_vulkanalia_destroy_video_session_resource_image(device, image);
    }

    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_record_av1_ready_prefix_decode_into_image(
    device: &Device,
    queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    queue_family_index: u32,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    requested_bitstream_buffer_size: u64,
    input: &NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    mut after_frame_submitted: Option<&mut dyn FnMut(u32, u32) -> Result<(), String>>,
) -> Result<NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err("Vulkanalia AV1 ready-prefix decode smoke requires an AV1 codec".into());
    }
    if input.codec != codec {
        return Err(format!(
            "Vulkanalia AV1 ready-prefix input codec {} does not match session codec {}",
            input.codec.label(),
            codec.label()
        ));
    }
    if input.requested_frame_count == 0 {
        return Err(
            "Vulkanalia AV1 ready-prefix decode smoke requires at least one frame".to_owned(),
        );
    }
    if input.frames.len() < input.requested_frame_count as usize {
        return Err(format!(
            "Vulkanalia AV1 ready-prefix input has {} frames but {} were requested",
            input.frames.len(),
            input.requested_frame_count
        ));
    }
    let frames = &input.frames[..input.requested_frame_count as usize];
    for frame in frames {
        let output_slot = frame.entry.output_slot.ok_or_else(|| {
            format!(
                "Vulkanalia AV1 ready-prefix TU {} has no planned output slot",
                frame.entry.temporal_unit_index
            )
        })?;
        if output_slot >= array_layers {
            return Err(format!(
                "Vulkanalia AV1 ready-prefix planned output slot {output_slot} exceeds image layers {array_layers}"
            ));
        }
        for dpb_slot in frame
            .entry
            .decode_reference_slots
            .iter()
            .filter_map(|slot| u32::try_from(*slot).ok())
        {
            if dpb_slot >= array_layers {
                return Err(format!(
                    "Vulkanalia AV1 ready-prefix reference slot {dpb_slot} exceeds image layers {array_layers}"
                ));
            }
        }
    }

    let (bitstream_payload, frame_bitstreams) = native_vulkan_vulkanalia_av1_decode_payloads(
        frames,
        capabilities.min_bitstream_buffer_offset_alignment,
        capabilities.min_bitstream_buffer_size_alignment,
    )?;
    let bitstream_buffer_size = requested_bitstream_buffer_size.max(bitstream_payload.len() as u64);
    let bitstream_buffer = native_vulkan_vulkanalia_create_video_session_bitstream_buffer(
        device,
        memory_properties,
        profile_info,
        bitstream_buffer_size,
        capabilities.min_bitstream_buffer_size_alignment,
        Some(&bitstream_payload),
        false,
    )?;
    let mut bitstream_buffer = Some(bitstream_buffer);
    let session_parameters = native_vulkan_vulkanalia_create_av1_video_session_parameters(
        device,
        session,
        codec,
        &input.sequence_header,
    )?;
    let mut session_parameters = Some(session_parameters);
    let command_buffer =
        native_vulkan_vulkanalia_create_decode_command_buffer(device, queue_family_index)?;
    let mut command_buffer = Some(command_buffer);

    let result =
        (|| -> Result<NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot, String> {
            let session_parameters_ref = session_parameters
                .as_ref()
                .expect("Vulkanalia AV1 session parameters are alive during smoke");
            let bitstream_buffer_ref = bitstream_buffer
                .as_ref()
                .expect("Vulkanalia AV1 bitstream buffer is alive during smoke");
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia AV1 command buffer is alive during smoke");
            let mut initialized_slots = vec![false; array_layers as usize];
            let mut frame_snapshots = Vec::with_capacity(frames.len());
            let mut command_buffer_recorded = true;
            let mut submitted = true;
            let mut uses_synchronization2 = true;
            let mut uses_submit2 = true;
            let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_av1.c";

            for (frame_index, (frame, frame_bitstream)) in
                frames.iter().zip(frame_bitstreams.iter()).enumerate()
            {
                let frame_index_u32 = u32::try_from(frame_index)
                    .map_err(|_| "Vulkanalia AV1 frame index exceeds u32".to_owned())?;
                let reset_control_recorded = frame_index == 0 || frame.frame.frame_type == 0;
                let plan = native_vulkan_vulkanalia_av1_ready_prefix_decode_submit_plan(
                    extent,
                    codec,
                    &frame.entry,
                    &frame.frame,
                    frame_bitstream.src_buffer_offset,
                    frame_bitstream.src_buffer_range,
                    reset_control_recorded,
                )?;
                ffmpeg_reference = plan.picture.ffmpeg_reference;
                let image_views =
                    native_vulkan_vulkanalia_av1_decode_image_view_bindings(image, &plan)?;
                let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
                let transition_dst_from_undefined = !initialized_slots[dst_slot];
                let record_plan = unsafe {
                    native_vulkan_vulkanalia_record_av1_decode_command_buffer(
                        device,
                        command_buffer_ref.command_buffer,
                        image.image,
                        &plan,
                        session,
                        session_parameters_ref.parameters,
                        bitstream_buffer_ref.buffer,
                        &image_views,
                        frame_index > 0,
                        transition_dst_from_undefined,
                    )
                }?;
                let submit_plan = unsafe {
                    native_vulkan_vulkanalia_submit_decode_command_buffer2(
                        device,
                        queue,
                        command_buffer_ref.command_buffer,
                        vk::Fence::default(),
                        true,
                    )
                }?;
                initialized_slots[dst_slot] = true;
                command_buffer_recorded &=
                    record_plan.command_order.contains(&"vkEndCommandBuffer");
                submitted &= submit_plan.command_order.contains(&"queue_submit2");
                uses_synchronization2 &= record_plan.uses_synchronization2;
                uses_submit2 &= submit_plan.uses_submit2;

                if let Some(after_frame_submitted) = after_frame_submitted.as_deref_mut() {
                    after_frame_submitted(
                        frame_index_u32,
                        plan.common.dst_picture_resource.base_array_layer,
                    )?;
                }

                frame_snapshots.push(NativeVulkanVulkanaliaAv1ReadyPrefixCommandFrameSnapshot {
                    frame_index: frame_index_u32,
                    temporal_unit_index: frame.entry.temporal_unit_index,
                    src_buffer_offset: plan.common.src_buffer_offset,
                    src_buffer_range: plan.common.src_buffer_range,
                    dst_base_array_layer: plan.common.dst_picture_resource.base_array_layer,
                    setup_slot_index: plan.common.setup_reference_slot.slot_index,
                    begin_reference_slot_count: plan.common.begin_reference_slots.len() as u32,
                    decode_reference_slot_count: plan.common.decode_reference_slots.len() as u32,
                    reset_control_recorded,
                    tile_count: plan.picture.tile_offsets.len() as u32,
                    tile_offsets: plan.picture.tile_offsets,
                    tile_sizes: plan.picture.tile_sizes,
                });
            }
            let last_frame = frame_snapshots
                .last()
                .cloned()
                .ok_or_else(|| "Vulkanalia AV1 submitted no ready-prefix frames".to_owned())?;

            Ok(NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot {
                requested_frame_count: input.requested_frame_count,
                recorded_frame_count: frame_snapshots.len() as u32,
                submitted_frame_count: frame_snapshots.len() as u32,
                ffmpeg_reference,
                command_buffer_recorded,
                submitted,
                uses_synchronization2,
                uses_submit2,
                queue_family_index,
                src_buffer_total_bytes: bitstream_payload.len() as u64,
                src_buffer_offset: last_frame.src_buffer_offset,
                src_buffer_range: last_frame.src_buffer_range,
                dst_base_array_layer: last_frame.dst_base_array_layer,
                setup_slot_index: last_frame.setup_slot_index,
                begin_reference_slot_count: last_frame.begin_reference_slot_count,
                decode_reference_slot_count: last_frame.decode_reference_slot_count,
                reset_control_recorded: last_frame.reset_control_recorded,
                tile_count: last_frame.tile_count,
                tile_offsets: last_frame.tile_offsets.clone(),
                tile_sizes: last_frame.tile_sizes.clone(),
                frames: frame_snapshots,
            })
        })();

    if let Some(command_buffer) = command_buffer.take() {
        native_vulkan_vulkanalia_destroy_decode_command_buffer(device, command_buffer);
    }
    if let Some(session_parameters) = session_parameters.take() {
        native_vulkan_vulkanalia_destroy_video_session_parameters(device, session_parameters);
    }
    if let Some(bitstream_buffer) = bitstream_buffer.take() {
        native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer(device, bitstream_buffer);
    }

    result
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_record_av1_ready_prefix_decode_smoke(
    instance: &Instance,
    device: &Device,
    queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    selection: &NativeVulkanVulkanaliaVideoPhysicalDeviceSelection,
    profile_info: &vk::VideoProfileInfoKHR,
    extent: vk::Extent2D,
    picture_format: vk::Format,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    capabilities: vk::VideoCapabilitiesKHR,
    session: vk::VideoSessionKHR,
    codec: NativeVulkanVideoSessionCodec,
    array_layers: u32,
    requested_bitstream_buffer_size: u64,
    input: &NativeVulkanVulkanaliaAv1ReadyPrefixDecodeInput,
) -> Result<NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot, String> {
    if !matches!(
        codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) {
        return Err("Vulkanalia AV1 ready-prefix decode smoke requires an AV1 codec".into());
    }
    if input.codec != codec {
        return Err(format!(
            "Vulkanalia AV1 ready-prefix input codec {} does not match session codec {}",
            input.codec.label(),
            codec.label()
        ));
    }
    if input.requested_frame_count == 0 {
        return Err(
            "Vulkanalia AV1 ready-prefix decode smoke requires at least one frame".to_owned(),
        );
    }
    if input.frames.len() < input.requested_frame_count as usize {
        return Err(format!(
            "Vulkanalia AV1 ready-prefix input has {} frames but {} were requested",
            input.frames.len(),
            input.requested_frame_count
        ));
    }
    let frames = &input.frames[..input.requested_frame_count as usize];
    for frame in frames {
        let output_slot = frame.entry.output_slot.ok_or_else(|| {
            format!(
                "Vulkanalia AV1 ready-prefix TU {} has no planned output slot",
                frame.entry.temporal_unit_index
            )
        })?;
        if output_slot >= array_layers {
            return Err(format!(
                "Vulkanalia AV1 ready-prefix planned output slot {output_slot} exceeds image layers {array_layers}"
            ));
        }
        for dpb_slot in frame
            .entry
            .decode_reference_slots
            .iter()
            .filter_map(|slot| u32::try_from(*slot).ok())
        {
            if dpb_slot >= array_layers {
                return Err(format!(
                    "Vulkanalia AV1 ready-prefix reference slot {dpb_slot} exceeds image layers {array_layers}"
                ));
            }
        }
    }

    let (bitstream_payload, frame_bitstreams) = native_vulkan_vulkanalia_av1_decode_payloads(
        frames,
        capabilities.min_bitstream_buffer_offset_alignment,
        capabilities.min_bitstream_buffer_size_alignment,
    )?;
    let bitstream_buffer_size = requested_bitstream_buffer_size.max(bitstream_payload.len() as u64);
    let image = native_vulkan_vulkanalia_create_video_session_resource_image(
        instance,
        device,
        memory_properties,
        selection.physical_device,
        profile_info,
        extent,
        array_layers,
        picture_format,
        decode_capability_flags,
        &[selection.queue_family_index],
    )?;
    let mut image = Some(image);
    let bitstream_buffer = native_vulkan_vulkanalia_create_video_session_bitstream_buffer(
        device,
        memory_properties,
        profile_info,
        bitstream_buffer_size,
        capabilities.min_bitstream_buffer_size_alignment,
        Some(&bitstream_payload),
        false,
    )?;
    let mut bitstream_buffer = Some(bitstream_buffer);
    let session_parameters = native_vulkan_vulkanalia_create_av1_video_session_parameters(
        device,
        session,
        codec,
        &input.sequence_header,
    )?;
    let mut session_parameters = Some(session_parameters);
    let command_buffer = native_vulkan_vulkanalia_create_decode_command_buffer(
        device,
        selection.queue_family_index,
    )?;
    let mut command_buffer = Some(command_buffer);

    let result =
        (|| -> Result<NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot, String> {
            let image_ref = image
                .as_ref()
                .expect("Vulkanalia AV1 decode image is alive during smoke");
            let session_parameters_ref = session_parameters
                .as_ref()
                .expect("Vulkanalia AV1 session parameters are alive during smoke");
            let bitstream_buffer_ref = bitstream_buffer
                .as_ref()
                .expect("Vulkanalia AV1 bitstream buffer is alive during smoke");
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia AV1 command buffer is alive during smoke");
            let mut initialized_slots = vec![false; array_layers as usize];
            let mut frame_snapshots = Vec::with_capacity(frames.len());
            let mut command_buffer_recorded = true;
            let mut submitted = true;
            let mut uses_synchronization2 = true;
            let mut uses_submit2 = true;
            let mut ffmpeg_reference = "references/ffmpeg/libavcodec/vulkan_av1.c";

            for (frame_index, (frame, frame_bitstream)) in
                frames.iter().zip(frame_bitstreams.iter()).enumerate()
            {
                let reset_control_recorded = frame_index == 0 || frame.frame.frame_type == 0;
                let plan = native_vulkan_vulkanalia_av1_ready_prefix_decode_submit_plan(
                    extent,
                    codec,
                    &frame.entry,
                    &frame.frame,
                    frame_bitstream.src_buffer_offset,
                    frame_bitstream.src_buffer_range,
                    reset_control_recorded,
                )?;
                ffmpeg_reference = plan.picture.ffmpeg_reference;
                let image_views =
                    native_vulkan_vulkanalia_av1_decode_image_view_bindings(image_ref, &plan)?;
                let dst_slot = plan.common.dst_picture_resource.base_array_layer as usize;
                let transition_dst_from_undefined = !initialized_slots[dst_slot];
                let record_plan = unsafe {
                    native_vulkan_vulkanalia_record_av1_decode_command_buffer(
                        device,
                        command_buffer_ref.command_buffer,
                        image_ref.image,
                        &plan,
                        session,
                        session_parameters_ref.parameters,
                        bitstream_buffer_ref.buffer,
                        &image_views,
                        frame_index > 0,
                        transition_dst_from_undefined,
                    )
                }?;
                let submit_plan = unsafe {
                    native_vulkan_vulkanalia_submit_decode_command_buffer2(
                        device,
                        queue,
                        command_buffer_ref.command_buffer,
                        vk::Fence::default(),
                        true,
                    )
                }?;
                initialized_slots[dst_slot] = true;
                command_buffer_recorded &=
                    record_plan.command_order.contains(&"vkEndCommandBuffer");
                submitted &= submit_plan.command_order.contains(&"queue_submit2");
                uses_synchronization2 &= record_plan.uses_synchronization2;
                uses_submit2 &= submit_plan.uses_submit2;

                frame_snapshots.push(NativeVulkanVulkanaliaAv1ReadyPrefixCommandFrameSnapshot {
                    frame_index: u32::try_from(frame_index)
                        .map_err(|_| "Vulkanalia AV1 frame index exceeds u32".to_owned())?,
                    temporal_unit_index: frame.entry.temporal_unit_index,
                    src_buffer_offset: plan.common.src_buffer_offset,
                    src_buffer_range: plan.common.src_buffer_range,
                    dst_base_array_layer: plan.common.dst_picture_resource.base_array_layer,
                    setup_slot_index: plan.common.setup_reference_slot.slot_index,
                    begin_reference_slot_count: plan.common.begin_reference_slots.len() as u32,
                    decode_reference_slot_count: plan.common.decode_reference_slots.len() as u32,
                    reset_control_recorded,
                    tile_count: plan.picture.tile_offsets.len() as u32,
                    tile_offsets: plan.picture.tile_offsets,
                    tile_sizes: plan.picture.tile_sizes,
                });
            }
            let last_frame = frame_snapshots
                .last()
                .cloned()
                .ok_or_else(|| "Vulkanalia AV1 submitted no ready-prefix frames".to_owned())?;

            Ok(NativeVulkanVulkanaliaAv1ReadyPrefixCommandSmokeSnapshot {
                requested_frame_count: input.requested_frame_count,
                recorded_frame_count: frame_snapshots.len() as u32,
                submitted_frame_count: frame_snapshots.len() as u32,
                ffmpeg_reference,
                command_buffer_recorded,
                submitted,
                uses_synchronization2,
                uses_submit2,
                queue_family_index: selection.queue_family_index,
                src_buffer_total_bytes: bitstream_payload.len() as u64,
                src_buffer_offset: last_frame.src_buffer_offset,
                src_buffer_range: last_frame.src_buffer_range,
                dst_base_array_layer: last_frame.dst_base_array_layer,
                setup_slot_index: last_frame.setup_slot_index,
                begin_reference_slot_count: last_frame.begin_reference_slot_count,
                decode_reference_slot_count: last_frame.decode_reference_slot_count,
                reset_control_recorded: last_frame.reset_control_recorded,
                tile_count: last_frame.tile_count,
                tile_offsets: last_frame.tile_offsets.clone(),
                tile_sizes: last_frame.tile_sizes.clone(),
                frames: frame_snapshots,
            })
        })();

    if let Some(command_buffer) = command_buffer.take() {
        native_vulkan_vulkanalia_destroy_decode_command_buffer(device, command_buffer);
    }
    if let Some(session_parameters) = session_parameters.take() {
        native_vulkan_vulkanalia_destroy_video_session_parameters(device, session_parameters);
    }
    if let Some(bitstream_buffer) = bitstream_buffer.take() {
        native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer(device, bitstream_buffer);
    }
    if let Some(image) = image.take() {
        native_vulkan_vulkanalia_destroy_video_session_resource_image(device, image);
    }

    result
}

fn native_vulkan_vulkanalia_h264_decode_image_view_bindings(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    plan: &super::video_decode_submit_h264::NativeVulkanVulkanaliaH264DecodeSubmitPlan,
) -> Result<NativeVulkanVulkanaliaDecodeImageViewBindings, String> {
    Ok(NativeVulkanVulkanaliaDecodeImageViewBindings {
        dst_picture_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.dst_picture_resource.base_array_layer,
        )?,
        setup_reference_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.setup_reference_slot.resource.base_array_layer,
        )?,
        begin_reference_image_views: plan
            .common
            .begin_reference_slots
            .iter()
            .map(|slot| native_vulkan_vulkanalia_layer_view(image, slot.resource.base_array_layer))
            .collect::<Result<Vec<_>, _>>()?,
        decode_reference_image_views: plan
            .common
            .decode_reference_slots
            .iter()
            .map(|slot| native_vulkan_vulkanalia_layer_view(image, slot.resource.base_array_layer))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn native_vulkan_vulkanalia_h265_decode_image_view_bindings(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    plan: &super::video_decode_submit_h265::NativeVulkanVulkanaliaH265DecodeSubmitPlan,
) -> Result<NativeVulkanVulkanaliaDecodeImageViewBindings, String> {
    Ok(NativeVulkanVulkanaliaDecodeImageViewBindings {
        dst_picture_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.dst_picture_resource.base_array_layer,
        )?,
        setup_reference_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.setup_reference_slot.resource.base_array_layer,
        )?,
        begin_reference_image_views: plan
            .common
            .begin_reference_slots
            .iter()
            .map(|slot| native_vulkan_vulkanalia_layer_view(image, slot.resource.base_array_layer))
            .collect::<Result<Vec<_>, _>>()?,
        decode_reference_image_views: plan
            .common
            .decode_reference_slots
            .iter()
            .map(|slot| native_vulkan_vulkanalia_layer_view(image, slot.resource.base_array_layer))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn native_vulkan_vulkanalia_av1_decode_image_view_bindings(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    plan: &super::video_decode_submit_av1::NativeVulkanVulkanaliaAv1DecodeSubmitPlan,
) -> Result<NativeVulkanVulkanaliaDecodeImageViewBindings, String> {
    Ok(NativeVulkanVulkanaliaDecodeImageViewBindings {
        dst_picture_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.dst_picture_resource.base_array_layer,
        )?,
        setup_reference_image_view: native_vulkan_vulkanalia_layer_view(
            image,
            plan.common.setup_reference_slot.resource.base_array_layer,
        )?,
        begin_reference_image_views: plan
            .common
            .begin_reference_slots
            .iter()
            .map(|slot| native_vulkan_vulkanalia_layer_view(image, slot.resource.base_array_layer))
            .collect::<Result<Vec<_>, _>>()?,
        decode_reference_image_views: plan
            .common
            .decode_reference_slots
            .iter()
            .map(|slot| native_vulkan_vulkanalia_layer_view(image, slot.resource.base_array_layer))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn native_vulkan_vulkanalia_layer_view(
    image: &super::video_session_images::VulkanaliaVideoSessionResourceImage,
    layer: u32,
) -> Result<vk::ImageView, String> {
    image
        .layer_views
        .get(layer as usize)
        .copied()
        .ok_or_else(|| {
            format!(
                "Vulkanalia video image has {} layer views but layer {layer} was requested",
                image.layer_views.len()
            )
        })
}

fn queue_flag_labels(flags: vk::QueueFlags) -> Vec<&'static str> {
    [
        (vk::QueueFlags::GRAPHICS, "graphics"),
        (vk::QueueFlags::COMPUTE, "compute"),
        (vk::QueueFlags::TRANSFER, "transfer"),
        (vk::QueueFlags::SPARSE_BINDING, "sparse-binding"),
        (vk::QueueFlags::PROTECTED, "protected"),
        (vk::QueueFlags::VIDEO_DECODE_KHR, "video-decode"),
        (vk::QueueFlags::VIDEO_ENCODE_KHR, "video-encode"),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flags.contains(flag).then_some(label))
    .collect()
}

fn video_codec_operation_labels(flags: vk::VideoCodecOperationFlagsKHR) -> Vec<&'static str> {
    [
        (vk::VideoCodecOperationFlagsKHR::DECODE_H264, "decode-h264"),
        (vk::VideoCodecOperationFlagsKHR::DECODE_H265, "decode-h265"),
        (vk::VideoCodecOperationFlagsKHR::DECODE_AV1, "decode-av1"),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flags.contains(flag).then_some(label))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::super::video_codec::{
        native_vulkan_vulkanalia_video_session_format_probe_profile as vulkanalia_video_session_format_probe_profile,
        native_vulkan_vulkanalia_video_session_picture_format as vulkanalia_video_session_picture_format,
        native_vulkan_vulkanalia_video_session_profile_label as vulkanalia_video_session_profile_label,
    };
    use super::super::video_device::native_vulkan_vulkanalia_video_decode_required_device_extensions;
    use super::*;

    #[test]
    fn session_bind_smoke_maps_codec_extensions_and_formats() {
        assert_eq!(
            native_vulkan_vulkanalia_video_decode_required_device_extensions(
                NativeVulkanVideoSessionCodec::H265Main10
            ),
            vec![
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                "VK_KHR_video_decode_h265"
            ]
        );
        assert_eq!(
            vulkanalia_video_session_picture_format(NativeVulkanVideoSessionCodec::Av1Main10),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        );
        assert_eq!(
            vulkanalia_video_session_format_probe_profile(NativeVulkanVideoSessionCodec::H264High8),
            "high"
        );
        assert_eq!(
            vulkanalia_video_session_profile_label(NativeVulkanVideoSessionCodec::H264High8),
            "high-8"
        );
    }
}
