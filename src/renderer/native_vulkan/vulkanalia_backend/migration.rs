use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanVulkanaliaMigrationStageKind {
    InstanceDevice,
    SurfaceSwapchain,
    VideoFormatCapabilities,
    VideoSessionResources,
    CodecSubmit,
    DirectVideoRuntime,
    ExternalMemoryImport,
    RenderPresent,
    AshRemoval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaMigrationStage {
    pub order: u8,
    pub kind: NativeVulkanVulkanaliaMigrationStageKind,
    pub ash_boundary: &'static str,
    pub vulkanalia_target_module: &'static str,
    pub validation_gate: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaMigrationContract {
    pub primary_binding: &'static str,
    pub compatibility_binding: &'static str,
    pub split_rule: &'static str,
    pub native_vulkan_rs_rule: &'static str,
    pub stages: Vec<NativeVulkanVulkanaliaMigrationStage>,
}

pub fn native_vulkan_vulkanalia_migration_contract() -> NativeVulkanVulkanaliaMigrationContract {
    NativeVulkanVulkanaliaMigrationContract {
        primary_binding: "vulkanalia",
        compatibility_binding: "ash",
        split_rule: "new Vulkanalia ownership code lives under src/renderer/native_vulkan/vulkanalia_backend/ and is split by boundary",
        native_vulkan_rs_rule: "native_vulkan.rs may keep compatibility call sites during migration but must not gain new large Vulkanalia implementations",
        stages: vec![
            NativeVulkanVulkanaliaMigrationStage {
                order: 0,
                kind: NativeVulkanVulkanaliaMigrationStageKind::InstanceDevice,
                ash_boundary: "Entry/Instance/PhysicalDevice/Device creation and extension feature chains",
                vulkanalia_target_module: "vulkanalia_backend/device.rs",
                validation_gate: "ordinary native-vulkan-renderer build exposes Vulkan 1.4 feature and extension telemetry without native-vulkan-vulkanalia",
            },
            NativeVulkanVulkanaliaMigrationStage {
                order: 1,
                kind: NativeVulkanVulkanaliaMigrationStageKind::SurfaceSwapchain,
                ash_boundary: "Wayland surface, swapchain creation, image acquisition and resize recovery",
                vulkanalia_target_module: "vulkanalia_backend/swapchain.rs",
                validation_gate: "probe surface/swapchain parity with existing ash path on all outputs",
            },
            NativeVulkanVulkanaliaMigrationStage {
                order: 2,
                kind: NativeVulkanVulkanaliaMigrationStageKind::VideoFormatCapabilities,
                ash_boundary: "Vulkan Video profile, format and DPB/output image capability discovery",
                vulkanalia_target_module: "vulkanalia_backend/video_format_probe.rs",
                validation_gate: "H.264, H.265 main8/main10 and AV1 main8/main10 profile+format support is reported per device",
            },
            NativeVulkanVulkanaliaMigrationStage {
                order: 3,
                kind: NativeVulkanVulkanaliaMigrationStageKind::VideoSessionResources,
                ash_boundary: "video session, session parameters, DPB/output images and bitstream ring resources",
                vulkanalia_target_module: "vulkanalia_backend/video_session.rs",
                validation_gate: "Vulkanalia-created resources can run the existing ready-prefix smoke paths without new copies",
            },
            NativeVulkanVulkanaliaMigrationStage {
                order: 4,
                kind: NativeVulkanVulkanaliaMigrationStageKind::CodecSubmit,
                ash_boundary: "H.264/H.265/AV1 picture info, reference lists and decode submit command recording",
                vulkanalia_target_module: "vulkanalia_backend/video_decode_submit.rs",
                validation_gate: "codec submit snapshots match the existing ash path for continuous real-source playback",
            },
            NativeVulkanVulkanaliaMigrationStage {
                order: 5,
                kind: NativeVulkanVulkanaliaMigrationStageKind::DirectVideoRuntime,
                ash_boundary: "continuous direct-video runtime ownership of session images, bitstream upload, command buffers, submit completion and display handoff",
                vulkanalia_target_module: "vulkanalia_backend/video_direct_runtime.rs",
                validation_gate: "H.264, H.265 main8/main10 and AV1 main8/main10 direct-video runs consume Vulkanalia-owned resources and submit with QueueSubmit2",
            },
            NativeVulkanVulkanaliaMigrationStage {
                order: 6,
                kind: NativeVulkanVulkanaliaMigrationStageKind::ExternalMemoryImport,
                ash_boundary: "DMABuf/DRM modifier import, external semaphore fd import and memory type selection",
                vulkanalia_target_module: "vulkanalia_backend/external_memory.rs",
                validation_gate: "gst-dma decoded-frame route reports import/display handoff without CPU frame copies when the driver supports it",
            },
            NativeVulkanVulkanaliaMigrationStage {
                order: 7,
                kind: NativeVulkanVulkanaliaMigrationStageKind::RenderPresent,
                ash_boundary: "render pass/dynamic rendering, synchronization, queue submit and present timing",
                vulkanalia_target_module: "vulkanalia_backend/render_present.rs",
                validation_gate: "present pacing telemetry stays at least equal to ash on 240 Hz video and scene-lite runs",
            },
            NativeVulkanVulkanaliaMigrationStage {
                order: 8,
                kind: NativeVulkanVulkanaliaMigrationStageKind::AshRemoval,
                ash_boundary: "ash dependency and compatibility shims",
                vulkanalia_target_module: "native-vulkan-renderer Cargo feature",
                validation_gate: "all native Vulkan renderer tests, probe CLIs and real-source smoke scripts pass without dep:ash",
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_contract_makes_vulkanalia_primary() {
        let contract = native_vulkan_vulkanalia_migration_contract();

        assert_eq!(contract.primary_binding, "vulkanalia");
        assert_eq!(contract.compatibility_binding, "ash");
        assert!(contract.split_rule.contains("vulkanalia_backend"));
        assert!(contract.native_vulkan_rs_rule.contains("must not gain"));
        assert_eq!(contract.stages.len(), 9);
        assert_eq!(
            contract.stages.first().map(|stage| stage.kind),
            Some(NativeVulkanVulkanaliaMigrationStageKind::InstanceDevice)
        );
        assert_eq!(
            contract.stages.last().map(|stage| stage.kind),
            Some(NativeVulkanVulkanaliaMigrationStageKind::AshRemoval)
        );
    }

    #[test]
    fn migration_contract_keeps_video_and_zero_copy_gates_explicit() {
        let contract = native_vulkan_vulkanalia_migration_contract();

        assert!(contract.stages.iter().any(|stage| {
            stage.kind == NativeVulkanVulkanaliaMigrationStageKind::VideoFormatCapabilities
                && stage
                    .validation_gate
                    .contains("H.264, H.265 main8/main10 and AV1 main8/main10")
        }));
        assert!(contract.stages.iter().any(|stage| {
            stage.kind == NativeVulkanVulkanaliaMigrationStageKind::DirectVideoRuntime
                && stage.validation_gate.contains("QueueSubmit2")
        }));
        assert!(contract.stages.iter().any(|stage| {
            stage.kind == NativeVulkanVulkanaliaMigrationStageKind::ExternalMemoryImport
                && stage.validation_gate.contains("without CPU frame copies")
        }));
        assert!(contract.stages.iter().any(|stage| {
            stage.kind == NativeVulkanVulkanaliaMigrationStageKind::RenderPresent
                && stage.validation_gate.contains("240 Hz")
        }));
    }
}
