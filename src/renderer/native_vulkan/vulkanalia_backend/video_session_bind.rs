use std::ffi::CString;

use crate::renderer::native_vulkan::{
    NativeVulkanAv1SequenceHeaderSnapshot, NativeVulkanH264ParameterSetSnapshot,
    NativeVulkanH265ParameterSetSnapshot, NativeVulkanVideoSessionCodec,
};
use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::loader::LibloadingLoader;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder, KhrVideoQueueExtensionInstanceCommands};

use super::queue_probe::native_vulkan_vulkanalia_video_decode_queue_family_indices;
use super::video_bitstream_buffer::{
    NativeVulkanVulkanaliaVideoSessionBitstreamBufferSmokeSnapshot,
    native_vulkan_vulkanalia_create_video_session_bitstream_buffer,
    native_vulkan_vulkanalia_destroy_video_session_bitstream_buffer,
    native_vulkan_vulkanalia_smoke_create_video_session_bitstream_buffer,
};
use super::video_command_pool::{
    native_vulkan_vulkanalia_create_decode_command_buffer,
    native_vulkan_vulkanalia_destroy_decode_command_buffer,
};
use super::video_decode_commands::{
    native_vulkan_vulkanalia_record_h265_decode_command_buffer,
    native_vulkan_vulkanalia_submit_decode_command_buffer2,
};
use super::video_decode_submit::NativeVulkanVulkanaliaDecodeImageViewBindings;
use super::video_decode_submit_h265::{
    NativeVulkanVulkanaliaH265ParameterIds,
    NativeVulkanVulkanaliaH265ReadyPrefixCommandFrameSnapshot,
    NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot,
    NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput,
    native_vulkan_vulkanalia_h265_ready_prefix_decode_submit_plan,
};
use super::video_format_probe::{
    NativeVulkanVulkanaliaVideoFormatQuerySnapshot, native_vulkan_vulkanalia_video_format_probe,
};
use super::video_profile_labels::{
    av1_level_label, h264_level_label, h265_level_label, video_capability_flag_labels,
    video_decode_capability_flag_labels,
};
use super::video_session::{
    NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot,
    NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    native_vulkan_vulkanalia_bind_video_session_memory_resources,
    native_vulkan_vulkanalia_create_video_session, native_vulkan_vulkanalia_destroy_video_session,
    native_vulkan_vulkanalia_destroy_video_session_memory_binding_resources,
    native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe,
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
    native_vulkan_vulkanalia_av1_sequence_header_bit_depth,
    native_vulkan_vulkanalia_av1_sequence_header_profile_label,
    native_vulkan_vulkanalia_smoke_create_av1_video_session_parameters,
};
use super::video_session_parameters_h264::{
    native_vulkan_vulkanalia_h264_std_profile_idc, native_vulkan_vulkanalia_h264_std_profile_label,
    native_vulkan_vulkanalia_smoke_create_h264_video_session_parameters,
};
use super::video_session_parameters_h265::{
    native_vulkan_vulkanalia_create_h265_video_session_parameters,
    native_vulkan_vulkanalia_smoke_create_h265_video_session_parameters,
};

const LOADER_CANDIDATES: &[&str] = &["libvulkan.so.1", "libvulkan.so"];
const VIDEO_QUEUE_EXTENSION_NAME: &str = "VK_KHR_video_queue";
const VIDEO_DECODE_QUEUE_EXTENSION_NAME: &str = "VK_KHR_video_decode_queue";
const VIDEO_DECODE_H264_EXTENSION_NAME: &str = "VK_KHR_video_decode_h264";
const VIDEO_DECODE_H265_EXTENSION_NAME: &str = "VK_KHR_video_decode_h265";
const VIDEO_DECODE_AV1_EXTENSION_NAME: &str = "VK_KHR_video_decode_av1";

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
    pub h265_ready_prefix_decode: Option<NativeVulkanVulkanaliaH265ReadyPrefixDecodeInput>,
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
            h265_ready_prefix_decode: None,
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
    pub h265_ready_prefix_decode_requested: bool,
    pub h265_ready_prefix_decode: Option<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot>,
}

pub fn probe_native_vulkan_vulkanalia_video_session_bind(
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let (loader, loader_name) = load_vulkanalia_loader()?;
    let entry = unsafe { Entry::new(loader) }
        .map_err(|err| format!("vulkanalia Entry::new({loader_name}): {err}"))?;

    let app_info = vk::ApplicationInfo::builder()
        .application_name(b"gilder-native-vulkan\0")
        .application_version(1)
        .engine_name(b"gilder\0")
        .engine_version(1)
        .api_version(u32::from(Version::V1_4_0));
    let create_info = vk::InstanceCreateInfo::builder().application_info(&app_info);
    let instance = unsafe { entry.create_instance(&create_info, None) }
        .map_err(|err| format!("vkCreateInstance(vulkanalia video session bind): {err:?}"))?;

    let result =
        probe_native_vulkan_vulkanalia_video_session_bind_inner(&instance, loader_name, options);
    unsafe {
        instance.destroy_instance(None);
    }
    result
}

