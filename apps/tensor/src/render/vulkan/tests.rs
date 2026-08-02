use super::*;

#[test]
fn typed_client_pipeline_formats_match_native_output_images() {
    for (fourcc, typed) in [
        (Fourcc::XRGB8888, TextureFormat::Bgra8Srgb),
        (Fourcc::XBGR8888, TextureFormat::Rgba8Srgb),
        (Fourcc::XRGB2101010, TextureFormat::A2R10G10B10UnormPack32),
        (Fourcc::XBGR2101010, TextureFormat::A2B10G10R10UnormPack32),
    ] {
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
