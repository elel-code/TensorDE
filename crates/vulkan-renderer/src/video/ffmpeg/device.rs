use std::ffi::CString;
use std::os::raw::{c_char, c_int};
use std::ptr;

use vulkanalia::{prelude::v1_4::*, vk::Handle};

use super::ffi::{self, AVBufferRef};
use crate::video::VideoDecodeDevice;
use crate::{Error, Result};

pub(super) struct FfmpegVulkanDevice {
    raw: *mut AVBufferRef,
    decode_device: VideoDecodeDevice,
}

impl FfmpegVulkanDevice {
    pub(super) fn create(decode_device: &VideoDecodeDevice) -> Result<Self> {
        let owner = &decode_device.owner;
        if decode_device.queue_family == owner.graphics_queue_family() {
            return Err(Error::VideoDecode(
                "FFmpeg Vulkan decode requires a video queue family distinct from graphics so VkQueue host access remains externally synchronized"
                    .into(),
            ));
        }
        let instance = owner.instance_owner();
        let instance_extensions = c_extensions(&instance.enabled_extensions)?;
        let device_extensions = c_extensions(&owner.enabled_device_extensions)?;
        let instance_pointers = c_extension_pointers(&instance_extensions);
        let device_pointers = c_extension_pointers(&device_extensions);
        let mut raw = ptr::null_mut();
        let result = unsafe {
            ffi::vr_ffmpeg_create_vulkan_device(
                &mut raw,
                instance.instance.handle().as_raw(),
                owner.physical_device().as_raw(),
                owner.device.handle().as_raw(),
                instance_pointers.as_ptr(),
                count_c_int(instance_pointers.len(), "instance extensions")?,
                device_pointers.as_ptr(),
                count_c_int(device_pointers.len(), "device extensions")?,
                c_int::try_from(decode_device.queue_family).map_err(|_| {
                    Error::VideoDecode("video queue-family index exceeds FFmpeg ABI".into())
                })?,
                1,
                decode_device.queue_flags.bits(),
                decode_device.operations.to_vk().bits(),
            )
        };
        super::resources::ffmpeg_ok(result, "create FFmpeg Vulkan hwdevice")?;
        if raw.is_null() {
            return Err(Error::VideoDecode(
                "FFmpeg returned a null Vulkan hwdevice".into(),
            ));
        }
        Ok(Self {
            raw,
            decode_device: decode_device.clone(),
        })
    }

    pub(super) const fn raw(&self) -> *mut AVBufferRef {
        self.raw
    }

    pub(super) const fn decode_device(&self) -> &VideoDecodeDevice {
        &self.decode_device
    }
}

impl Drop for FfmpegVulkanDevice {
    fn drop(&mut self) {
        unsafe { ffi::vr_ffmpeg_buffer_unref(&mut self.raw) };
    }
}

// The owning decoder moves as one unit to a decode worker and is never shared.
unsafe impl Send for FfmpegVulkanDevice {}

fn c_extensions(extensions: &[String]) -> Result<Vec<CString>> {
    extensions
        .iter()
        .map(|extension| {
            CString::new(extension.as_str()).map_err(|_| {
                Error::VideoDecode(format!(
                    "Vulkan extension name contains an interior NUL: {extension:?}"
                ))
            })
        })
        .collect()
}

fn c_extension_pointers(extensions: &[CString]) -> Vec<*const c_char> {
    extensions
        .iter()
        .map(|extension| extension.as_ptr())
        .collect()
}

fn count_c_int(count: usize, label: &'static str) -> Result<c_int> {
    c_int::try_from(count)
        .map_err(|_| Error::VideoDecode(format!("{label} count exceeds FFmpeg ABI")))
}
