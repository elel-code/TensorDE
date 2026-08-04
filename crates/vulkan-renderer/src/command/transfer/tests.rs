use super::*;

#[test]
fn typed_color_copy_lowers_exact_mip_layer_origin_and_extent() {
    let copy = lower_color_image_copy(ColorImageCopy {
        source_mip_level: 2,
        source_base_array_layer: 3,
        source_origin: Origin2D::new(4, 5),
        destination_mip_level: 1,
        destination_base_array_layer: 7,
        destination_origin: Origin2D::new(8, 9),
        extent: Extent2D::new(64, 32),
        layer_count: 2,
    })
    .unwrap();
    assert_eq!(copy.source_subresource.mip_level, 2);
    assert_eq!(copy.source_subresource.base_array_layer, 3);
    assert_eq!(copy.source_subresource.layer_count, 2);
    assert_eq!((copy.source_offset.x, copy.source_offset.y), (4, 5));
    assert_eq!(copy.destination_subresource.mip_level, 1);
    assert_eq!(copy.destination_subresource.base_array_layer, 7);
    assert_eq!(
        (copy.destination_offset.x, copy.destination_offset.y),
        (8, 9)
    );
    assert_eq!(
        (
            copy.extent.width,
            copy.extent.height,
            copy.extent.depth_or_layers
        ),
        (64, 32, 1)
    );
}

#[test]
fn typed_color_copy_rejects_empty_ranges() {
    let copy = ColorImageCopy {
        source_mip_level: 0,
        source_base_array_layer: 0,
        source_origin: Origin2D::new(0, 0),
        destination_mip_level: 0,
        destination_base_array_layer: 0,
        destination_origin: Origin2D::new(0, 0),
        extent: Extent2D::new(0, 1),
        layer_count: 1,
    };
    assert!(lower_color_image_copy(copy).is_err());
}

#[test]
fn transfer_descriptors_preserve_explicit_offsets_and_packing() {
    let copy = BufferImageCopy {
        buffer_offset: 256,
        buffer_row_length: 64,
        buffer_image_height: 32,
        image_subresource: TextureSubresourceLayers::color(0, 0, 1),
        image_offset: Origin3D::new(4, 8, 0),
        image_extent: Extent3D::new(32, 16, 1),
    };
    assert_eq!(copy.buffer_offset, 256);
    assert_eq!(copy.image_extent.width, 32);
}

#[test]
fn typed_color_buffer_upload_lowers_without_vulkan_copy_types() {
    let copy = lower_color_buffer_image_copy(ColorBufferImageCopy {
        buffer_offset: 128,
        buffer_row_length: 80,
        buffer_image_height: 40,
        destination_mip_level: 2,
        destination_base_array_layer: 3,
        destination_origin: Origin2D::new(7, 11),
        extent: Extent2D::new(64, 32),
        layer_count: 1,
    })
    .unwrap();
    assert_eq!(copy.buffer_offset, 128);
    assert_eq!(copy.image_subresource.mip_level, 2);
    assert_eq!(copy.image_subresource.base_array_layer, 3);
    assert_eq!((copy.image_offset.x, copy.image_offset.y), (7, 11));
    assert_eq!(
        (
            copy.image_extent.width,
            copy.image_extent.height,
            copy.image_extent.depth_or_layers
        ),
        (64, 32, 1)
    );
}

#[test]
fn typed_color_buffer_upload_rejects_empty_or_negative_regions() {
    let empty = ColorBufferImageCopy {
        buffer_offset: 0,
        buffer_row_length: 0,
        buffer_image_height: 0,
        destination_mip_level: 0,
        destination_base_array_layer: 0,
        destination_origin: Origin2D::new(0, 0),
        extent: Extent2D::new(0, 1),
        layer_count: 1,
    };
    assert!(lower_color_buffer_image_copy(empty).is_err());
    let negative = ColorBufferImageCopy {
        destination_origin: Origin2D::new(-1, 0),
        extent: Extent2D::new(1, 1),
        ..empty
    };
    assert!(lower_color_buffer_image_copy(negative).is_err());
}

#[test]
fn typed_color_readback_lowers_without_vulkan_copy_types() {
    let copy = lower_color_image_buffer_copy(ColorImageBufferCopy {
        buffer_offset: 512,
        buffer_row_length: 1920,
        buffer_image_height: 1080,
        source_mip_level: 1,
        source_base_array_layer: 2,
        source_origin: Origin2D::new(32, 24),
        extent: Extent2D::new(640, 480),
        layer_count: 1,
    })
    .unwrap();
    assert_eq!(copy.buffer_offset, 512);
    assert_eq!(copy.buffer_row_length, 1920);
    assert_eq!(copy.image_subresource.mip_level, 1);
    assert_eq!(copy.image_subresource.base_array_layer, 2);
    assert_eq!((copy.image_offset.x, copy.image_offset.y), (32, 24));
    assert_eq!(
        (
            copy.image_extent.width,
            copy.image_extent.height,
            copy.image_extent.depth_or_layers
        ),
        (640, 480, 1)
    );
}

#[test]
fn typed_color_readback_rejects_empty_or_negative_regions() {
    let empty = ColorImageBufferCopy {
        buffer_offset: 0,
        buffer_row_length: 0,
        buffer_image_height: 0,
        source_mip_level: 0,
        source_base_array_layer: 0,
        source_origin: Origin2D::new(0, 0),
        extent: Extent2D::new(0, 1),
        layer_count: 1,
    };
    assert!(lower_color_image_buffer_copy(empty).is_err());
    assert!(
        lower_color_image_buffer_copy(ColorImageBufferCopy {
            source_origin: Origin2D::new(-1, 0),
            extent: Extent2D::new(1, 1),
            ..empty
        })
        .is_err()
    );
}

#[test]
fn image_copy_preserves_bounded_effect_region() {
    let copy = ImageCopy {
        source_subresource: color_layers(),
        source_offset: Origin3D::new(120, 40, 0),
        destination_subresource: color_layers(),
        destination_offset: Origin3D::new(120, 40, 0),
        extent: Extent3D::new(300, 500, 1),
    };
    assert_eq!(copy.source_offset.x, 120);
    assert_eq!(copy.extent.height, 500);
}

#[test]
fn image_blit_keeps_independent_source_and_destination_boxes() {
    let blit = ImageBlit {
        source_subresource: color_layers(),
        source_offsets: [Origin3D::default(), Origin3D::new(64, 32, 1)],
        destination_subresource: color_layers(),
        destination_offsets: [Origin3D::new(8, 16, 0), Origin3D::new(136, 80, 1)],
    };
    assert_eq!(blit.source_offsets[1].x, 64);
    assert_eq!(blit.destination_offsets[0].y, 16);
    assert_ne!(ImageBlitFilter::Nearest, ImageBlitFilter::Linear);
}

fn color_layers() -> TextureSubresourceLayers {
    TextureSubresourceLayers::color(0, 0, 1)
}
