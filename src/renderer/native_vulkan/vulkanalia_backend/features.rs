use serde::Serialize;
use vulkanalia::vk::{self, HasBuilder};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaFeatureChainTemplate {
    pub api: &'static str,
    pub chain_root: &'static str,
    pub feature_struct: &'static str,
    pub requested_feature_fields: &'static [&'static str],
}

pub fn native_vulkan_vulkanalia_feature_chain_template()
-> NativeVulkanVulkanaliaFeatureChainTemplate {
    let mut vulkan14_features = vk::PhysicalDeviceVulkan14Features::builder()
        .dynamic_rendering_local_read(true)
        .maintenance5(true)
        .maintenance6(true)
        .push_descriptor(true)
        .build();
    let features2 = vk::PhysicalDeviceFeatures2::builder()
        .push_next(&mut vulkan14_features)
        .build();

    NativeVulkanVulkanaliaFeatureChainTemplate {
        api: "Vulkan 1.4",
        chain_root: std::any::type_name_of_val(&features2),
        feature_struct: std::any::type_name_of_val(&vulkan14_features),
        requested_feature_fields: &[
            "dynamic_rendering_local_read",
            "maintenance5",
            "maintenance6",
            "push_descriptor",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_chain_template_uses_vulkan_1_4_feature_struct() {
        let template = native_vulkan_vulkanalia_feature_chain_template();
        assert_eq!(template.api, "Vulkan 1.4");
        assert!(template.chain_root.ends_with("PhysicalDeviceFeatures2"));
        assert!(
            template
                .feature_struct
                .ends_with("PhysicalDeviceVulkan14Features")
        );
        assert!(
            template
                .requested_feature_fields
                .contains(&"dynamic_rendering_local_read")
        );
        assert!(template.requested_feature_fields.contains(&"maintenance6"));
    }
}
