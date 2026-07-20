#![allow(dead_code)]

use vulkanalia::Version;
use vulkanalia::loader::LibloadingLoader;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::roadmap_2026::{
    GILDER_ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS, ROADMAP_2026_API_VERSION,
    ROADMAP_2026_PROFILE_NAME, ROADMAP_2026_PROFILE_REVISION,
};

pub(in crate::renderer::native_vulkan::vulkan) const NATIVE_VULKAN_VULKANALIA_REQUIRED_LOADER:
    &str = "libvulkan.so.1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaInstanceExtensionSelection
{
    pub(in crate::renderer::native_vulkan::vulkan) available_instance_extensions: Vec<String>,
    pub(in crate::renderer::native_vulkan::vulkan) enabled_instance_extensions: Vec<&'static str>,
    pub(in crate::renderer::native_vulkan::vulkan) missing_instance_extensions: Vec<&'static str>,
}

pub(in crate::renderer::native_vulkan::vulkan) struct NativeVulkanVulkanaliaInstance {
    pub(in crate::renderer::native_vulkan::vulkan) instance: Instance,
    pub(in crate::renderer::native_vulkan::vulkan) loader_name: &'static str,
    pub(in crate::renderer::native_vulkan::vulkan) entry_version: Version,
    pub(in crate::renderer::native_vulkan::vulkan) extension_selection:
        NativeVulkanVulkanaliaInstanceExtensionSelection,
    _entry: Entry,
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_instance()
-> Result<NativeVulkanVulkanaliaInstance, String> {
    native_vulkan_vulkanalia_create_instance_with_required_extensions(&[])
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_instance_with_required_extensions(
    required_instance_extensions: &[&'static str],
) -> Result<NativeVulkanVulkanaliaInstance, String> {
    let (loader, loader_name) = native_vulkan_vulkanalia_load_loader()?;
    let entry = unsafe { Entry::new(loader) }
        .map_err(|err| format!("vulkanalia Entry::new({loader_name}): {err}"))?;
    let entry_version = entry
        .version()
        .map_err(|err| format!("vkEnumerateInstanceVersion: {err:?}"))?;
    if u32::from(entry_version) < u32::from(ROADMAP_2026_API_VERSION) {
        return Err(format!(
            "Vulkan loader {loader_name} exposes {entry_version}, below mandatory {ROADMAP_2026_PROFILE_NAME} revision {ROADMAP_2026_PROFILE_REVISION} API floor {ROADMAP_2026_API_VERSION}"
        ));
    }
    let available_instance_extensions =
        unsafe { entry.enumerate_instance_extension_properties(None) }
            .map_err(|err| format!("vkEnumerateInstanceExtensionProperties: {err:?}"))?
            .into_iter()
            .map(|property| property.extension_name.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
    let required_instance_extensions =
        merged_required_instance_extensions(required_instance_extensions);
    let extension_selection = native_vulkan_vulkanalia_select_instance_extensions(
        available_instance_extensions,
        &required_instance_extensions,
    );
    if !extension_selection.missing_instance_extensions.is_empty() {
        return Err(format!(
            "mandatory {ROADMAP_2026_PROFILE_NAME} Wayland instance contract missing extensions: {}",
            extension_selection.missing_instance_extensions.join(", ")
        ));
    }
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
        .api_version(u32::from(ROADMAP_2026_API_VERSION));
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

fn merged_required_instance_extensions(
    additional_required: &[&'static str],
) -> Vec<&'static str> {
    let mut required = GILDER_ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS.to_vec();
    for extension in additional_required {
        if !required.contains(extension) {
            required.push(extension);
        }
    }
    required
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_instance(
    vulkan: NativeVulkanVulkanaliaInstance,
) {
    unsafe {
        vulkan.instance.destroy_instance(None);
    }
}

fn native_vulkan_vulkanalia_load_loader() -> Result<(LibloadingLoader, &'static str), String> {
    unsafe { LibloadingLoader::new(NATIVE_VULKAN_VULKANALIA_REQUIRED_LOADER) }
        .map(|loader| (loader, NATIVE_VULKAN_VULKANALIA_REQUIRED_LOADER))
        .map_err(|err| {
            format!(
                "failed to load required Vulkan loader {} via vulkanalia: {err}",
                NATIVE_VULKAN_VULKANALIA_REQUIRED_LOADER
            )
        })
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
    use super::{
        merged_required_instance_extensions,
        native_vulkan_vulkanalia_select_instance_extensions,
    };
    use crate::renderer::native_vulkan::vulkan::core::roadmap_2026::GILDER_ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS;

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

    #[test]
    fn every_instance_path_inherits_the_exact_roadmap_2026_wayland_contract() {
        let required = merged_required_instance_extensions(&["VK_TEST_required"]);
        assert_eq!(
            &required[..GILDER_ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS.len()],
            GILDER_ROADMAP_2026_REQUIRED_INSTANCE_EXTENSIONS
        );
        assert_eq!(required.last(), Some(&"VK_TEST_required"));
    }
}