fn probe_native_vulkan_vulkanalia_video_session_bind_inner(
    instance: &Instance,
    loader_name: &'static str,
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let selection = select_vulkanalia_video_session_physical_device(instance, options.codec)?;
    let requested_extent = vk::Extent2D {
        width: options.width,
        height: options.height,
    };
    let picture_format = vulkanalia_video_session_effective_picture_format(&options);
    let picture_format_label = format!("{picture_format:?}");
    let video_format_capabilities = native_vulkan_vulkanalia_video_format_probe(
        instance,
        selection.physical_device,
        &selection.device_extensions,
        true,
    );
    let format_probe_profile = vulkanalia_video_session_effective_format_probe_profile(&options)?;
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

    let priorities = [1.0_f32];
    let queue_create_info = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(selection.queue_family_index)
        .queue_priorities(&priorities)
        .build();
    let queue_create_infos = [queue_create_info];
    let enabled_device_extensions =
        vulkanalia_video_session_required_device_extensions(options.codec);
    let extension_names = enabled_device_extensions
        .iter()
        .map(|extension| CString::new(*extension).expect("static extension name has no nul"))
        .collect::<Vec<_>>();
    let extension_name_ptrs = extension_names
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();
    let device_create_info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&extension_name_ptrs);
    let device =
        unsafe { instance.create_device(selection.physical_device, &device_create_info, None) }
            .map_err(|err| format!("vkCreateDevice(vulkanalia video session bind): {err:?}"))?;

    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let result = match options.codec {
        NativeVulkanVideoSessionCodec::H264High8 => {
            let h264_std_profile_idc =
                vulkanalia_video_session_effective_h264_std_profile_idc(&options)?;
            let mut h264_profile_info = vk::VideoDecodeH264ProfileInfoKHR::builder()
                .std_profile_idc(h264_std_profile_idc)
                .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE)
                .build();
            let profile_info = vk::VideoProfileInfoKHR::builder()
                .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
                .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::_420)
                .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::_8)
                .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::_8)
                .push_next(&mut h264_profile_info)
                .build();
            let mut h264_capabilities = vk::VideoDecodeH264CapabilitiesKHR::default();
            let queried = query_vulkanalia_h264_video_session_capabilities(
                instance,
                selection.physical_device,
                &profile_info,
                &mut h264_capabilities,
            )?;
            smoke_bind_vulkanalia_video_session_profile(
                instance,
                &device,
                &memory_properties,
                &selection,
                loader_name,
                options,
                requested_extent,
                picture_format,
                target_picture_dpb_supported,
                target_picture_sampled_output_supported,
                target_resource_plan,
                &profile_info,
                queried,
            )
        }
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            let mut h265_profile_info = vk::VideoDecodeH265ProfileInfoKHR::builder()
                .std_profile_idc(match options.codec {
                    NativeVulkanVideoSessionCodec::H265Main8 => {
                        vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN
                    }
                    NativeVulkanVideoSessionCodec::H265Main10 => {
                        vk::video::STD_VIDEO_H265_PROFILE_IDC_MAIN_10
                    }
                    _ => unreachable!("matched H.265 codec"),
                })
                .build();
            let bit_depth = vulkanalia_video_session_bit_depth(options.codec);
            let profile_info = vk::VideoProfileInfoKHR::builder()
                .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H265)
                .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::_420)
                .luma_bit_depth(bit_depth)
                .chroma_bit_depth(bit_depth)
                .push_next(&mut h265_profile_info)
                .build();
            let mut h265_capabilities = vk::VideoDecodeH265CapabilitiesKHR::default();
            let queried = query_vulkanalia_h265_video_session_capabilities(
                instance,
                selection.physical_device,
                &profile_info,
                &mut h265_capabilities,
            )?;
            smoke_bind_vulkanalia_video_session_profile(
                instance,
                &device,
                &memory_properties,
                &selection,
                loader_name,
                options,
                requested_extent,
                picture_format,
                target_picture_dpb_supported,
                target_picture_sampled_output_supported,
                target_resource_plan,
                &profile_info,
                queried,
            )
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            let mut av1_profile_info = vk::VideoDecodeAV1ProfileInfoKHR::builder()
                .std_profile(vk::video::STD_VIDEO_AV1_PROFILE_MAIN)
                .film_grain_support(false)
                .build();
            let bit_depth = vulkanalia_video_session_effective_bit_depth(&options);
            let profile_info = vk::VideoProfileInfoKHR::builder()
                .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_AV1)
                .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::_420)
                .luma_bit_depth(bit_depth)
                .chroma_bit_depth(bit_depth)
                .push_next(&mut av1_profile_info)
                .build();
            let mut av1_capabilities = vk::VideoDecodeAV1CapabilitiesKHR::default();
            let queried = query_vulkanalia_av1_video_session_capabilities(
                instance,
                selection.physical_device,
                &profile_info,
                &mut av1_capabilities,
            )?;
            smoke_bind_vulkanalia_video_session_profile(
                instance,
                &device,
                &memory_properties,
                &selection,
                loader_name,
                options,
                requested_extent,
                picture_format,
                target_picture_dpb_supported,
                target_picture_sampled_output_supported,
                target_resource_plan,
                &profile_info,
                queried,
            )
        }
    };

    unsafe {
        device.destroy_device(None);
    }
    result
}

