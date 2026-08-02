use super::*;

#[test]
fn typed_client_pipeline_formats_match_native_output_images() {
    for (fourcc, raw, typed) in [
        (
            Fourcc::XRGB8888,
            vk::Format::B8G8R8A8_SRGB,
            TextureFormat::Bgra8Srgb,
        ),
        (
            Fourcc::XBGR8888,
            vk::Format::R8G8B8A8_SRGB,
            TextureFormat::Rgba8Srgb,
        ),
        (
            Fourcc::XRGB2101010,
            vk::Format::A2R10G10B10_UNORM_PACK32,
            TextureFormat::A2R10G10B10UnormPack32,
        ),
        (
            Fourcc::XBGR2101010,
            vk::Format::A2B10G10R10_UNORM_PACK32,
            TextureFormat::A2B10G10R10UnormPack32,
        ),
    ] {
        assert_eq!(vulkan_format_for_fourcc(fourcc), Some(raw));
        assert_eq!(texture_format_for_fourcc(fourcc), Some(typed));
    }
}

#[test]
fn client_image_allocator_cannot_retain_a_pool_larger_than_two_mib() {
    const MIB: u64 = 1024 * 1024;
    let config = client_image_allocator_config();

    assert_eq!(config.device_block_size, 2 * MIB);
    assert_eq!(config.image_block_size, 2 * MIB);
    assert_eq!(config.upload_block_size, 2 * MIB);
    assert_eq!(config.readback_block_size, 2 * MIB);
    assert_eq!(config.dedicated_threshold, 2 * MIB);
}
