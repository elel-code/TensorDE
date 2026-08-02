use std::collections::BTreeSet;

use vulkanalia::{prelude::v1_4::*, vk};

use super::RenderingEncoder;
use crate::{Backend, Error, Features, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderingLocalReadMappingKind {
    OutputOnly,
    InputAttachment,
}

#[derive(Clone, Copy, Debug)]
pub struct RenderingLocalReadMappingDescriptor<'a> {
    /// One shader color-output location per dynamic-rendering color attachment.
    /// `None` maps the attachment to `VK_ATTACHMENT_UNUSED`.
    pub color_attachment_locations: &'a [Option<u32>],
    /// One fragment input-attachment index per color attachment. `None` means
    /// the attachment is not read as an input attachment in this mapping.
    pub color_attachment_input_indices: &'a [Option<u32>],
    pub kind: RenderingLocalReadMappingKind,
}

/// Validated dynamic-rendering attachment mapping created for one Device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderingLocalReadMapping {
    color_attachment_locations: Vec<u32>,
    color_attachment_input_indices: Vec<u32>,
}

impl Backend {
    pub fn create_rendering_local_read_mapping(
        &self,
        descriptor: RenderingLocalReadMappingDescriptor<'_>,
    ) -> Result<RenderingLocalReadMapping> {
        if !self
            .features()
            .contains(Features::DYNAMIC_RENDERING_LOCAL_READ)
        {
            return Err(Error::Validation(
                "local-read mapping requires enabled DYNAMIC_RENDERING_LOCAL_READ".into(),
            ));
        }
        RenderingLocalReadMapping::from_limits(
            descriptor,
            self.device_info().limits.max_color_attachments,
            self.device_info()
                .limits
                .max_per_stage_descriptor_input_attachments,
        )
    }
}

impl RenderingLocalReadMapping {
    fn from_limits(
        descriptor: RenderingLocalReadMappingDescriptor<'_>,
        max_color_attachments: u32,
        max_input_attachments: u32,
    ) -> Result<Self> {
        let locations = descriptor.color_attachment_locations;
        let input_indices = descriptor.color_attachment_input_indices;
        if locations.is_empty() || locations.len() != input_indices.len() {
            return Err(Error::Validation(
                "local-read location/input-index arrays must have equal non-zero lengths".into(),
            ));
        }
        let count = u32::try_from(locations.len()).map_err(|_| {
            Error::Validation("local-read color attachment count exceeds u32".into())
        })?;
        if count > max_color_attachments {
            return Err(Error::Validation(format!(
                "local-read color attachment count {count} exceeds device limit {max_color_attachments}"
            )));
        }
        validate_values(
            locations,
            max_color_attachments,
            "color attachment location",
        )?;
        validate_values(
            input_indices,
            max_input_attachments,
            "input attachment index",
        )?;
        let has_input = input_indices.iter().any(Option::is_some);
        match descriptor.kind {
            RenderingLocalReadMappingKind::OutputOnly if has_input => {
                return Err(Error::Validation(
                    "output-only local-read mapping declares an input attachment".into(),
                ));
            }
            RenderingLocalReadMappingKind::InputAttachment if !has_input => {
                return Err(Error::Validation(
                    "input local-read mapping declares no input attachment".into(),
                ));
            }
            _ => {}
        }
        Ok(Self {
            color_attachment_locations: lower_values(locations),
            color_attachment_input_indices: lower_values(input_indices),
        })
    }

    pub fn color_attachment_count(&self) -> usize {
        self.color_attachment_locations.len()
    }

    pub(crate) fn attachment_location_info(&self) -> vk::RenderingAttachmentLocationInfo {
        vk::RenderingAttachmentLocationInfo::builder()
            .color_attachment_locations(&self.color_attachment_locations)
            .build()
    }

    pub(crate) fn input_attachment_index_info(&self) -> vk::RenderingInputAttachmentIndexInfo {
        vk::RenderingInputAttachmentIndexInfo::builder()
            .color_attachment_input_indices(&self.color_attachment_input_indices)
            .build()
    }

    pub(crate) fn validate_for_device(
        &self,
        features: Features,
        max_color_attachments: u32,
        max_input_attachments: u32,
    ) -> Result<()> {
        if !features.contains(Features::DYNAMIC_RENDERING_LOCAL_READ) {
            return Err(Error::Validation(
                "local-read mapping requires enabled DYNAMIC_RENDERING_LOCAL_READ".into(),
            ));
        }
        let count = u32::try_from(self.color_attachment_locations.len()).map_err(|_| {
            Error::Validation("local-read color attachment count exceeds u32".into())
        })?;
        if count > max_color_attachments {
            return Err(Error::Validation(format!(
                "local-read color attachment count {count} exceeds device limit {max_color_attachments}"
            )));
        }
        validate_lowered_values(
            &self.color_attachment_locations,
            max_color_attachments,
            "color attachment location",
        )?;
        validate_lowered_values(
            &self.color_attachment_input_indices,
            max_input_attachments,
            "input attachment index",
        )
    }
}

