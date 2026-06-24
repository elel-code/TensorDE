#![allow(dead_code)]

use vulkanalia::Version;
use vulkanalia::loader::LibloadingLoader;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

pub(super) const NATIVE_VULKAN_VULKANALIA_LOADER_CANDIDATES: &[&str] =
    &["libvulkan.so.1", "libvulkan.so"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanVulkanaliaInstanceExtensionSelection {
    pub(super) available_instance_extensions: Vec<String>,
    pub(super) enabled_instance_extensions: Vec<&'static str>,
    pub(super) missing_instance_extensions: Vec<&'static str>,
}

pub(super) struct NativeVulkanVulkanaliaInstance {
    pub(super) instance: Instance,
    pub(super) loader_name: &'static str,
    pub(super) entry_version: Version,
    pub(super) extension_selection: NativeVulkanVulkanaliaInstanceExtensionSelection,
    _entry: Entry,
}

pub(super) fn native_vulkan_vulkanalia_create_instance()
-> Result<NativeVulkanVulkanaliaInstance, String> {
    native_vulkan_vulkanalia_create_instance_with_required_extensions(&[])
}

pub(super) fn native_vulkan_vulkanalia_create_instance_with_required_extensions(
    required_instance_extensions: &[&'static str],
) -> Result<NativeVulkanVulkanaliaInstance, String> {
    let (loader, loader_name) = native_vulkan_vulkanalia_load_loader()?;
    let entry = unsafe { Entry::new(loader) }
        .map_err(|err| format!("vulkanalia Entry::new({loader_name}): {err}"))?;
    let entry_version = entry
        .version()
        .map_err(|err| format!("vkEnumerateInstanceVersion: {err:?}"))?;
    let available_instance_extensions =
        unsafe { entry.enumerate_instance_extension_properties(None) }
            .map_err(|err| format!("vkEnumerateInstanceExtensionProperties: {err:?}"))?
            .into_iter()
            .map(|property| property.extension_name.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
    let extension_selection = native_vulkan_vulkanalia_select_instance_extensions(
        available_instance_extensions,
        required_instance_extensions,
    );
    let extension_names = extension_selection
        .enabled_instance_extensions
        .iter()
        .map(|extension| std::ffi::CString::new(*extension).expect("static extension has no nul"))
        .collect::<Vec<_>>();
    let extension_name_ptrs = extension_names
        .iter()
        .map(|extension| extension.as_ptr())
        .collect::<Vec<_>>();

    let app_info = vk::ApplicationInfo::builder()
        .application_name(b"gilder-native-vulkan\0")
        .application_version(1)
        .engine_name(b"gilder\0")
        .engine_version(1)
        .api_version(u32::from(Version::V1_4_0));
    let create_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_extension_names(&extension_name_ptrs);
    let instance = unsafe { entry.create_instance(&create_info, None) }
        .map_err(|err| format!("vkCreateInstance(vulkanalia): {err:?}"))?;

    Ok(NativeVulkanVulkanaliaInstance {
        instance,
        loader_name,
        entry_version,
        extension_selection,
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
    for candidate in NATIVE_VULKAN_VULKANALIA_LOADER_CANDIDATES {
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

fn native_vulkan_vulkanalia_select_instance_extensions(
    available_instance_extensions: Vec<String>,
    required_instance_extensions: &[&'static str],
) -> NativeVulkanVulkanaliaInstanceExtensionSelection {
    let enabled_instance_extensions = required_instance_extensions
        .iter()
        .copied()
        .filter(|extension| {
            available_instance_extensions
                .iter()
                .any(|name| name == extension)
        })
        .collect::<Vec<_>>();
    let missing_instance_extensions = required_instance_extensions
        .iter()
        .copied()
        .filter(|extension| {
            !available_instance_extensions
                .iter()
                .any(|name| name == extension)
        })
        .collect::<Vec<_>>();

    NativeVulkanVulkanaliaInstanceExtensionSelection {
        available_instance_extensions,
        enabled_instance_extensions,
        missing_instance_extensions,
    }
}

#[cfg(test)]
mod tests {
    use super::native_vulkan_vulkanalia_select_instance_extensions;

    #[test]
    fn extension_selection_enables_only_available_required_extensions() {
        let available = vec!["VK_A".to_owned(), "VK_B".to_owned()];
        let required = ["VK_A", "VK_C"];
        let selection =
            native_vulkan_vulkanalia_select_instance_extensions(available.clone(), &required);

        assert_eq!(selection.available_instance_extensions, available);
        assert_eq!(selection.enabled_instance_extensions, vec!["VK_A"]);
        assert_eq!(selection.missing_instance_extensions, vec!["VK_C"]);
    }
}
