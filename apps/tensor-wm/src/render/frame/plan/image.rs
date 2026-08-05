use std::collections::HashMap;

use crate::ecs::SurfaceBufferId;

use super::super::FrameError;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ClientImageDescriptor {
    pub(crate) buffer_id: SurfaceBufferId,
    pub(crate) view_encoding: vulkan_renderer::SourceImageViewEncoding,
}

impl ClientImageDescriptor {
    pub(crate) const fn srgb(buffer_id: SurfaceBufferId) -> Self {
        Self {
            buffer_id,
            view_encoding: vulkan_renderer::SourceImageViewEncoding::SrgbDecoded,
        }
    }
}

pub(super) fn image_descriptor_for(
    image: ClientImageDescriptor,
    images: &mut Vec<ClientImageDescriptor>,
    descriptors: &mut HashMap<ClientImageDescriptor, u32>,
) -> Result<u32, FrameError> {
    if let Some(index) = descriptors.get(&image) {
        return Ok(*index);
    }
    let index = u32::try_from(images.len())
        .ok()
        .and_then(|index| index.checked_add(1))
        .ok_or(FrameError::DescriptorSizeOverflow)?;
    images.push(image);
    descriptors.insert(image, index);
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_buffer_uses_distinct_descriptors_for_encoded_and_srgb_views() {
        let buffer_id = SurfaceBufferId::new(9);
        let srgb = ClientImageDescriptor::srgb(buffer_id);
        let encoded = ClientImageDescriptor {
            buffer_id,
            view_encoding: vulkan_renderer::SourceImageViewEncoding::Encoded,
        };
        let mut images = Vec::new();
        let mut descriptors = HashMap::new();

        assert_eq!(
            image_descriptor_for(srgb, &mut images, &mut descriptors).unwrap(),
            1
        );
        assert_eq!(
            image_descriptor_for(encoded, &mut images, &mut descriptors).unwrap(),
            2
        );
        assert_eq!(
            image_descriptor_for(srgb, &mut images, &mut descriptors).unwrap(),
            1
        );
        assert_eq!(images, [srgb, encoded]);
    }
}
