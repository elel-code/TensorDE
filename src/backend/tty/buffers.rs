use crate::{
    backend::BackendOutputId,
    render::{DrmNodeId, ExportedDmabuf, NativeOutputBuffer, OutputFormat},
};

use super::{BackendError, TtyBackend, kms::KmsOutput};

impl TtyBackend {
    pub(crate) fn install_output_buffers(
        &mut self,
        output_id: BackendOutputId,
        buffers: Vec<NativeOutputBuffer>,
    ) -> Result<(), BackendError> {
        let descriptor = self
            .outputs
            .get(&output_id)
            .ok_or(BackendError::UnknownOutput(output_id))?
            .clone();
        let output_name = descriptor.name.clone();
        let expected_size = (descriptor.mode.width, descriptor.mode.height);
        let expected_format = descriptor.native_format;
        let expected_render_node =
            DrmNodeId::new(self.render_node.major(), self.render_node.minor());
        let device_id = output_id.device_id;
        let device = self
            .devices
            .get_mut(&device_id)
            .ok_or(BackendError::UnknownDevice { device_id })?;
        if device.native_targets.contains_key(&output_id) {
            return Err(BackendError::OutputBuffers {
                output: output_name,
                message: "live KMS target replacement requires a completed modeset generation"
                    .to_owned(),
            });
        }
        let drm_mode = {
            device
                .scanner
                .connectors()
                .values()
                .find(|connector| u32::from(connector.handle()) == output_id.connector_id)
                .and_then(|connector| {
                    connector.modes().iter().copied().find(|mode| {
                        let physical = crate::backend::physical_mode_from_smithay(
                            smithay::output::Mode::from(*mode),
                        );
                        physical == descriptor.mode
                    })
                })
                .ok_or_else(|| BackendError::OutputBuffers {
                    output: descriptor.name.clone(),
                    message: "selected DRM mode disappeared before KMS surface creation".to_owned(),
                })?
        };
        if buffers.len() != NativeOutputBuffer::COUNT {
            return Err(BackendError::OutputBuffers {
                output: output_name,
                message: format!(
                    "renderer returned {} buffers, expected {}",
                    buffers.len(),
                    NativeOutputBuffer::COUNT
                ),
            });
        }
        let mut buffers = buffers;
        buffers.sort_unstable_by_key(|buffer| buffer.slot);
        let mut imported = Vec::with_capacity(buffers.len());
        for (expected_slot, buffer) in buffers.into_iter().enumerate() {
            if usize::from(buffer.slot) != expected_slot {
                return Err(BackendError::OutputBuffers {
                    output: output_name.clone(),
                    message: format!(
                        "renderer returned non-contiguous or duplicate slot {}; expected {expected_slot}",
                        buffer.slot
                    ),
                });
            }
            validate_output_dmabuf(
                &buffer.dmabuf,
                expected_size,
                expected_format,
                expected_render_node,
            )
            .map_err(|message| BackendError::OutputBuffers {
                output: output_name.clone(),
                message,
            })?;
            imported.push((buffer.slot, buffer.dmabuf));
        }
        let target = KmsOutput::new(
            &mut device.drm,
            &device.gbm,
            &descriptor,
            drm_mode,
            imported,
        )
        .map_err(|error| BackendError::OutputBuffers {
            output: descriptor.name.clone(),
            message: error.to_string(),
        })?;
        device.native_targets.insert(output_id, target);
        Ok(())
    }

    pub(crate) fn remove_output_buffers(&mut self, output_id: BackendOutputId) {
        if let Some(device) = self.devices.get_mut(&output_id.device_id) {
            device.native_targets.remove(&output_id);
        }
    }
}

fn validate_output_dmabuf(
    dmabuf: &ExportedDmabuf,
    expected_size: (i32, i32),
    expected_format: OutputFormat,
    expected_render_node: DrmNodeId,
) -> Result<(), String> {
    let expected_width = u32::try_from(expected_size.0).unwrap_or(u32::MAX);
    let expected_height = u32::try_from(expected_size.1).unwrap_or(u32::MAX);
    if (dmabuf.size.width, dmabuf.size.height) != (expected_width, expected_height) {
        return Err(format!(
            "dma-buf size {}x{} does not match output {}x{}",
            dmabuf.size.width, dmabuf.size.height, expected_size.0, expected_size.1
        ));
    }
    if dmabuf.format != expected_format.format {
        return Err(format!(
            "dma-buf format {:?} does not match negotiated {:?}",
            dmabuf.format, expected_format.format
        ));
    }
    if dmabuf.planes.len() != usize::try_from(expected_format.plane_count).unwrap_or(usize::MAX) {
        return Err(format!(
            "dma-buf has {} planes, expected {}",
            dmabuf.planes.len(),
            expected_format.plane_count
        ));
    }
    match dmabuf.node {
        Some(node) if node == expected_render_node => {}
        Some(node) => {
            return Err(format!(
                "dma-buf render node {node} does not match the Vulkan-selected node {expected_render_node}"
            ));
        }
        None => return Err("dma-buf has no Vulkan render-node identity".to_owned()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs::File, os::fd::OwnedFd, sync::Arc};

    use tensor_host::{DrmFormat, Fourcc, Modifier};
    use tensor_util::Size;

    use super::*;
    use crate::render::DmabufPlane;

    fn format() -> OutputFormat {
        OutputFormat {
            format: DrmFormat {
                code: Fourcc::XRGB8888,
                modifier: Modifier::from_raw(9),
            },
            plane_count: 1,
        }
    }

    fn dmabuf(size: Size, output_format: OutputFormat) -> ExportedDmabuf {
        let fd: OwnedFd = File::open("/dev/null").unwrap().into();
        ExportedDmabuf {
            size,
            format: output_format.format,
            node: Some(DrmNodeId::new(226, 128)),
            planes: vec![DmabufPlane {
                fd: Arc::new(fd),
                offset: 0,
                stride: 256,
            }],
        }
    }

    #[test]
    fn output_dmabuf_validation_accepts_the_negotiated_contract() {
        let output_format = format();
        let render_node = DrmNodeId::new(226, 128);
        assert!(
            validate_output_dmabuf(
                &dmabuf(Size::new(1920, 1080), output_format),
                (1920, 1080),
                output_format,
                render_node,
            )
            .is_ok()
        );
    }

    #[test]
    fn output_dmabuf_validation_rejects_size_format_and_plane_mismatches() {
        let output_format = format();
        let render_node = DrmNodeId::new(226, 128);
        let buffer = dmabuf(Size::new(1280, 720), output_format);
        assert!(validate_output_dmabuf(&buffer, (1920, 1080), output_format, render_node).is_err());
        let mut different = output_format;
        different.format.code = Fourcc::ARGB8888;
        assert!(validate_output_dmabuf(&buffer, (1280, 720), different, render_node).is_err());
        let mut different_planes = output_format;
        different_planes.plane_count = 2;
        assert!(
            validate_output_dmabuf(&buffer, (1280, 720), different_planes, render_node).is_err()
        );
        assert!(
            validate_output_dmabuf(
                &buffer,
                (1280, 720),
                output_format,
                DrmNodeId::new(226, 129),
            )
            .is_err()
        );
    }
}
