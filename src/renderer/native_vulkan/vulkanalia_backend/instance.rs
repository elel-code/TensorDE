#![allow(dead_code)]

use vulkanalia::Version;
use vulkanalia::loader::LibloadingLoader;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

const LOADER_CANDIDATES: &[&str] = &["libvulkan.so.1", "libvulkan.so"];

pub(super) struct NativeVulkanVulkanaliaInstance {
    pub(super) instance: Instance,
    pub(super) loader_name: &'static str,
    _entry: Entry,
}

pub(super) fn native_vulkan_vulkanalia_create_instance()
-> Result<NativeVulkanVulkanaliaInstance, String> {
    let (loader, loader_name) = native_vulkan_vulkanalia_load_loader()?;
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
        .map_err(|err| format!("vkCreateInstance(vulkanalia): {err:?}"))?;

    Ok(NativeVulkanVulkanaliaInstance {
        instance,
        loader_name,
        _entry: entry,
    })
}

pub(super) fn native_vulkan_vulkanalia_destroy_instance(vulkan: NativeVulkanVulkanaliaInstance) {
    unsafe {
        vulkan.instance.destroy_instance(None);
    }
}

fn native_vulkan_vulkanalia_load_loader() -> Result<(LibloadingLoader, &'static str), String> {
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
