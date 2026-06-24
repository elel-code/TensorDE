use std::collections::BTreeSet;
use std::ffi::CString;
use std::ptr;

use crate::renderer::native_vulkan::NativeVulkanVideoSessionCodec;
use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::loader::LibloadingLoader;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder, KhrVideoQueueExtensionInstanceCommands};

use super::queue_probe::native_vulkan_vulkanalia_video_decode_queue_family_indices;
use super::video_format_probe::{
    NativeVulkanVulkanaliaVideoFormatProbeSnapshot, NativeVulkanVulkanaliaVideoFormatQuerySnapshot,
    native_vulkan_vulkanalia_video_format_probe,
};
use super::video_profile_labels::{
    av1_level_label, h264_level_label, h265_level_label, video_capability_flag_labels,
    video_decode_capability_flag_labels,
};

const DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const LOADER_CANDIDATES: &[&str] = &["libvulkan.so.1", "libvulkan.so"];
const VIDEO_QUEUE_EXTENSION_NAME: &str = "VK_KHR_video_queue";
const VIDEO_DECODE_QUEUE_EXTENSION_NAME: &str = "VK_KHR_video_decode_queue";
const VIDEO_DECODE_H264_EXTENSION_NAME: &str = "VK_KHR_video_decode_h264";
const VIDEO_DECODE_H265_EXTENSION_NAME: &str = "VK_KHR_video_decode_h265";
const VIDEO_DECODE_AV1_EXTENSION_NAME: &str = "VK_KHR_video_decode_av1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVulkanaliaVideoSessionResourceStepKind {
    ProfileFormatSelection,
    SessionCreate,
    SessionMemoryBind,
    ResourceImages,
    SessionParameters,
    BitstreamRing,
    DecodeSubmit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionResourceStep {
    pub order: u8,
    pub kind: NativeVulkanVulkanaliaVideoSessionResourceStepKind,
    pub ash_source: &'static str,
    pub vulkanalia_target: &'static str,
    pub validation_gate: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionTemplate {
    pub boundary: &'static str,
    pub ash_source_modules: &'static [&'static str],
    pub vulkanalia_target_module: &'static str,
    pub api_type_evidence: Vec<&'static str>,
    pub resource_steps: Vec<NativeVulkanVulkanaliaVideoSessionResourceStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionResourceProbePlan {
    pub codec: &'static str,
    pub profile: &'static str,
    pub sampled_output_format_count: usize,
    pub dpb_format_count: usize,
    pub coincident_format: Option<String>,
    pub sampled_output_ready: bool,
    pub dpb_ready: bool,
    pub direct_dpb_candidate: bool,
    pub query_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaMemoryTypeCandidate {
    pub index: u32,
    pub property_flags_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionMemoryRequirementSnapshot {
    pub memory_bind_index: u32,
    pub size: u64,
    pub alignment: u64,
    pub memory_type_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionMemoryBindPlan {
    pub memory_bind_index: u32,
    pub size: u64,
    pub alignment: u64,
    pub memory_type_bits: u32,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
    pub preferred_device_local: bool,
    pub dedicated_allocation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot {
    pub session_created: bool,
    pub memory_bound: bool,
    pub memory_requirements: Vec<NativeVulkanVulkanaliaVideoSessionMemoryRequirementSnapshot>,
    pub bind_plans: Vec<NativeVulkanVulkanaliaVideoSessionMemoryBindPlan>,
    pub total_bound_memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
    pub codec: NativeVulkanVideoSessionCodec,
    pub width: u32,
    pub height: u32,
}

impl Default for NativeVulkanVulkanaliaVideoSessionBindSmokeOptions {
    fn default() -> Self {
        Self {
            codec: NativeVulkanVideoSessionCodec::H265Main8,
            width: 3840,
            height: 2160,
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
}

pub fn native_vulkan_vulkanalia_video_session_template()
-> NativeVulkanVulkanaliaVideoSessionTemplate {
    NativeVulkanVulkanaliaVideoSessionTemplate {
        boundary: "vulkanalia-video-session-resource-ownership",
        ash_source_modules: &[
            "src/renderer/native_vulkan.rs::native_vulkan_create_video_session",
            "src/renderer/native_vulkan/video_session_resources.rs",
            "src/renderer/native_vulkan/video_session_parameters.rs",
            "src/renderer/native_vulkan/direct_h265_submit.rs",
        ],
        vulkanalia_target_module: "src/renderer/native_vulkan/vulkanalia_backend/video_session.rs",
        api_type_evidence: vec![
            std::any::type_name::<vk::VideoSessionCreateInfoKHR>(),
            std::any::type_name::<vk::VideoSessionKHR>(),
            std::any::type_name::<vk::VideoSessionMemoryRequirementsKHR>(),
            std::any::type_name::<vk::BindVideoSessionMemoryInfoKHR>(),
            std::any::type_name::<vk::VideoSessionParametersCreateInfoKHR>(),
            std::any::type_name::<vk::VideoBeginCodingInfoKHR>(),
            std::any::type_name::<vk::VideoDecodeInfoKHR>(),
            std::any::type_name::<vk::VideoPictureResourceInfoKHR>(),
            std::any::type_name::<vk::VideoReferenceSlotInfoKHR>(),
        ],
        resource_steps: vec![
            NativeVulkanVulkanaliaVideoSessionResourceStep {
                order: 0,
                kind: NativeVulkanVulkanaliaVideoSessionResourceStepKind::ProfileFormatSelection,
                ash_source: "native_vulkan_video_format_properties_raw",
                vulkanalia_target: "video_format_probe + session resource probe plan",
                validation_gate: "sampled decode-output and DPB formats are both reported before resource creation",
            },
            NativeVulkanVulkanaliaVideoSessionResourceStep {
                order: 1,
                kind: NativeVulkanVulkanaliaVideoSessionResourceStepKind::SessionCreate,
                ash_source: "native_vulkan_create_video_session",
                vulkanalia_target: "vkCreateVideoSessionKHR through vulkanalia Device commands",
                validation_gate: "created session reports non-empty explicit memory requirements",
            },
            NativeVulkanVulkanaliaVideoSessionResourceStep {
                order: 2,
                kind: NativeVulkanVulkanaliaVideoSessionResourceStepKind::SessionMemoryBind,
                ash_source: "native_vulkan_video_session_memory_requirements + bind",
                vulkanalia_target: "explicit default-initialized VideoSessionMemoryRequirementsKHR and BindVideoSessionMemoryInfoKHR",
                validation_gate: "memory bind indices/sizes match ash telemetry for each codec profile",
            },
            NativeVulkanVulkanaliaVideoSessionResourceStep {
                order: 3,
                kind: NativeVulkanVulkanaliaVideoSessionResourceStepKind::ResourceImages,
                ash_source: "native_vulkan_create_video_session_resource_image",
                vulkanalia_target: "Vulkanalia DPB/output image allocation module split from session creation",
                validation_gate: "coincident DPB/output image path is retained when supported; separate output image path remains available",
            },
            NativeVulkanVulkanaliaVideoSessionResourceStep {
                order: 4,
                kind: NativeVulkanVulkanaliaVideoSessionResourceStepKind::SessionParameters,
                ash_source: "native_vulkan_create_h264/h265/av1_video_session_parameters",
                vulkanalia_target: "codec-specific Vulkanalia session parameter builders",
                validation_gate: "parameter snapshot remains byte-for-byte equivalent to ash path for H.264/H.265/AV1",
            },
            NativeVulkanVulkanaliaVideoSessionResourceStep {
                order: 5,
                kind: NativeVulkanVulkanaliaVideoSessionResourceStepKind::BitstreamRing,
                ash_source: "native_vulkan_create_video_session_bitstream_buffer_with_mapping",
                vulkanalia_target: "Vulkanalia-owned bitstream ring with unchanged upload/copy telemetry",
                validation_gate: "bitstream upload copy scope is unchanged and still bounded",
            },
            NativeVulkanVulkanaliaVideoSessionResourceStep {
                order: 6,
                kind: NativeVulkanVulkanaliaVideoSessionResourceStepKind::DecodeSubmit,
                ash_source: "direct_h265_submit.rs and codec submit call sites",
                vulkanalia_target: "Vulkanalia VideoBeginCodingInfoKHR/VideoDecodeInfoKHR submit helpers",
                validation_gate: "H.265 main8/main10 ready-prefix submit matches ash output before H.264/AV1 are switched",
            },
        ],
    }
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
    let picture_format = vulkanalia_video_session_picture_format(options.codec);
    let picture_format_label = format!("{picture_format:?}");
    let video_format_capabilities = native_vulkan_vulkanalia_video_format_probe(
        instance,
        selection.physical_device,
        &selection.device_extensions,
        true,
    );
    let format_probe_profile = vulkanalia_video_session_format_probe_profile(options.codec);
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
            let mut h264_profile_info = vk::VideoDecodeH264ProfileInfoKHR::builder()
                .std_profile_idc(vk::video::STD_VIDEO_H264_PROFILE_IDC_HIGH)
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
            let bit_depth = vulkanalia_video_session_bit_depth(options.codec);
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
    let memory_binding = native_vulkan_vulkanalia_smoke_bind_video_session_memory(
        device,
        memory_properties,
        &create_info,
    )?;

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
        profile: vulkanalia_video_session_profile_label(options.codec),
        format_probe_profile: vulkanalia_video_session_format_probe_profile(options.codec),
        picture_format: format!("{picture_format:?}"),
        reference_picture_format: format!("{picture_format:?}"),
        target_picture_dpb_supported,
        target_picture_sampled_output_supported,
        target_resource_plan,
        capability_flags: video_capability_flag_labels(capabilities.flags),
        decode_capability_flags: video_decode_capability_flag_labels(
            queried.decode_capability_flags,
        ),
        min_bitstream_buffer_offset_alignment: capabilities.min_bitstream_buffer_offset_alignment,
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
    })
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

#[allow(dead_code)]
pub(super) fn native_vulkan_vulkanalia_create_video_session(
    device: &Device,
    create_info: &vk::VideoSessionCreateInfoKHR,
) -> Result<vk::VideoSessionKHR, String> {
    let mut session = vk::VideoSessionKHR::default();
    let result = unsafe {
        (device.commands().create_video_session_khr)(
            device.handle(),
            create_info,
            ptr::null(),
            &mut session,
        )
    };
    if result == vk::Result::SUCCESS {
        Ok(session)
    } else {
        Err(format!("vkCreateVideoSessionKHR(vulkanalia): {result:?}"))
    }
}

#[allow(dead_code)]
pub(super) fn native_vulkan_vulkanalia_video_session_memory_requirements(
    device: &Device,
    session: vk::VideoSessionKHR,
) -> Result<Vec<vk::VideoSessionMemoryRequirementsKHR>, String> {
    let mut memory_requirement_count = 0u32;
    let result = unsafe {
        (device.commands().get_video_session_memory_requirements_khr)(
            device.handle(),
            session,
            &mut memory_requirement_count,
            ptr::null_mut(),
        )
    };
    if result != vk::Result::SUCCESS {
        return Err(format!(
            "vkGetVideoSessionMemoryRequirementsKHR(count, vulkanalia): {result:?}"
        ));
    }
    if memory_requirement_count == 0 {
        return Ok(Vec::new());
    }

    let mut memory_requirements =
        vec![vk::VideoSessionMemoryRequirementsKHR::default(); memory_requirement_count as usize];
    let result = unsafe {
        (device.commands().get_video_session_memory_requirements_khr)(
            device.handle(),
            session,
            &mut memory_requirement_count,
            memory_requirements.as_mut_ptr(),
        )
    };
    if result != vk::Result::SUCCESS {
        return Err(format!(
            "vkGetVideoSessionMemoryRequirementsKHR(values, vulkanalia): {result:?}"
        ));
    }
    memory_requirements.truncate(memory_requirement_count as usize);
    Ok(memory_requirements)
}

#[allow(dead_code)]
pub(super) fn native_vulkan_vulkanalia_bind_video_session_memory(
    device: &Device,
    session: vk::VideoSessionKHR,
    bind_infos: &[vk::BindVideoSessionMemoryInfoKHR],
) -> Result<(), String> {
    let result = unsafe {
        (device.commands().bind_video_session_memory_khr)(
            device.handle(),
            session,
            bind_infos.len() as u32,
            bind_infos.as_ptr(),
        )
    };
    if result == vk::Result::SUCCESS {
        Ok(())
    } else {
        Err(format!(
            "vkBindVideoSessionMemoryKHR(vulkanalia): {result:?}"
        ))
    }
}

#[allow(dead_code)]
pub(super) fn native_vulkan_vulkanalia_destroy_video_session(
    device: &Device,
    session: vk::VideoSessionKHR,
) {
    unsafe {
        (device.commands().destroy_video_session_khr)(device.handle(), session, ptr::null());
    }
}

#[allow(dead_code)]
pub(super) fn native_vulkan_vulkanalia_smoke_bind_video_session_memory(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    create_info: &vk::VideoSessionCreateInfoKHR,
) -> Result<NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot, String> {
    let session = native_vulkan_vulkanalia_create_video_session(device, create_info)?;
    let mut allocated_memories = Vec::new();
    let result = (|| {
        let memory_requirements =
            native_vulkan_vulkanalia_video_session_memory_requirements(device, session)?;
        let memory_requirement_snapshots =
            native_vulkan_vulkanalia_video_session_memory_requirement_snapshots(
                &memory_requirements,
            );
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let bind_plans = native_vulkan_vulkanalia_video_session_memory_bind_plans(
            &memory_requirements,
            &memory_type_candidates,
        )?;
        let mut bind_infos = Vec::with_capacity(bind_plans.len());
        let mut total_bound_memory_bytes = 0u64;

        for plan in bind_plans.iter() {
            let allocation_info = vk::MemoryAllocateInfo::builder()
                .allocation_size(plan.size)
                .memory_type_index(plan.selected_memory_type_index);
            let memory =
                unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|err| {
                    format!(
                        "vkAllocateMemory(vulkanalia video session bind {}): {err:?}",
                        plan.memory_bind_index
                    )
                })?;
            allocated_memories.push(memory);
            bind_infos.push(
                vk::BindVideoSessionMemoryInfoKHR::builder()
                    .memory_bind_index(plan.memory_bind_index)
                    .memory(memory)
                    .memory_offset(0)
                    .memory_size(plan.size)
                    .build(),
            );
            total_bound_memory_bytes = total_bound_memory_bytes.saturating_add(plan.size);
        }

        native_vulkan_vulkanalia_bind_video_session_memory(device, session, &bind_infos)?;

        Ok(
            NativeVulkanVulkanaliaVideoSessionMemoryBindingSmokeSnapshot {
                session_created: true,
                memory_bound: true,
                memory_requirements: memory_requirement_snapshots,
                bind_plans,
                total_bound_memory_bytes,
            },
        )
    })();

    native_vulkan_vulkanalia_destroy_video_session(device, session);
    for memory in allocated_memories.drain(..) {
        unsafe {
            device.free_memory(memory, None);
        }
    }

    result
}

pub fn native_vulkan_vulkanalia_memory_type_candidates(
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
) -> Vec<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    let count =
        (memory_properties.memory_type_count as usize).min(memory_properties.memory_types.len());
    memory_properties.memory_types[..count]
        .iter()
        .enumerate()
        .map(
            |(index, memory_type)| NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: index as u32,
                property_flags_bits: memory_type.property_flags.bits(),
            },
        )
        .collect()
}

pub fn native_vulkan_vulkanalia_video_session_memory_requirement_snapshots(
    memory_requirements: &[vk::VideoSessionMemoryRequirementsKHR],
) -> Vec<NativeVulkanVulkanaliaVideoSessionMemoryRequirementSnapshot> {
    memory_requirements
        .iter()
        .map(
            |requirement| NativeVulkanVulkanaliaVideoSessionMemoryRequirementSnapshot {
                memory_bind_index: requirement.memory_bind_index,
                size: requirement.memory_requirements.size,
                alignment: requirement.memory_requirements.alignment,
                memory_type_bits: requirement.memory_requirements.memory_type_bits,
            },
        )
        .collect()
}

pub fn native_vulkan_vulkanalia_video_session_memory_bind_plans(
    memory_requirements: &[vk::VideoSessionMemoryRequirementsKHR],
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
) -> Result<Vec<NativeVulkanVulkanaliaVideoSessionMemoryBindPlan>, String> {
    memory_requirements
        .iter()
        .map(|requirement| {
            native_vulkan_vulkanalia_video_session_memory_bind_plan(requirement, memory_types)
        })
        .collect()
}

fn native_vulkan_vulkanalia_video_session_memory_bind_plan(
    requirement: &vk::VideoSessionMemoryRequirementsKHR,
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
) -> Result<NativeVulkanVulkanaliaVideoSessionMemoryBindPlan, String> {
    let memory_requirements = requirement.memory_requirements;
    if memory_requirements.size == 0 {
        return Err(format!(
            "video session memory bind {} reported zero size",
            requirement.memory_bind_index
        ));
    }

    let selected_memory_type = native_vulkan_vulkanalia_memory_type_index(
        memory_types,
        memory_requirements.memory_type_bits,
        DEVICE_LOCAL_MEMORY_FLAG_BITS,
    )
    .or_else(|| {
        native_vulkan_vulkanalia_memory_type_index(
            memory_types,
            memory_requirements.memory_type_bits,
            0,
        )
    })
    .ok_or_else(|| {
        format!(
            "video session memory bind {} has no compatible memory type for bits 0x{:08x}",
            requirement.memory_bind_index, memory_requirements.memory_type_bits
        )
    })?;
    let preferred_device_local = selected_memory_type.property_flags_bits
        & DEVICE_LOCAL_MEMORY_FLAG_BITS
        == DEVICE_LOCAL_MEMORY_FLAG_BITS;

    Ok(NativeVulkanVulkanaliaVideoSessionMemoryBindPlan {
        memory_bind_index: requirement.memory_bind_index,
        size: memory_requirements.size,
        alignment: memory_requirements.alignment,
        memory_type_bits: memory_requirements.memory_type_bits,
        selected_memory_type_index: selected_memory_type.index,
        selected_memory_property_flags: memory_property_flag_labels(
            selected_memory_type.property_flags_bits,
        ),
        preferred_device_local,
        dedicated_allocation: true,
    })
}

fn native_vulkan_vulkanalia_memory_type_index(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
    required_property_flags_bits: u32,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    memory_types.iter().copied().find(|candidate| {
        let allowed = candidate.index < u32::BITS
            && allowed_memory_type_bits & (1u32 << candidate.index) != 0;
        let properties_match = candidate.property_flags_bits & required_property_flags_bits
            == required_property_flags_bits;
        allowed && properties_match
    })
}

fn memory_property_flag_labels(bits: u32) -> Vec<&'static str> {
    [
        (vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(), "device-local"),
        (vk::MemoryPropertyFlags::HOST_VISIBLE.bits(), "host-visible"),
        (
            vk::MemoryPropertyFlags::HOST_COHERENT.bits(),
            "host-coherent",
        ),
        (vk::MemoryPropertyFlags::HOST_CACHED.bits(), "host-cached"),
        (
            vk::MemoryPropertyFlags::LAZILY_ALLOCATED.bits(),
            "lazily-allocated",
        ),
        (vk::MemoryPropertyFlags::PROTECTED.bits(), "protected"),
    ]
    .into_iter()
    .filter_map(|(flag_bits, label)| (bits & flag_bits == flag_bits).then_some(label))
    .collect()
}

pub fn native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe(
    probe: &NativeVulkanVulkanaliaVideoFormatProbeSnapshot,
) -> Vec<NativeVulkanVulkanaliaVideoSessionResourceProbePlan> {
    probe
        .decode_output_sampled_formats
        .iter()
        .map(|sampled_query| {
            let dpb_query = probe.dpb_formats.iter().find(|candidate| {
                candidate.codec == sampled_query.codec && candidate.profile == sampled_query.profile
            });
            video_session_resource_probe_plan(sampled_query, dpb_query)
        })
        .collect()
}

fn video_session_resource_probe_plan(
    sampled_query: &NativeVulkanVulkanaliaVideoFormatQuerySnapshot,
    dpb_query: Option<&NativeVulkanVulkanaliaVideoFormatQuerySnapshot>,
) -> NativeVulkanVulkanaliaVideoSessionResourceProbePlan {
    let sampled_formats = sampled_query
        .formats
        .iter()
        .map(|format| format.format.as_str())
        .collect::<BTreeSet<_>>();
    let dpb_formats = dpb_query
        .map(|query| {
            query
                .formats
                .iter()
                .map(|format| format.format.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let coincident_format = sampled_formats
        .intersection(&dpb_formats)
        .next()
        .map(|format| (*format).to_owned());
    let sampled_output_ready =
        sampled_query.query_error.is_none() && sampled_query.supported_format_count > 0;
    let dpb_ready = dpb_query
        .map(|query| query.query_error.is_none() && query.supported_format_count > 0)
        .unwrap_or(false);
    let query_error = [
        sampled_query.query_error.as_deref(),
        dpb_query.and_then(|query| query.query_error.as_deref()),
        dpb_query.is_none().then_some("missing DPB format query"),
    ]
    .into_iter()
    .flatten()
    .collect::<BTreeSet<_>>();

    NativeVulkanVulkanaliaVideoSessionResourceProbePlan {
        codec: sampled_query.codec,
        profile: sampled_query.profile,
        sampled_output_format_count: sampled_query.supported_format_count,
        dpb_format_count: dpb_query
            .map(|query| query.supported_format_count)
            .unwrap_or(0),
        coincident_format,
        sampled_output_ready,
        dpb_ready,
        direct_dpb_candidate: sampled_output_ready && dpb_ready,
        query_error: (!query_error.is_empty())
            .then(|| query_error.into_iter().collect::<Vec<_>>().join("; ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::vulkanalia_backend::video_format_probe::{
        NativeVulkanVulkanaliaVideoFormatPropertySnapshot,
        NativeVulkanVulkanaliaVideoFormatQuerySnapshot,
    };
    use vulkanalia::vk::HasBuilder;

    #[test]
    fn video_session_template_names_vulkanalia_video_types_and_steps() {
        let template = native_vulkan_vulkanalia_video_session_template();

        assert_eq!(
            template.boundary,
            "vulkanalia-video-session-resource-ownership"
        );
        assert!(
            template
                .api_type_evidence
                .iter()
                .any(|name| name.ends_with("VideoSessionCreateInfoKHR"))
        );
        assert!(
            template
                .api_type_evidence
                .iter()
                .any(|name| name.ends_with("VideoDecodeInfoKHR"))
        );
        assert_eq!(template.resource_steps.len(), 7);
        assert_eq!(
            template.resource_steps.first().map(|step| step.kind),
            Some(NativeVulkanVulkanaliaVideoSessionResourceStepKind::ProfileFormatSelection)
        );
        assert_eq!(
            template.resource_steps.last().map(|step| step.kind),
            Some(NativeVulkanVulkanaliaVideoSessionResourceStepKind::DecodeSubmit)
        );
    }

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
    fn resource_plans_mark_profiles_with_sampled_output_and_dpb_ready() {
        let probe = NativeVulkanVulkanaliaVideoFormatProbeSnapshot {
            decode_output_sampled_formats: vec![format_query(
                "h265",
                "main-10",
                "video-decode-dst-sampled",
                1028,
                "G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16",
                None,
            )],
            dpb_formats: vec![format_query(
                "h265",
                "main-10",
                "video-decode-dpb",
                4096,
                "G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16",
                None,
            )],
        };

        let plans = native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe(&probe);

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].codec, "h265");
        assert_eq!(plans[0].profile, "main-10");
        assert_eq!(plans[0].sampled_output_format_count, 1);
        assert_eq!(plans[0].dpb_format_count, 1);
        assert_eq!(
            plans[0].coincident_format.as_deref(),
            Some("G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16")
        );
        assert!(plans[0].direct_dpb_candidate);
        assert_eq!(plans[0].query_error, None);
    }

    #[test]
    fn resource_plans_preserve_query_errors() {
        let probe = NativeVulkanVulkanaliaVideoFormatProbeSnapshot {
            decode_output_sampled_formats: vec![format_query(
                "av1",
                "main-10",
                "video-decode-dst-sampled",
                1028,
                "",
                Some("missing VK_KHR_video_decode_av1"),
            )],
            dpb_formats: Vec::new(),
        };

        let plans = native_vulkan_vulkanalia_video_session_resource_plans_from_format_probe(&probe);

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].sampled_output_format_count, 0);
        assert_eq!(plans[0].dpb_format_count, 0);
        assert!(!plans[0].direct_dpb_candidate);
        let error = plans[0].query_error.as_deref().expect("query error");
        assert!(error.contains("missing VK_KHR_video_decode_av1"));
        assert!(error.contains("missing DPB format query"));
    }

    #[test]
    fn memory_requirement_snapshots_preserve_bind_indices_and_sizes() {
        let requirements = vec![memory_requirement(3, 4096, 256, 0b1010)];

        let snapshots =
            native_vulkan_vulkanalia_video_session_memory_requirement_snapshots(&requirements);

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].memory_bind_index, 3);
        assert_eq!(snapshots[0].size, 4096);
        assert_eq!(snapshots[0].alignment, 256);
        assert_eq!(snapshots[0].memory_type_bits, 0b1010);
    }

    #[test]
    fn memory_bind_plans_prefer_device_local_types() {
        let requirements = vec![memory_requirement(0, 8192, 512, 0b111)];
        let memory_types = vec![
            memory_type_candidate(0, vk::MemoryPropertyFlags::HOST_VISIBLE),
            memory_type_candidate(1, vk::MemoryPropertyFlags::DEVICE_LOCAL),
            memory_type_candidate(
                2,
                vk::MemoryPropertyFlags::DEVICE_LOCAL | vk::MemoryPropertyFlags::HOST_VISIBLE,
            ),
        ];

        let plans =
            native_vulkan_vulkanalia_video_session_memory_bind_plans(&requirements, &memory_types)
                .expect("bind plans");

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].selected_memory_type_index, 1);
        assert!(plans[0].preferred_device_local);
        assert!(plans[0].dedicated_allocation);
        assert!(
            plans[0]
                .selected_memory_property_flags
                .contains(&"device-local")
        );
    }

    #[test]
    fn memory_bind_plans_fall_back_to_any_compatible_type() {
        let requirements = vec![memory_requirement(1, 4096, 128, 0b010)];
        let memory_types = vec![
            memory_type_candidate(0, vk::MemoryPropertyFlags::DEVICE_LOCAL),
            memory_type_candidate(1, vk::MemoryPropertyFlags::HOST_VISIBLE),
        ];

        let plans =
            native_vulkan_vulkanalia_video_session_memory_bind_plans(&requirements, &memory_types)
                .expect("bind plans");

        assert_eq!(plans[0].selected_memory_type_index, 1);
        assert!(!plans[0].preferred_device_local);
        assert!(
            plans[0]
                .selected_memory_property_flags
                .contains(&"host-visible")
        );
    }

    #[test]
    fn memory_bind_plans_reject_zero_size_and_missing_memory_type() {
        let memory_types = vec![memory_type_candidate(
            0,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )];

        let zero_size = native_vulkan_vulkanalia_video_session_memory_bind_plans(
            &[memory_requirement(2, 0, 64, 0b001)],
            &memory_types,
        )
        .expect_err("zero size must fail");
        assert!(zero_size.contains("reported zero size"));

        let missing_type = native_vulkan_vulkanalia_video_session_memory_bind_plans(
            &[memory_requirement(3, 4096, 64, 0b010)],
            &memory_types,
        )
        .expect_err("missing type must fail");
        assert!(missing_type.contains("no compatible memory type"));
    }

    #[test]
    fn memory_type_candidates_read_physical_memory_properties() {
        let mut properties = vk::PhysicalDeviceMemoryProperties {
            memory_type_count: 2,
            ..Default::default()
        };
        properties.memory_types[0] = vk::MemoryType {
            property_flags: vk::MemoryPropertyFlags::HOST_VISIBLE,
            heap_index: 0,
        };
        properties.memory_types[1] = vk::MemoryType {
            property_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            heap_index: 0,
        };

        let candidates = native_vulkan_vulkanalia_memory_type_candidates(&properties);

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].index, 0);
        assert_eq!(
            candidates[1].property_flags_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL.bits()
        );
    }

    fn format_query(
        codec: &'static str,
        profile: &'static str,
        image_usage: &'static str,
        image_usage_bits: u32,
        format: &'static str,
        query_error: Option<&'static str>,
    ) -> NativeVulkanVulkanaliaVideoFormatQuerySnapshot {
        let formats = query_error
            .is_none()
            .then(|| {
                vec![NativeVulkanVulkanaliaVideoFormatPropertySnapshot {
                    format: format.to_owned(),
                    image_type: "TYPE_2D".to_owned(),
                    image_tiling: "OPTIMAL".to_owned(),
                    image_create_flags: Vec::new(),
                    image_create_flag_bits: 0,
                    image_usage_flags: Vec::new(),
                    image_usage_flag_bits: image_usage_bits,
                    component_mapping: "identity".to_owned(),
                }]
            })
            .unwrap_or_default();
        NativeVulkanVulkanaliaVideoFormatQuerySnapshot {
            codec,
            profile,
            image_usage,
            image_usage_bits,
            supported_format_count: formats.len(),
            formats,
            query_error: query_error.map(str::to_owned),
        }
    }

    fn memory_requirement(
        memory_bind_index: u32,
        size: u64,
        alignment: u64,
        memory_type_bits: u32,
    ) -> vk::VideoSessionMemoryRequirementsKHR {
        vk::VideoSessionMemoryRequirementsKHR::builder()
            .memory_bind_index(memory_bind_index)
            .memory_requirements(
                vk::MemoryRequirements::builder()
                    .size(size)
                    .alignment(alignment)
                    .memory_type_bits(memory_type_bits),
            )
            .build()
    }

    fn memory_type_candidate(
        index: u32,
        property_flags: vk::MemoryPropertyFlags,
    ) -> NativeVulkanVulkanaliaMemoryTypeCandidate {
        NativeVulkanVulkanaliaMemoryTypeCandidate {
            index,
            property_flags_bits: property_flags.bits(),
        }
    }
}
