use std::collections::BTreeSet;
use std::ptr;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::video_format_probe::{
    NativeVulkanVulkanaliaVideoFormatProbeSnapshot, NativeVulkanVulkanaliaVideoFormatQuerySnapshot,
};

const DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();

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