#[derive(Debug, Clone)]
struct VulkanaliaVideoSessionPhysicalDeviceSelection {
    physical_device_index: usize,
    physical_device: vk::PhysicalDevice,
    properties: vk::PhysicalDeviceProperties,
    queue_family_index: u32,
    queue_count: u32,
    queue_flags: vk::QueueFlags,
    device_extensions: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct VulkanaliaVideoSessionCapabilityQuery {
    capabilities: vk::VideoCapabilitiesKHR,
    decode_capability_flags: vk::VideoDecodeCapabilityFlagsKHR,
    codec_max_level: Option<&'static str>,
    codec_max_level_raw: Option<i32>,
}

fn smoke_bind_vulkanalia_video_session_profile(
    instance: &Instance,
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    selection: &VulkanaliaVideoSessionPhysicalDeviceSelection,
    loader_name: &'static str,
    options: NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
    requested_extent: vk::Extent2D,
    picture_format: vk::Format,
    target_picture_dpb_supported: bool,
    target_picture_sampled_output_supported: bool,
    target_resource_plan: NativeVulkanVulkanaliaVideoSessionResourceProbePlan,
    profile_info: &vk::VideoProfileInfoKHR,
    queried: VulkanaliaVideoSessionCapabilityQuery,
) -> Result<NativeVulkanVulkanaliaVideoSessionBindSmokeSnapshot, String> {
    let capabilities = queried.capabilities;
    let effective_profile_label = vulkanalia_video_session_effective_profile_label(&options)?;
    let requested_extent_supported =
        vulkanalia_video_session_extent_supported(requested_extent, capabilities);
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

    let session_max_dpb_slots = vulkanalia_video_session_max_dpb_slots(capabilities.max_dpb_slots);
    let session_max_active_reference_pictures =
        vulkanalia_video_session_max_active_reference_pictures(
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
        let h265_ready_prefix_decode =
            if let Some(input) = options.h265_ready_prefix_decode.as_ref() {
                Some(
                    native_vulkan_vulkanalia_record_h265_ready_prefix_decode_smoke(
                        instance,
                        device,
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
            enabled_device_extensions: vulkanalia_video_session_required_device_extensions(
                options.codec,
            ),
            video_codec_operation: video_codec_operation_labels(
                vulkanalia_video_session_codec_operation(options.codec),
            ),
            profile: effective_profile_label,
            format_probe_profile: vulkanalia_video_session_effective_format_probe_profile(
                &options,
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
                || options.h265_ready_prefix_decode.is_some(),
            session_parameters,
            h265_ready_prefix_decode_requested: options.h265_ready_prefix_decode.is_some(),
            h265_ready_prefix_decode,
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
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    selection: &VulkanaliaVideoSessionPhysicalDeviceSelection,
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
    let session_parameters = native_vulkan_vulkanalia_create_h265_video_session_parameters(
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
        (|| -> Result<NativeVulkanVulkanaliaH265ReadyPrefixCommandSmokeSnapshot, String> {
            let parameter_ids =
                NativeVulkanVulkanaliaH265ParameterIds::from_parameter_sets(&input.parameter_sets)?;
            let image_ref = image
                .as_ref()
                .expect("Vulkanalia H.265 decode image is alive during smoke");
            let session_parameters_ref = session_parameters
                .as_ref()
                .expect("Vulkanalia H.265 session parameters are alive during smoke");
            let bitstream_buffer_ref = bitstream_buffer
                .as_ref()
                .expect("Vulkanalia bitstream buffer is alive during smoke");
            let command_buffer_ref = command_buffer
                .as_ref()
                .expect("Vulkanalia command buffer is alive during smoke");
            let queue = unsafe { device.get_device_queue(selection.queue_family_index, 0) };
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

                frame_snapshots.push(NativeVulkanVulkanaliaH265ReadyPrefixCommandFrameSnapshot {
                    frame_index: u32::try_from(frame_index)
                        .map_err(|_| "Vulkanalia H.265 frame index exceeds u32".to_owned())?,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeVulkanVulkanaliaH265FrameBitstream {
    src_buffer_offset: u64,
    src_buffer_range: u64,
}

fn native_vulkan_vulkanalia_h265_decode_payloads(
    frames: &[super::video_decode_submit_h265::NativeVulkanVulkanaliaH265ReadyPrefixFrameInput],
    min_offset_alignment: u64,
    min_size_alignment: u64,
) -> Result<(Vec<u8>, Vec<NativeVulkanVulkanaliaH265FrameBitstream>), String> {
    if frames.is_empty() {
        return Err("Vulkanalia H.265 decode payload set cannot be empty".to_owned());
    }

    let mut bytes = Vec::new();
    let mut bitstreams = Vec::with_capacity(frames.len());
    for frame in frames {
        if frame.access_unit_payload.is_empty() {
            return Err(format!(
                "Vulkanalia H.265 AU {} decode payload cannot be empty",
                frame.entry.access_unit_index
            ));
        }
        let src_buffer_offset =
            native_vulkan_vulkanalia_align_up(bytes.len() as u64, min_offset_alignment.max(1))?;
        bytes.resize(src_buffer_offset as usize, 0);
        let src_buffer_range = native_vulkan_vulkanalia_align_up(
            frame.access_unit_payload.len() as u64,
            min_size_alignment.max(1),
        )?;
        bytes.extend_from_slice(&frame.access_unit_payload);
        bytes.resize((src_buffer_offset + src_buffer_range) as usize, 0);
        bitstreams.push(NativeVulkanVulkanaliaH265FrameBitstream {
            src_buffer_offset,
            src_buffer_range,
        });
    }
    Ok((bytes, bitstreams))
}

fn native_vulkan_vulkanalia_align_up(value: u64, alignment: u64) -> Result<u64, String> {
    let alignment = alignment.max(1);
    value
        .checked_add(alignment.saturating_sub(1))
        .map(|aligned| aligned / alignment * alignment)
        .ok_or_else(|| "Vulkanalia alignment overflow".to_owned())
}

fn query_vulkanalia_h264_video_session_capabilities(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR,
    h264_capabilities: &mut vk::VideoDecodeH264CapabilitiesKHR,
) -> Result<VulkanaliaVideoSessionCapabilityQuery, String> {
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(h264_capabilities)
        .push_next(&mut decode_capabilities)
        .build();
    unsafe {
        instance.get_physical_device_video_capabilities_khr(
            physical_device,
            profile_info,
            &mut capabilities,
        )
    }
    .map_err(|err| format!("vkGetPhysicalDeviceVideoCapabilitiesKHR(h264): {err:?}"))?;
    Ok(VulkanaliaVideoSessionCapabilityQuery {
        capabilities,
        decode_capability_flags: decode_capabilities.flags,
        codec_max_level: h264_level_label(h264_capabilities.max_level_idc),
        codec_max_level_raw: Some(h264_capabilities.max_level_idc.0),
    })
}

fn query_vulkanalia_h265_video_session_capabilities(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR,
    h265_capabilities: &mut vk::VideoDecodeH265CapabilitiesKHR,
) -> Result<VulkanaliaVideoSessionCapabilityQuery, String> {
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(h265_capabilities)
        .push_next(&mut decode_capabilities)
        .build();
    unsafe {
        instance.get_physical_device_video_capabilities_khr(
            physical_device,
            profile_info,
            &mut capabilities,
        )
    }
    .map_err(|err| format!("vkGetPhysicalDeviceVideoCapabilitiesKHR(h265): {err:?}"))?;
    Ok(VulkanaliaVideoSessionCapabilityQuery {
        capabilities,
        decode_capability_flags: decode_capabilities.flags,
        codec_max_level: h265_level_label(h265_capabilities.max_level_idc),
        codec_max_level_raw: Some(h265_capabilities.max_level_idc.0),
    })
}

fn query_vulkanalia_av1_video_session_capabilities(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    profile_info: &vk::VideoProfileInfoKHR,
    av1_capabilities: &mut vk::VideoDecodeAV1CapabilitiesKHR,
) -> Result<VulkanaliaVideoSessionCapabilityQuery, String> {
    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    let mut capabilities = vk::VideoCapabilitiesKHR::builder()
        .push_next(av1_capabilities)
        .push_next(&mut decode_capabilities)
        .build();
    unsafe {
        instance.get_physical_device_video_capabilities_khr(
            physical_device,
            profile_info,
            &mut capabilities,
        )
    }
    .map_err(|err| format!("vkGetPhysicalDeviceVideoCapabilitiesKHR(av1): {err:?}"))?;
    Ok(VulkanaliaVideoSessionCapabilityQuery {
        capabilities,
        decode_capability_flags: decode_capabilities.flags,
        codec_max_level: av1_level_label(av1_capabilities.max_level),
        codec_max_level_raw: Some(av1_capabilities.max_level.0),
    })
}

fn select_vulkanalia_video_session_physical_device(
    instance: &Instance,
    codec: NativeVulkanVideoSessionCodec,
) -> Result<VulkanaliaVideoSessionPhysicalDeviceSelection, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia video session): {err:?}"))?;
    let required_extensions = vulkanalia_video_session_required_device_extensions(codec);
    let mut rejected = Vec::new();

    for (physical_device_index, physical_device) in physical_devices.iter().copied().enumerate() {
        let properties = unsafe { instance.get_physical_device_properties(physical_device) };
        let device_extensions =
            unsafe { instance.enumerate_device_extension_properties(physical_device, None) }
                .map_err(|err| {
                    format!(
                        "vkEnumerateDeviceExtensionProperties(vulkanalia video session): {err:?}"
                    )
                })?
                .into_iter()
                .map(|property| property.extension_name.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
        let missing_extensions = required_extensions
            .iter()
            .copied()
            .filter(|required| {
                !device_extensions
                    .iter()
                    .any(|available| available == required)
            })
            .collect::<Vec<_>>();
        if !missing_extensions.is_empty() {
            rejected.push(format!(
                "{} missing {}",
                properties.device_name.to_string_lossy(),
                missing_extensions.join(", ")
            ));
            continue;
        }

        let queue_family_indices =
            native_vulkan_vulkanalia_video_decode_queue_family_indices(instance, physical_device);
        let Some(queue_family_index) = queue_family_indices.first().copied() else {
            rejected.push(format!(
                "{} has no VIDEO_DECODE_KHR queue family",
                properties.device_name.to_string_lossy()
            ));
            continue;
        };
        let queue_families =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        let queue_family = queue_families
            .get(queue_family_index as usize)
            .ok_or_else(|| format!("selected invalid queue family index {queue_family_index}"))?;

        return Ok(VulkanaliaVideoSessionPhysicalDeviceSelection {
            physical_device_index,
            physical_device,
            properties,
            queue_family_index,
            queue_count: queue_family.queue_count,
            queue_flags: queue_family.queue_flags,
            device_extensions,
        });
    }

    Err(format!(
        "no Vulkanalia physical device can create {} video session{}",
        vulkanalia_video_session_label(codec),
        if rejected.is_empty() {
            String::new()
        } else {
            format!(": {}", rejected.join("; "))
        }
    ))
}

fn load_vulkanalia_loader() -> Result<(LibloadingLoader, &'static str), String> {
    let mut errors = Vec::new();
    for candidate in LOADER_CANDIDATES {
        match unsafe { LibloadingLoader::new(candidate) } {
            Ok(loader) => return Ok((loader, candidate)),
            Err(err) => errors.push(format!("{candidate}: {err}")),
        }
    }
    Err(format!(
        "failed to load Vulkan loader via vulkanalia: {}",
        errors.join("; ")
    ))
}

fn vulkanalia_video_session_required_device_extensions(
    codec: NativeVulkanVideoSessionCodec,
) -> Vec<&'static str> {
    let mut extensions = vec![
        VIDEO_QUEUE_EXTENSION_NAME,
        VIDEO_DECODE_QUEUE_EXTENSION_NAME,
    ];
    extensions.push(match codec {
        NativeVulkanVideoSessionCodec::H264High8 => VIDEO_DECODE_H264_EXTENSION_NAME,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            VIDEO_DECODE_H265_EXTENSION_NAME
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            VIDEO_DECODE_AV1_EXTENSION_NAME
        }
    });
    extensions
}

fn vulkanalia_video_session_codec_name(codec: NativeVulkanVideoSessionCodec) -> &'static str {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => "h264",
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            "h265"
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => "av1",
    }
}

fn vulkanalia_video_session_label(codec: NativeVulkanVideoSessionCodec) -> &'static str {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => "h264-high-8",
        NativeVulkanVideoSessionCodec::H265Main8 => "h265-main-8",
        NativeVulkanVideoSessionCodec::H265Main10 => "h265-main-10",
        NativeVulkanVideoSessionCodec::Av1Main8 => "av1-main-8",
        NativeVulkanVideoSessionCodec::Av1Main10 => "av1-main-10",
    }
}

fn vulkanalia_video_session_profile_label(codec: NativeVulkanVideoSessionCodec) -> &'static str {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => "high-8",
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::Av1Main8 => {
            "main-8"
        }
        NativeVulkanVideoSessionCodec::H265Main10 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            "main-10"
        }
    }
}

fn vulkanalia_video_session_effective_profile_label(
    options: &NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<&'static str, String> {
    match options.codec {
        NativeVulkanVideoSessionCodec::H264High8 => {
            if let Some(parameter_sets) = options.h264_parameter_sets.as_ref() {
                let profile = native_vulkan_vulkanalia_h264_std_profile_label(
                    parameter_sets.sps.profile_idc,
                )?;
                Ok(match profile {
                    "baseline" => "baseline-8",
                    "main" => "main-8",
                    "high" => "high-8",
                    _ => unreachable!("mapper returns a fixed H.264 profile label"),
                })
            } else {
                Ok(vulkanalia_video_session_profile_label(options.codec))
            }
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            if let Some(sequence_header) = options.av1_sequence_header.as_ref() {
                Ok(native_vulkan_vulkanalia_av1_sequence_header_profile_label(
                    sequence_header,
                ))
            } else {
                Ok(vulkanalia_video_session_profile_label(options.codec))
            }
        }
        _ => Ok(vulkanalia_video_session_profile_label(options.codec)),
    }
}

fn vulkanalia_video_session_format_probe_profile(
    codec: NativeVulkanVideoSessionCodec,
) -> &'static str {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => "high",
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::Av1Main8 => {
            "main-8"
        }
        NativeVulkanVideoSessionCodec::H265Main10 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            "main-10"
        }
    }
}

