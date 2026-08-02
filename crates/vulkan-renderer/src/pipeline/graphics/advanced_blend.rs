use vulkanalia::vk;

use super::GraphicsPipelineDescriptor;
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlendOverlap {
    Uncorrelated,
    Disjoint,
    Conjoint,
}

impl BlendOverlap {
    pub(super) const fn to_vk(self) -> vk::BlendOverlapEXT {
        match self {
            Self::Uncorrelated => vk::BlendOverlapEXT::UNCORRELATED,
            Self::Disjoint => vk::BlendOverlapEXT::DISJOINT,
            Self::Conjoint => vk::BlendOverlapEXT::CONJOINT,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdvancedBlendState {
    pub source_premultiplied: bool,
    pub destination_premultiplied: bool,
    pub overlap: BlendOverlap,
}

pub(super) fn validate_advanced_blend(descriptor: &GraphicsPipelineDescriptor<'_>) -> Result<()> {
    let uses_advanced_operation = descriptor
        .fragment
        .targets
        .iter()
        .flatten()
        .filter_map(|target| target.blend)
        .flat_map(|blend| [blend.color.operation, blend.alpha.operation])
        .any(super::BlendOperation::is_advanced);
    match (uses_advanced_operation, descriptor.advanced_blend) {
        (true, None) => Err(Error::Validation(
            "advanced blend operations require AdvancedBlendState".into(),
        )),
        (false, Some(_)) => Err(Error::Validation(
            "AdvancedBlendState requires at least one advanced blend operation".into(),
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn core_blend_operations_do_not_require_advanced_state() {
        for operation in [
            super::super::BlendOperation::Add,
            super::super::BlendOperation::Subtract,
            super::super::BlendOperation::ReverseSubtract,
            super::super::BlendOperation::Minimum,
            super::super::BlendOperation::Maximum,
        ] {
            assert!(!operation.is_advanced());
        }
    }

    #[test]
    fn scene_composite_operations_require_advanced_state() {
        for operation in [
            super::super::BlendOperation::Multiply,
            super::super::BlendOperation::Screen,
            super::super::BlendOperation::HslColor,
        ] {
            assert!(operation.is_advanced());
        }
    }
}
