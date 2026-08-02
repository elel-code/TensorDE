use std::fmt;
use std::sync::Arc;

use vulkanalia::{
    Device,
    prelude::v1_4::*,
    vk::{self, KhrPipelineBinaryExtensionDeviceCommands},
};

use crate::backend::DeviceOwner;
use crate::{Backend, Error, Features, Result};

/// One implementation-defined GPU machine-code payload and its Vulkan key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipelineBinaryBlob {
    pub key: Vec<u8>,
    pub data: Vec<u8>,
}

/// The ordered binary set required to recreate one pipeline without compiling.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PipelineBinaryArchive {
    pub binaries: Vec<PipelineBinaryBlob>,
}

/// Pipeline creation phase selected by [`create_graphics_pipeline_machine_code`].
#[derive(Clone, Copy, Debug)]
pub enum PipelineBinaryCreation<'a> {
    /// Compile and retain capturable implementation data.
    Capture,
    /// Create only from an exact ordered set of device-native binaries.
    Ready(&'a [vk::PipelineBinaryKHR]),
}

impl<'a> PipelineBinaryCreation<'a> {
    pub const fn flags(self) -> vk::PipelineCreateFlags2 {
        match self {
            Self::Capture => vk::PipelineCreateFlags2::CAPTURE_DATA_KHR,
            // VK_KHR_pipeline_binary forbids FAIL_ON_PIPELINE_COMPILE_REQUIRED
            // when VkPipelineBinaryInfoKHR supplies one or more binaries. The
            // binaries themselves are the strict no-compile contract.
            Self::Ready(_) => vk::PipelineCreateFlags2::empty(),
        }
    }

    pub fn ready_binaries(self) -> Option<&'a [vk::PipelineBinaryKHR]> {
        match self {
            Self::Capture => None,
            Self::Ready(binaries) => Some(binaries),
        }
    }
}

/// Retained pipeline whose driver machine code was materialized before use.
pub struct MachineCodePipeline {
    inner: Arc<MachineCodePipelineInner>,
    archive: PipelineBinaryArchive,
    archive_reused: bool,
    kind: MachineCodePipelineKind,
}

#[derive(Clone, Debug)]
enum MachineCodePipelineKind {
    Unspecified,
    Graphics(MachineCodeGraphicsFacts),
    Compute,
}

#[derive(Clone, Debug)]
pub(crate) struct MachineCodeGraphicsFacts {
    pub(crate) color_formats: Vec<Option<crate::TextureFormat>>,
    pub(crate) depth_format: vk::Format,
    pub(crate) stencil_format: vk::Format,
    pub(crate) sample_count: crate::SampleCount,
    pub(crate) vertex_buffer_slots: Vec<u32>,
}

struct MachineCodePipelineInner {
    owner: Arc<DeviceOwner>,
    pipeline: vk::Pipeline,
}

impl fmt::Debug for MachineCodePipeline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MachineCodePipeline")
            .field("pipeline", &self.inner.pipeline)
            .field("binary_count", &self.archive.binaries.len())
            .field("archive_reused", &self.archive_reused)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl MachineCodePipeline {
    pub fn raw(&self) -> vk::Pipeline {
        self.inner.pipeline
    }

    pub const fn archive(&self) -> &PipelineBinaryArchive {
        &self.archive
    }

    pub const fn archive_reused(&self) -> bool {
        self.archive_reused
    }

    pub(crate) fn belongs_to(&self, owner: &Arc<DeviceOwner>) -> bool {
        Arc::ptr_eq(&self.inner.owner, owner)
    }

    pub(crate) fn mark_graphics(&mut self, facts: MachineCodeGraphicsFacts) {
        self.kind = MachineCodePipelineKind::Graphics(facts);
    }

    pub(crate) fn mark_compute(&mut self) {
        self.kind = MachineCodePipelineKind::Compute;
    }

    pub(crate) fn graphics_facts(&self) -> Option<&MachineCodeGraphicsFacts> {
        match &self.kind {
            MachineCodePipelineKind::Graphics(facts) => Some(facts),
            MachineCodePipelineKind::Unspecified | MachineCodePipelineKind::Compute => None,
        }
    }

    pub(crate) fn is_compute(&self) -> bool {
        matches!(self.kind, MachineCodePipelineKind::Compute)
    }
}

impl crate::SubmissionResource for MachineCodePipeline {
    fn submission_lease(&self) -> crate::SubmissionLease {
        crate::SubmissionLease::new(Arc::clone(&self.inner))
    }
}

impl Drop for MachineCodePipelineInner {
    fn drop(&mut self) {
        unsafe { self.owner.device.destroy_pipeline(self.pipeline, None) };
    }
}