fn vulkanalia_video_session_effective_format_probe_profile(
    options: &NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<&'static str, String> {
    match options.codec {
        NativeVulkanVideoSessionCodec::H264High8 => options
            .h264_parameter_sets
            .as_ref()
            .map(|parameter_sets| {
                native_vulkan_vulkanalia_h264_std_profile_label(parameter_sets.sps.profile_idc)
            })
            .transpose()
            .map(|profile| profile.unwrap_or("high")),
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            Ok(options
                .av1_sequence_header
                .as_ref()
                .map(native_vulkan_vulkanalia_av1_sequence_header_profile_label)
                .unwrap_or_else(|| vulkanalia_video_session_format_probe_profile(options.codec)))
        }
        _ => Ok(vulkanalia_video_session_format_probe_profile(options.codec)),
    }
}

fn vulkanalia_video_session_effective_h264_std_profile_idc(
    options: &NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> Result<vk::video::StdVideoH264ProfileIdc, String> {
    options
        .h264_parameter_sets
        .as_ref()
        .map(|parameter_sets| {
            native_vulkan_vulkanalia_h264_std_profile_idc(parameter_sets.sps.profile_idc)
        })
        .transpose()
        .map(|profile| profile.unwrap_or(vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH))
}

fn vulkanalia_video_session_bit_depth(
    codec: NativeVulkanVideoSessionCodec,
) -> vk::VideoComponentBitDepthFlagsKHR {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8
        | NativeVulkanVideoSessionCodec::H265Main8
        | NativeVulkanVideoSessionCodec::Av1Main8 => vk::VideoComponentBitDepthFlagsKHR::_8,
        NativeVulkanVideoSessionCodec::H265Main10 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            vk::VideoComponentBitDepthFlagsKHR::_10
        }
    }
}

