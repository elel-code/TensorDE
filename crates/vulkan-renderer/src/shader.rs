use std::fmt;
use std::sync::Arc;

use vulkanalia::{prelude::v1_4::*, vk};

use crate::Backend;
use crate::backend::DeviceOwner;

const SPIRV_MAGIC: u32 = 0x0723_0203;

/// Owned SPIR-V shader-module descriptor.
///
/// Keeping words owned avoids retaining caller byte buffers and guarantees the
/// alignment required by `VkShaderModuleCreateInfo::pCode`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderModuleDescriptor {
    pub label: Option<String>,
    pub spirv: Vec<u32>,
}

impl ShaderModuleDescriptor {
    pub fn validate(&self) -> Result<(), SpirvValidationError> {
        validate_spirv(&self.spirv)
    }
}

/// Vulkan shader module with shared logical-device ownership.
pub struct ShaderModule {
    owner: Arc<DeviceOwner>,
    raw: vk::ShaderModule,
    label: Option<String>,
}

impl ShaderModule {
    pub const fn raw(&self) -> vk::ShaderModule {
        self.raw
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }
}

impl fmt::Debug for ShaderModule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShaderModule")
            .field("raw", &self.raw)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_shader_module(self.raw, None) };
    }
}

impl Backend {
    /// Validates SPIR-V before calling Vulkan. The module owns a device
    /// reference and may outlive the `Device` handle used to create it.
    pub fn create_shader_module(
        &self,
        descriptor: ShaderModuleDescriptor,
    ) -> Result<ShaderModule, ShaderModuleError> {
        descriptor
            .validate()
            .map_err(ShaderModuleError::InvalidSpirv)?;
        let info = shader_module_create_info(&descriptor.spirv);
        let owner = self.shared_owner();
        let raw = unsafe { owner.device.create_shader_module(&info, None) }
            .map_err(ShaderModuleError::Vulkan)?;
        Ok(ShaderModule {
            owner,
            raw,
            label: descriptor.label,
        })
    }
}

fn shader_module_create_info(words: &[u32]) -> vk::ShaderModuleCreateInfo {
    vk::ShaderModuleCreateInfo::builder()
        .code(words)
        // vulkanalia's builder does not derive Vulkan's byte count from the
        // u32 slice, so this field must be set explicitly.
        .code_size(std::mem::size_of_val(words))
        .build()
}

fn validate_spirv(words: &[u32]) -> Result<(), SpirvValidationError> {
    if words.len() < 5 {
        return Err(SpirvValidationError::HeaderTooShort(words.len()));
    }
    if words[0] != SPIRV_MAGIC {
        return Err(SpirvValidationError::InvalidMagic(words[0]));
    }
    let version = words[1];
    let major = (version >> 16) & 0xff;
    let minor = (version >> 8) & 0xff;
    if major != 1 || minor > 6 || version & 0xff != 0 {
        return Err(SpirvValidationError::UnsupportedVersion(version));
    }
    if words[3] == 0 {
        return Err(SpirvValidationError::ZeroIdBound);
    }
    if words[4] != 0 {
        return Err(SpirvValidationError::NonZeroSchema(words[4]));
    }

    let mut offset = 5usize;
    while offset < words.len() {
        let instruction_words = (words[offset] >> 16) as usize;
        if instruction_words == 0 {
            return Err(SpirvValidationError::ZeroInstructionLength { offset });
        }
        offset = offset
            .checked_add(instruction_words)
            .ok_or(SpirvValidationError::InstructionOverflow { offset })?;
        if offset > words.len() {
            return Err(SpirvValidationError::TruncatedInstruction {
                offset: offset - instruction_words,
                declared_words: instruction_words,
                remaining_words: words.len() - (offset - instruction_words),
            });
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpirvValidationError {
    HeaderTooShort(usize),
    InvalidMagic(u32),
    UnsupportedVersion(u32),
    ZeroIdBound,
    NonZeroSchema(u32),
    ZeroInstructionLength {
        offset: usize,
    },
    InstructionOverflow {
        offset: usize,
    },
    TruncatedInstruction {
        offset: usize,
        declared_words: usize,
        remaining_words: usize,
    },
}

impl fmt::Display for SpirvValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SpirvValidationError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderModuleError {
    InvalidSpirv(SpirvValidationError),
    Vulkan(vk::ErrorCode),
}

impl fmt::Display for ShaderModuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ShaderModuleError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Vec<u32> {
        vec![SPIRV_MAGIC, 0x0001_0600, 0, 1, 0]
    }

    #[test]
    fn valid_header_and_instruction_stream_is_accepted() {
        let mut spirv = header();
        spirv.extend([0x0002_0000, 0]);
        assert_eq!(validate_spirv(&spirv), Ok(()));
    }

    #[test]
    fn malformed_instruction_length_is_rejected_before_vulkan() {
        let mut zero = header();
        zero.push(1);
        assert_eq!(
            validate_spirv(&zero),
            Err(SpirvValidationError::ZeroInstructionLength { offset: 5 })
        );

        let mut truncated = header();
        truncated.extend([0x0003_0000, 0]);
        assert_eq!(
            validate_spirv(&truncated),
            Err(SpirvValidationError::TruncatedInstruction {
                offset: 5,
                declared_words: 3,
                remaining_words: 2,
            })
        );
    }

    #[test]
    fn unsupported_spirv_version_and_schema_are_rejected() {
        let mut future = header();
        future[1] = 0x0001_0700;
        assert!(matches!(
            validate_spirv(&future),
            Err(SpirvValidationError::UnsupportedVersion(_))
        ));
        let mut schema = header();
        schema[4] = 1;
        assert_eq!(
            validate_spirv(&schema),
            Err(SpirvValidationError::NonZeroSchema(1))
        );
    }

    #[test]
    fn vulkan_shader_code_size_is_bytes_not_words() {
        let words = header();
        let info = shader_module_create_info(&words);
        assert_eq!(info.code_size, words.len() * std::mem::size_of::<u32>());
        assert_eq!(info.code, words.as_ptr());
    }
}