pub type MachineCodeGraphicsPipeline = MachineCodePipeline;
pub type MachineCodeComputePipeline = MachineCodePipeline;

/// Returns the implementation's complete key for one graphics create info.
///
/// # Safety
///
/// Every pointer reachable from `create_info` must remain valid for the
/// duration of this call and every referenced Vulkan object must belong to
/// `backend`.
pub unsafe fn graphics_pipeline_binary_key(
    backend: &Backend,
    create_info: &vk::GraphicsPipelineCreateInfo,
) -> Result<Vec<u8>> {
    require_pipeline_binaries(backend)?;
    unsafe {
        pipeline_binary_key(
            backend.device(),
            create_info as *const _ as *mut std::ffi::c_void,
        )
    }
}

/// Returns the implementation's complete key for one compute create info.
///
/// # Safety
///
/// Every pointer reachable from `create_info` must remain valid for the
/// duration of this call and every referenced Vulkan object must belong to
/// `backend`.
pub unsafe fn compute_pipeline_binary_key(
    backend: &Backend,
    create_info: &vk::ComputePipelineCreateInfo,
) -> Result<Vec<u8>> {
    require_pipeline_binaries(backend)?;
    unsafe {
        pipeline_binary_key(
            backend.device(),
            create_info as *const _ as *mut std::ffi::c_void,
        )
    }
}

unsafe fn pipeline_binary_key(
    device: &Device,
    create_info: *mut std::ffi::c_void,
) -> Result<Vec<u8>> {
    let info = vk::PipelineCreateInfoKHR {
        next: create_info,
        ..Default::default()
    };
    let mut key = vk::PipelineBinaryKeyKHR::default();
    unsafe { device.get_pipeline_key_khr(Some(&info), &mut key) }
        .map_err(|source| Error::vulkan("vkGetPipelineKeyKHR", source))?;
    let key_size = key.key_size as usize;
    if key_size == 0 || key_size > vk::MAX_PIPELINE_BINARY_KEY_SIZE_KHR {
        return Err(Error::Validation(format!(
            "vkGetPipelineKeyKHR returned invalid key size {key_size}"
        )));
    }
    Ok(key.key.0[..key_size].to_vec())
}