fn vulkanalia_video_session_effective_bit_depth(
    options: &NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> vk::VideoComponentBitDepthFlagsKHR {
    if matches!(
        options.codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) && let Some(sequence_header) = options.av1_sequence_header.as_ref()
    {
        return match native_vulkan_vulkanalia_av1_sequence_header_bit_depth(sequence_header) {
            10 => vk::VideoComponentBitDepthFlagsKHR::_10,
            _ => vk::VideoComponentBitDepthFlagsKHR::_8,
        };
    }
    vulkanalia_video_session_bit_depth(options.codec)
}

fn vulkanalia_video_session_picture_format(codec: NativeVulkanVideoSessionCodec) -> vk::Format {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8
        | NativeVulkanVideoSessionCodec::H265Main8
        | NativeVulkanVideoSessionCodec::Av1Main8 => vk::Format::G8_B8R8_2PLANE_420_UNORM,
        NativeVulkanVideoSessionCodec::H265Main10 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        }
    }
}

fn vulkanalia_video_session_effective_picture_format(
    options: &NativeVulkanVulkanaliaVideoSessionBindSmokeOptions,
) -> vk::Format {
    if matches!(
        options.codec,
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10
    ) && let Some(sequence_header) = options.av1_sequence_header.as_ref()
    {
        return match native_vulkan_vulkanalia_av1_sequence_header_bit_depth(sequence_header) {
            10 => vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            _ => vk::Format::G8_B8R8_2PLANE_420_UNORM,
        };
    }
    vulkanalia_video_session_picture_format(options.codec)
}

