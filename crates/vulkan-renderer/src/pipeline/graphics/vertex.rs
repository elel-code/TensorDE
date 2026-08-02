use vulkanalia::vk::{self, HasBuilder};

use super::{VertexBufferLayout, VertexStepMode};

pub(super) fn vertex_input_descriptions(
    buffers: &[VertexBufferLayout<'_>],
) -> (
    Vec<vk::VertexInputBindingDescription>,
    Vec<vk::VertexInputAttributeDescription>,
) {
    let bindings = buffers
        .iter()
        .map(|buffer| {
            vk::VertexInputBindingDescription::builder()
                .binding(buffer.slot)
                .stride(buffer.array_stride as u32)
                .input_rate(match buffer.step_mode {
                    VertexStepMode::Vertex => vk::VertexInputRate::VERTEX,
                    VertexStepMode::Instance => vk::VertexInputRate::INSTANCE,
                })
                .build()
        })
        .collect();
    let attributes = buffers
        .iter()
        .flat_map(|buffer| {
            buffer.attributes.iter().map(move |attribute| {
                vk::VertexInputAttributeDescription::builder()
                    .location(attribute.shader_location)
                    .binding(buffer.slot)
                    .format(attribute.format.to_vk())
                    .offset(attribute.offset as u32)
                    .build()
            })
        })
        .collect();
    (bindings, attributes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VertexAttribute, VertexFormat};

    #[test]
    fn vertex_layout_preserves_an_explicit_nonzero_slot() {
        let attributes = [VertexAttribute {
            format: VertexFormat::Float32x4,
            offset: 16,
            shader_location: 6,
        }];
        let buffers = [VertexBufferLayout {
            slot: 3,
            array_stride: 32,
            step_mode: VertexStepMode::Instance,
            attributes: &attributes,
        }];

        let (bindings, lowered_attributes) = vertex_input_descriptions(&buffers);

        assert_eq!(bindings[0].binding, 3);
        assert_eq!(bindings[0].input_rate, vk::VertexInputRate::INSTANCE);
        assert_eq!(lowered_attributes[0].binding, 3);
        assert_eq!(lowered_attributes[0].location, 6);
    }
}