/// Creates a graphics pipeline only after its device-native binaries are usable.
///
/// With no archive, this compiles a capturable pipeline, extracts every binary,
/// destroys that provisional pipeline, and recreates it with
/// the captured binaries. With an archive, compilation is never permitted.
/// The callback must add the phase flags and, for `Ready`, a
/// `VkPipelineBinaryInfoKHR` containing the supplied handles.
///
/// # Safety
///
/// `create` must create exactly one pipeline from the supplied `Device`. It
/// must append [`PipelineBinaryCreation::flags`] and, for `Ready`, a
/// `VkPipelineBinaryInfoKHR` containing exactly
/// [`PipelineBinaryCreation::ready_binaries`]. It must not retain pointers or
/// binary handles after returning.
pub unsafe fn create_graphics_pipeline_machine_code(
    backend: &Backend,
    archive: Option<&PipelineBinaryArchive>,
    create: impl FnMut(&Device, PipelineBinaryCreation<'_>) -> Result<vk::Pipeline>,
) -> Result<MachineCodeGraphicsPipeline> {
    unsafe { create_pipeline_machine_code(backend, archive, create) }
}

/// Creates a retained compute pipeline from materialized device machine code.
///
/// # Safety
///
/// The callback has the same requirements as
/// [`create_graphics_pipeline_machine_code`].
pub unsafe fn create_compute_pipeline_machine_code(
    backend: &Backend,
    archive: Option<&PipelineBinaryArchive>,
    create: impl FnMut(&Device, PipelineBinaryCreation<'_>) -> Result<vk::Pipeline>,
) -> Result<MachineCodeComputePipeline> {
    unsafe { create_pipeline_machine_code(backend, archive, create) }
}

unsafe fn create_pipeline_machine_code(
    backend: &Backend,
    archive: Option<&PipelineBinaryArchive>,
    mut create: impl FnMut(&Device, PipelineBinaryCreation<'_>) -> Result<vk::Pipeline>,
) -> Result<MachineCodePipeline> {
    require_pipeline_binaries(backend)?;
    let device = backend.device();
    let archive_reused = archive.is_some();
    let archive = match archive {
        Some(archive) => {
            validate_archive(archive)?;
            archive.clone()
        }
        None => {
            let provisional = create(device, PipelineBinaryCreation::Capture)?;
            let captured = capture_pipeline_archive(device, provisional);
            let release = vk::ReleaseCapturedPipelineDataInfoKHR::builder()
                .pipeline(provisional)
                .build();
            let released = unsafe { device.release_captured_pipeline_data_khr(&release, None) }
                .map_err(|source| Error::vulkan("vkReleaseCapturedPipelineDataKHR", source));
            unsafe { device.destroy_pipeline(provisional, None) };
            let captured = captured?;
            released?;
            captured
        }
    };
    let handles = create_binary_handles_from_archive(device, &archive)?;
    let pipeline = create(device, PipelineBinaryCreation::Ready(&handles));
    destroy_binary_handles(device, &handles);
    Ok(MachineCodePipeline {
        inner: Arc::new(MachineCodePipelineInner {
            owner: backend.shared_owner(),
            pipeline: pipeline?,
        }),
        archive,
        archive_reused,
        kind: MachineCodePipelineKind::Unspecified,
    })
}

fn require_pipeline_binaries(backend: &Backend) -> Result<()> {
    if !backend.features().contains(Features::PIPELINE_BINARIES) {
        return Err(Error::Validation(
            "pipeline machine code requires enabled Features::PIPELINE_BINARIES".into(),
        ));
    }
    Ok(())
}

fn capture_pipeline_archive(
    device: &Device,
    pipeline: vk::Pipeline,
) -> Result<PipelineBinaryArchive> {
    let create = vk::PipelineBinaryCreateInfoKHR::builder()
        .pipeline(pipeline)
        .build();
    let handles = create_binary_handles(device, &create, None)?;
    let archive = handles
        .iter()
        .copied()
        .map(|handle| pipeline_binary_blob(device, handle))
        .collect::<Result<Vec<_>>>();
    destroy_binary_handles(device, &handles);
    let archive = PipelineBinaryArchive { binaries: archive? };
    validate_archive(&archive)?;
    Ok(archive)
}

fn pipeline_binary_blob(
    device: &Device,
    handle: vk::PipelineBinaryKHR,
) -> Result<PipelineBinaryBlob> {
    let info = vk::PipelineBinaryDataInfoKHR::builder()
        .pipeline_binary(handle)
        .build();
    let mut key = vk::PipelineBinaryKeyKHR::default();
    let mut data_size = 0usize;
    let first = unsafe {
        (device.commands().get_pipeline_binary_data_khr)(
            device.handle(),
            &info,
            &mut key,
            &mut data_size,
            std::ptr::null_mut(),
        )
    };
    require_raw_success("vkGetPipelineBinaryDataKHR(size)", first)?;
    if data_size == 0 {
        return Err(Error::Validation(
            "pipeline binary returned an empty machine-code payload".into(),
        ));
    }
    let mut data = vec![0u8; data_size];
    let second = unsafe {
        (device.commands().get_pipeline_binary_data_khr)(
            device.handle(),
            &info,
            &mut key,
            &mut data_size,
            data.as_mut_ptr().cast(),
        )
    };
    require_raw_success("vkGetPipelineBinaryDataKHR", second)?;
    data.truncate(data_size);
    let key_size = key.key_size as usize;
    if key_size == 0 || key_size > vk::MAX_PIPELINE_BINARY_KEY_SIZE_KHR {
        return Err(Error::Validation(format!(
            "pipeline binary returned invalid key size {key_size}"
        )));
    }
    Ok(PipelineBinaryBlob {
        key: key.key[..key_size].to_vec(),
        data,
    })
}

fn create_binary_handles_from_archive(
    device: &Device,
    archive: &PipelineBinaryArchive,
) -> Result<Vec<vk::PipelineBinaryKHR>> {
    validate_archive(archive)?;
    let keys = archive
        .binaries
        .iter()
        .map(native_key)
        .collect::<Result<Vec<_>>>()?;
    let mut data_storage = archive
        .binaries
        .iter()
        .map(|binary| binary.data.clone())
        .collect::<Vec<_>>();
    let data = data_storage
        .iter_mut()
        .map(|bytes| vk::PipelineBinaryDataKHR::builder().data(bytes).build())
        .collect::<Vec<_>>();
    let keys_and_data = vk::PipelineBinaryKeysAndDataKHR::builder()
        .pipeline_binary_keys(&keys)
        .pipeline_binary_data(&data)
        .build();
    let create = vk::PipelineBinaryCreateInfoKHR::builder()
        .keys_and_data_info(&keys_and_data)
        .build();
    create_binary_handles(device, &create, Some(archive.binaries.len()))
}

fn native_key(binary: &PipelineBinaryBlob) -> Result<vk::PipelineBinaryKeyKHR> {
    if binary.key.is_empty() || binary.key.len() > vk::MAX_PIPELINE_BINARY_KEY_SIZE_KHR {
        return Err(Error::Validation(format!(
            "pipeline binary archive has invalid key size {}",
            binary.key.len()
        )));
    }
    let mut key = vk::PipelineBinaryKeyKHR::default();
    key.key_size = binary.key.len() as u32;
    key.key.0[..binary.key.len()].copy_from_slice(&binary.key);
    Ok(key)
}

fn create_binary_handles(
    device: &Device,
    create: &vk::PipelineBinaryCreateInfoKHR,
    expected_count: Option<usize>,
) -> Result<Vec<vk::PipelineBinaryKHR>> {
    let count = match expected_count {
        Some(count) => count,
        None => {
            let mut query = vk::PipelineBinaryHandlesInfoKHR::default();
            let status = unsafe { device.create_pipeline_binaries_khr(create, None, &mut query) }
                .map_err(|source| {
                Error::vulkan("vkCreatePipelineBinariesKHR(count)", source)
            })?;
            require_success("vkCreatePipelineBinariesKHR(count)", status)?;
            query.pipeline_binary_count as usize
        }
    };
    if count == 0 {
        return Err(Error::Validation(
            "pipeline machine-code preparation returned no binaries".into(),
        ));
    }
    let mut handles = vec![vk::PipelineBinaryKHR::null(); count];
    let mut output = vk::PipelineBinaryHandlesInfoKHR::builder()
        .pipeline_binaries(&mut handles)
        .build();
    let status = unsafe { device.create_pipeline_binaries_khr(create, None, &mut output) }
        .map_err(|source| Error::vulkan("vkCreatePipelineBinariesKHR", source))?;
    if let Err(error) = require_success("vkCreatePipelineBinariesKHR", status) {
        destroy_binary_handles(device, &handles);
        return Err(error);
    }
    if output.pipeline_binary_count as usize != count
        || handles
            .iter()
            .any(|handle| *handle == vk::PipelineBinaryKHR::null())
    {
        destroy_binary_handles(device, &handles);
        return Err(Error::Validation(
            "pipeline binary count or handle set changed during creation".into(),
        ));
    }
    Ok(handles)
}

fn require_success(operation: &'static str, status: vk::SuccessCode) -> Result<()> {
    if status == vk::SuccessCode::SUCCESS {
        Ok(())
    } else {
        Err(Error::Validation(format!(
            "{operation} did not complete: {status:?}"
        )))
    }
}

fn require_raw_success(operation: &'static str, status: vk::Result) -> Result<()> {
    if status == vk::Result::SUCCESS {
        Ok(())
    } else {
        Err(Error::Validation(format!(
            "{operation} did not complete: {status:?}"
        )))
    }
}

pub(super) fn validate_archive(archive: &PipelineBinaryArchive) -> Result<()> {
    if archive.binaries.is_empty() {
        return Err(Error::Validation("pipeline binary archive is empty".into()));
    }
    for binary in &archive.binaries {
        if binary.key.is_empty() || binary.key.len() > vk::MAX_PIPELINE_BINARY_KEY_SIZE_KHR {
            return Err(Error::Validation(format!(
                "pipeline binary archive has invalid key size {}",
                binary.key.len()
            )));
        }
        if binary.data.is_empty() {
            return Err(Error::Validation(
                "pipeline binary archive contains an empty machine-code payload".into(),
            ));
        }
    }
    Ok(())
}

fn destroy_binary_handles(device: &Device, handles: &[vk::PipelineBinaryKHR]) {
    unsafe {
        for handle in handles.iter().copied() {
            if handle != vk::PipelineBinaryKHR::null() {
                device.destroy_pipeline_binary_khr(handle, None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_modes_use_binary_info_as_the_no_compile_contract() {
        assert_eq!(
            PipelineBinaryCreation::Capture.flags(),
            vk::PipelineCreateFlags2::CAPTURE_DATA_KHR
        );
        let handles = [vk::PipelineBinaryKHR::null()];
        let ready = PipelineBinaryCreation::Ready(&handles);
        assert_eq!(ready.flags(), vk::PipelineCreateFlags2::empty());
        assert_eq!(ready.ready_binaries(), Some(handles.as_slice()));
    }

    #[test]
    fn archives_reject_missing_machine_code() {
        let empty = PipelineBinaryArchive::default();
        assert!(validate_archive(&empty).is_err());
        let missing_data = PipelineBinaryArchive {
            binaries: vec![PipelineBinaryBlob {
                key: vec![1],
                data: Vec::new(),
            }],
        };
        assert!(validate_archive(&missing_data).is_err());
    }
}