fn vulkanalia_video_session_codec_operation(
    codec: NativeVulkanVideoSessionCodec,
) -> vk::VideoCodecOperationFlagsKHR {
    match codec {
        NativeVulkanVideoSessionCodec::H264High8 => vk::VideoCodecOperationFlagsKHR::DECODE_H264,
        NativeVulkanVideoSessionCodec::H265Main8 | NativeVulkanVideoSessionCodec::H265Main10 => {
            vk::VideoCodecOperationFlagsKHR::DECODE_H265
        }
        NativeVulkanVideoSessionCodec::Av1Main8 | NativeVulkanVideoSessionCodec::Av1Main10 => {
            vk::VideoCodecOperationFlagsKHR::DECODE_AV1
        }
    }
}

fn video_format_probe_includes_format(
    queries: &[NativeVulkanVulkanaliaVideoFormatQuerySnapshot],
    codec: &'static str,
    profile: &'static str,
    format: &str,
) -> bool {
    queries
        .iter()
        .find(|query| query.codec == codec && query.profile == profile)
        .is_some_and(|query| {
            query
                .formats
                .iter()
                .any(|property| property.format == format)
        })
}

fn vulkanalia_video_session_extent_supported(
    extent: vk::Extent2D,
    capabilities: vk::VideoCapabilitiesKHR,
) -> bool {
    extent.width >= capabilities.min_coded_extent.width
        && extent.height >= capabilities.min_coded_extent.height
        && extent.width <= capabilities.max_coded_extent.width
        && extent.height <= capabilities.max_coded_extent.height
        && vulkanalia_video_session_extent_aligned(
            extent.width,
            capabilities.picture_access_granularity.width,
        )
        && vulkanalia_video_session_extent_aligned(
            extent.height,
            capabilities.picture_access_granularity.height,
        )
}

