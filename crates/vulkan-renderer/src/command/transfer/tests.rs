use super::*;

#[test]
fn transfer_descriptors_preserve_explicit_offsets_and_packing() {
    let copy = BufferImageCopy {
        buffer_offset: 256,
        buffer_row_length: 64,
        buffer_image_height: 32,
        image_subresource: vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        },
        image_offset: vk::Offset3D { x: 4, y: 8, z: 0 },
        image_extent: vk::Extent3D {
            width: 32,
            height: 16,
            depth: 1,
        },
    };
    assert_eq!(copy.buffer_offset, 256);
    assert_eq!(copy.image_extent.width, 32);
}

#[test]
fn image_copy_preserves_bounded_effect_region() {
    let copy = ImageCopy {
        source_subresource: color_layers(),
        source_offset: vk::Offset3D {
            x: 120,
            y: 40,
            z: 0,
        },
        destination_subresource: color_layers(),
        destination_offset: vk::Offset3D {
            x: 120,
            y: 40,
            z: 0,
        },
        extent: vk::Extent3D {
            width: 300,
            height: 500,
            depth: 1,
        },
    };
    assert_eq!(copy.source_offset.x, 120);
    assert_eq!(copy.extent.height, 500);
}

#[test]
fn image_blit_keeps_independent_source_and_destination_boxes() {
    let blit = ImageBlit {
        source_subresource: color_layers(),
        source_offsets: [vk::Offset3D::default(), vk::Offset3D { x: 64, y: 32, z: 1 }],
        destination_subresource: color_layers(),
        destination_offsets: [
            vk::Offset3D { x: 8, y: 16, z: 0 },
            vk::Offset3D {
                x: 136,
                y: 80,
                z: 1,
            },
        ],
    };
    assert_eq!(blit.source_offsets[1].x, 64);
    assert_eq!(blit.destination_offsets[0].y, 16);
    assert_eq!(ImageBlitFilter::Linear.to_vk(), vk::Filter::LINEAR);
}

fn color_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers {
        aspect_mask: vk::ImageAspectFlags::COLOR,
        mip_level: 0,
        base_array_layer: 0,
        layer_count: 1,
    }
}
