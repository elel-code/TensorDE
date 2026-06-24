const COMMON_VIDEO_DECODE_EXTENSIONS: &[&str] =
    &["VK_KHR_video_queue", "VK_KHR_video_decode_queue"];

pub(super) fn query_disabled_reason(
    device_extensions: &[String],
    has_video_decode_queue_family: bool,
    codec_extension: &'static str,
) -> Option<String> {
    let mut missing = COMMON_VIDEO_DECODE_EXTENSIONS
        .iter()
        .copied()
        .filter(|extension| !has_extension(device_extensions, extension))
        .collect::<Vec<_>>();
    if !has_extension(device_extensions, codec_extension) {
        missing.push(codec_extension);
    }
    if !missing.is_empty() {
        return Some(format!(
            "missing required Vulkan Video decode extensions: {}",
            missing.join(", ")
        ));
    }
    (!has_video_decode_queue_family).then(|| "missing Vulkan video decode queue family".to_owned())
}

fn has_extension(available: &[String], required: &str) -> bool {
    available.iter().any(|extension| extension == required)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_query_gate_requires_extensions_and_decode_queue() {
        let missing_extensions = query_disabled_reason(&[], false, "VK_KHR_video_decode_h264")
            .expect("missing extensions should disable profile queries");
        assert!(missing_extensions.contains("VK_KHR_video_queue"));
        assert!(missing_extensions.contains("VK_KHR_video_decode_h264"));

        let extensions = vec![
            "VK_KHR_video_queue".to_owned(),
            "VK_KHR_video_decode_queue".to_owned(),
            "VK_KHR_video_decode_h264".to_owned(),
        ];
        assert_eq!(
            query_disabled_reason(&extensions, false, "VK_KHR_video_decode_h264"),
            Some("missing Vulkan video decode queue family".to_owned())
        );
        assert_eq!(
            query_disabled_reason(&extensions, true, "VK_KHR_video_decode_h264"),
            None
        );
    }
}