fn vulkanalia_video_session_extent_aligned(value: u32, granularity: u32) -> bool {
    granularity == 0 || value.is_multiple_of(granularity)
}

fn vulkanalia_video_session_max_dpb_slots(driver_max_dpb_slots: u32) -> u32 {
    if driver_max_dpb_slots == 0 {
        0
    } else {
        driver_max_dpb_slots.min(8).max(1)
    }
}

fn vulkanalia_video_session_max_active_reference_pictures(
    driver_max_active_reference_pictures: u32,
    session_max_dpb_slots: u32,
) -> u32 {
    if driver_max_active_reference_pictures == 0 || session_max_dpb_slots == 0 {
        0
    } else {
        driver_max_active_reference_pictures
            .min(session_max_dpb_slots)
            .min(session_max_dpb_slots.max(1))
    }
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
    use super::*;
    use crate::renderer::native_vulkan::{
        NativeVulkanAv1ColorConfigSnapshot, NativeVulkanAv1OperatingPointSnapshot,
        NativeVulkanAv1SequenceHeaderSnapshot, NativeVulkanH265AccessUnitSliceSnapshot,
        NativeVulkanH265DecodeReferencePlanEntrySnapshot,
        NativeVulkanVulkanaliaH265ReadyPrefixFrameInput,
    };

    #[test]
    fn session_bind_smoke_maps_codec_extensions_and_formats() {
        assert_eq!(
            vulkanalia_video_session_required_device_extensions(
                NativeVulkanVideoSessionCodec::H265Main10
            ),
            vec![
                VIDEO_QUEUE_EXTENSION_NAME,
                VIDEO_DECODE_QUEUE_EXTENSION_NAME,
                VIDEO_DECODE_H265_EXTENSION_NAME
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

    #[test]
    fn session_bind_applies_av1_stream_bit_depth_only_to_av1() {
        let av1_main10_header = test_av1_sequence_header(10);
        let av1_options = NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
            codec: NativeVulkanVideoSessionCodec::Av1Main8,
            av1_sequence_header: Some(av1_main10_header.clone()),
            ..NativeVulkanVulkanaliaVideoSessionBindSmokeOptions::default()
        };
        assert_eq!(
            vulkanalia_video_session_effective_bit_depth(&av1_options),
            vk::VideoComponentBitDepthFlagsKHR::_10
        );
        assert_eq!(
            vulkanalia_video_session_effective_picture_format(&av1_options),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        );

        let h265_options = NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            av1_sequence_header: Some(av1_main10_header),
            ..NativeVulkanVulkanaliaVideoSessionBindSmokeOptions::default()
        };
        assert_eq!(
            vulkanalia_video_session_effective_bit_depth(&h265_options),
            vk::VideoComponentBitDepthFlagsKHR::_8
        );
        assert_eq!(
            vulkanalia_video_session_effective_picture_format(&h265_options),
            vk::Format::G8_B8R8_2PLANE_420_UNORM
        );
    }

    #[test]
    fn session_bind_extent_check_matches_driver_granularity() {
        let capabilities = vk::VideoCapabilitiesKHR::builder()
            .min_coded_extent(vk::Extent2D {
                width: 64,
                height: 64,
            })
            .max_coded_extent(vk::Extent2D {
                width: 3840,
                height: 2160,
            })
            .picture_access_granularity(vk::Extent2D {
                width: 16,
                height: 16,
            })
            .build();

        assert!(vulkanalia_video_session_extent_supported(
            vk::Extent2D {
                width: 1920,
                height: 1088,
            },
            capabilities
        ));
        assert!(!vulkanalia_video_session_extent_supported(
            vk::Extent2D {
                width: 1921,
                height: 1088,
            },
            capabilities
        ));
    }

    #[test]
    fn h265_multi_frame_payloads_align_offsets_and_ranges() {
        let frames = vec![
            test_h265_ready_prefix_frame(0, vec![1, 2, 3]),
            test_h265_ready_prefix_frame(1, vec![4; 260]),
        ];

        let (bytes, bitstreams) =
            native_vulkan_vulkanalia_h265_decode_payloads(&frames, 128, 256).unwrap();

        assert_eq!(bitstreams.len(), 2);
        assert_eq!(bitstreams[0].src_buffer_offset, 0);
        assert_eq!(bitstreams[0].src_buffer_range, 256);
        assert_eq!(bitstreams[1].src_buffer_offset, 256);
        assert_eq!(bitstreams[1].src_buffer_range, 512);
        assert_eq!(bytes.len(), 768);
        assert_eq!(&bytes[..3], &[1, 2, 3]);
        assert_eq!(&bytes[256..260], &[4, 4, 4, 4]);
    }

    fn test_av1_sequence_header(bit_depth: u8) -> NativeVulkanAv1SequenceHeaderSnapshot {
        NativeVulkanAv1SequenceHeaderSnapshot {
            parser: "test",
            seq_profile: 0,
            seq_profile_label: "main",
            still_picture: false,
            reduced_still_picture_header: false,
            timing_info_present_flag: false,
            timing_info: None,
            decoder_model_info_present_flag: false,
            buffer_delay_length_minus_1: 0,
            frame_presentation_time_length_minus_1: 0,
            initial_display_delay_present_flag: false,
            operating_points_cnt_minus_1: 0,
            operating_points: vec![NativeVulkanAv1OperatingPointSnapshot {
                index: 0,
                idc: 0,
                seq_level_idx: 0,
                seq_level_label: None,
                seq_tier: false,
                decoder_model_present_for_this_op: false,
                initial_display_delay_present_for_this_op: false,
                initial_display_delay_minus_1: None,
            }],
            frame_width_bits_minus_1: 15,
            frame_height_bits_minus_1: 15,
            max_frame_width_minus_1: 639,
            max_frame_height_minus_1: 367,
            max_frame_width: 640,
            max_frame_height: 368,
            frame_id_numbers_present_flag: false,
            delta_frame_id_length_minus_2: None,
            additional_frame_id_length_minus_1: None,
            use_128x128_superblock: false,
            enable_filter_intra: true,
            enable_intra_edge_filter: true,
            enable_interintra_compound: true,
            enable_masked_compound: true,
            enable_warped_motion: true,
            enable_dual_filter: true,
            enable_order_hint: true,
            enable_jnt_comp: true,
            enable_ref_frame_mvs: true,
            seq_force_screen_content_tools: 2,
            seq_force_integer_mv: 2,
            order_hint_bits_minus_1: Some(6),
            enable_superres: false,
            enable_cdef: true,
            enable_restoration: true,
            film_grain_params_present: false,
            color_config: NativeVulkanAv1ColorConfigSnapshot {
                high_bitdepth: bit_depth > 8,
                twelve_bit: bit_depth == 12,
                mono_chrome: false,
                color_description_present_flag: false,
                color_primaries: 2,
                transfer_characteristics: 2,
                matrix_coefficients: 2,
                color_range: false,
                subsampling_x: true,
                subsampling_y: true,
                chroma_sample_position: 0,
                separate_uv_delta_q: false,
                bit_depth,
                num_planes: 3,
            },
            requested_profile_compatible: matches!(bit_depth, 8 | 10),
            vulkan_std_session_parameters_ready: matches!(bit_depth, 8 | 10),
        }
    }

    fn test_h265_ready_prefix_frame(
        access_unit_index: u32,
        access_unit_payload: Vec<u8>,
    ) -> NativeVulkanVulkanaliaH265ReadyPrefixFrameInput {
        NativeVulkanVulkanaliaH265ReadyPrefixFrameInput {
            entry: NativeVulkanH265DecodeReferencePlanEntrySnapshot {
                access_unit_index,
                pts_ms: None,
                nal_type_label: None,
                current_poc: Some(access_unit_index as i32),
                planned_output_slot: access_unit_index,
                setup_slot_index: None,
                evicted_poc: None,
                references: Vec::new(),
                available_reference_count: 0,
                missing_reference_count: 0,
                missing_reference_pocs: Vec::new(),
                ready_for_decode_submit: true,
            },
            first_slice: NativeVulkanH265AccessUnitSliceSnapshot {
                nal_type: 1,
                nal_type_label: "TRAIL_R",
                slice_segment_offset: 0,
                first_slice_segment_in_pic_flag: true,
                slice_type: 1,
                pps_id: 0,
                pic_order_cnt_lsb: Some(0),
                short_term_ref_pic_set_sps_flag: false,
                short_term_ref_pic_set_idx: None,
                num_delta_pocs_of_ref_rps_idx: 0,
                num_bits_for_st_ref_pic_set_in_slice: 0,
                short_term_ref_pic_set: None,
                long_term_references: Vec::new(),
                idr: false,
                irap: false,
            },
            access_unit_payload,
            slice_segment_offset: 0,
        }
    }
}