impl RenderingEncoder<'_> {
    /// Records Vulkan 1.4 dynamic-rendering local-read mappings for this scope.
    pub fn set_local_read_mapping(&mut self, mapping: &RenderingLocalReadMapping) -> Result<()> {
        mapping.validate_for_device(
            self.encoder.owner.enabled_features,
            self.encoder.owner.limits.max_color_attachments,
            self.encoder
                .owner
                .limits
                .max_per_stage_descriptor_input_attachments,
        )?;
        if mapping.color_attachment_count() != self.color_formats.len() {
            return Err(Error::Validation(format!(
                "local-read mapping has {} color attachments but the rendering scope has {}",
                mapping.color_attachment_count(),
                self.color_formats.len()
            )));
        }
        let locations = mapping.attachment_location_info();
        let input_indices = mapping.input_attachment_index_info();
        unsafe {
            self.encoder
                .owner
                .device
                .cmd_set_rendering_attachment_locations(self.encoder.raw(), &locations);
            self.encoder
                .owner
                .device
                .cmd_set_rendering_input_attachment_indices(self.encoder.raw(), &input_indices);
        }
        Ok(())
    }
}

impl RenderingEncoder<'_> {
    /// Separates an authored color-attachment producer from a fragment
    /// input-attachment consumer inside one dynamic-rendering scope.
    pub fn local_read_by_region_dependency(&mut self) -> Result<()> {
        if !self
            .encoder
            .owner
            .enabled_features
            .contains(Features::DYNAMIC_RENDERING_LOCAL_READ)
        {
            return Err(Error::Validation(
                "local-read dependency requires enabled DYNAMIC_RENDERING_LOCAL_READ".into(),
            ));
        }
        let barriers = [vk::MemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::INPUT_ATTACHMENT_READ)
            .build()];
        let dependency = vk::DependencyInfo::builder()
            .dependency_flags(vk::DependencyFlags::BY_REGION)
            .memory_barriers(&barriers)
            .build();
        unsafe {
            self.encoder
                .owner
                .device
                .cmd_pipeline_barrier2(self.encoder.raw(), &dependency);
        }
        Ok(())
    }
}

fn validate_values(values: &[Option<u32>], upper_bound: u32, label: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values.iter().flatten() {
        if *value >= upper_bound {
            return Err(Error::Validation(format!(
                "local-read {label} {value} exceeds device limit {upper_bound}"
            )));
        }
        if !seen.insert(*value) {
            return Err(Error::Validation(format!(
                "local-read {label} {value} is duplicated"
            )));
        }
    }
    Ok(())
}

fn lower_values(values: &[Option<u32>]) -> Vec<u32> {
    values
        .iter()
        .map(|value| value.unwrap_or(vk::ATTACHMENT_UNUSED))
        .collect()
}

fn validate_lowered_values(values: &[u32], upper_bound: u32, label: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if *value == vk::ATTACHMENT_UNUSED {
            continue;
        }
        if *value >= upper_bound {
            return Err(Error::Validation(format!(
                "local-read {label} {value} exceeds device limit {upper_bound}"
            )));
        }
        if !seen.insert(*value) {
            return Err(Error::Validation(format!(
                "local-read {label} {value} is duplicated"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn producer_and_consumer_mappings_preserve_unused_attachment_slots() {
        let producer = RenderingLocalReadMapping::from_limits(
            RenderingLocalReadMappingDescriptor {
                color_attachment_locations: &[Some(0), None],
                color_attachment_input_indices: &[None, None],
                kind: RenderingLocalReadMappingKind::OutputOnly,
            },
            8,
            8,
        )
        .unwrap();
        assert_eq!(
            producer.color_attachment_locations,
            [0, vk::ATTACHMENT_UNUSED]
        );

        let consumer = RenderingLocalReadMapping::from_limits(
            RenderingLocalReadMappingDescriptor {
                color_attachment_locations: &[None, Some(0)],
                color_attachment_input_indices: &[Some(2), None],
                kind: RenderingLocalReadMappingKind::InputAttachment,
            },
            8,
            8,
        )
        .unwrap();
        assert_eq!(
            consumer.color_attachment_input_indices,
            [2, vk::ATTACHMENT_UNUSED]
        );
    }

    #[test]
    fn mappings_reject_missing_inputs_duplicates_and_device_limit_overflow() {
        let missing = RenderingLocalReadMappingDescriptor {
            color_attachment_locations: &[Some(0)],
            color_attachment_input_indices: &[None],
            kind: RenderingLocalReadMappingKind::InputAttachment,
        };
        assert!(RenderingLocalReadMapping::from_limits(missing, 8, 8).is_err());

        let duplicate = RenderingLocalReadMappingDescriptor {
            color_attachment_locations: &[Some(0), Some(0)],
            color_attachment_input_indices: &[None, None],
            kind: RenderingLocalReadMappingKind::OutputOnly,
        };
        assert!(RenderingLocalReadMapping::from_limits(duplicate, 8, 8).is_err());

        let overflow = RenderingLocalReadMappingDescriptor {
            color_attachment_locations: &[Some(4)],
            color_attachment_input_indices: &[None],
            kind: RenderingLocalReadMappingKind::OutputOnly,
        };
        assert!(RenderingLocalReadMapping::from_limits(overflow, 4, 8).is_err());
    }
}
