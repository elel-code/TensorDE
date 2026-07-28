use super::*;

#[test]
fn roadmap_2026_api_floor_is_the_profile_version_not_vulkan_1_4_zero() {
    assert!(!roadmap_2026_api_version_ready(u32::from(Version::V1_4_0)));
    assert!(!roadmap_2026_api_version_ready(u32::from(Version::new(
        1, 4, 327
    ))));
    assert!(roadmap_2026_api_version_ready(u32::from(
        ROADMAP_2026_API_VERSION
    )));
}

#[test]
fn roadmap_2026_required_extensions_include_inherited_profile_capabilities() {
    assert!(ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS.contains(&"VK_KHR_global_priority"));
    assert!(ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS.contains(&"VK_KHR_shader_quad_control"));
    assert!(
        ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS
            .contains(&"VK_KHR_workgroup_memory_explicit_layout")
    );
    assert!(!ROADMAP_2026_REQUIRED_DEVICE_EXTENSIONS.contains(&"VK_KHR_maintenance10"));
}
