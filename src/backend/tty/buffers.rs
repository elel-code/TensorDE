use std::collections::BTreeMap;

use smithay::backend::allocator::{Buffer, dmabuf::Dmabuf};

use crate::{
    backend::BackendOutputId,
    render::{NativeOutputBuffer, OutputFormat},
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
        let expected_size = (descriptor.mode.size.w, descriptor.mode.size.h);
        let expected_format = descriptor.native_format;
        let device_id = output_id.device_id as libc::dev_t;
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
                    connector
                        .modes()
                        .iter()
                        .copied()
                        .find(|mode| smithay::output::Mode::from(*mode) == descriptor.mode)
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
        let mut imported = BTreeMap::new();
        for buffer in buffers {
            if usize::from(buffer.slot) >= NativeOutputBuffer::COUNT {
                return Err(BackendError::OutputBuffers {
                    output: output_name.clone(),
                    message: format!("renderer returned invalid slot {}", buffer.slot),
                });
            }
            if imported.contains_key(&buffer.slot) {
                return Err(BackendError::OutputBuffers {
                    output: output_name.clone(),
                    message: format!("renderer returned duplicate slot {}", buffer.slot),
                });
            }
            validate_output_dmabuf(&buffer.dmabuf, expected_size, expected_format).map_err(
                |message| BackendError::OutputBuffers {
                    output: output_name.clone(),
                    message,
                },
            )?;
            imported.insert(buffer.slot, buffer);
        }
        let target = KmsOutput::new(
            &mut device.drm,
            &device.gbm,
            &descriptor,
            drm_mode,
            imported.into_values().collect(),
        )
        .map_err(|error| BackendError::OutputBuffers {
            output: descriptor.name.clone(),
            message: error.to_string(),
        })?;
        device.native_targets.insert(output_id, target);
        Ok(())
    }

    pub(crate) fn remove_output_buffers(&mut self, output_id: BackendOutputId) {
        if let Some(device) = self.devices.get_mut(&(output_id.device_id as libc::dev_t)) {
            device.native_targets.remove(&output_id);
        }
    }
}

fn validate_output_dmabuf(
    dmabuf: &Dmabuf,
    expected_size: (i32, i32),
    expected_format: OutputFormat,
) -> Result<(), String> {
    let size = dmabuf.size();
    if (size.w, size.h) != expected_size {
        return Err(format!(
            "dma-buf size {}x{} does not match output {}x{}",
            size.w, size.h, expected_size.0, expected_size.1
        ));
    }
    if dmabuf.format() != expected_format.format {
        return Err(format!(
            "dma-buf format {:?} does not match negotiated {:?}",
            dmabuf.format(),
            expected_format.format
        ));
    }
    if dmabuf.num_planes() != usize::try_from(expected_format.plane_count).unwrap_or(usize::MAX) {
        return Err(format!(
            "dma-buf has {} planes, expected {}",
            dmabuf.num_planes(),
            expected_format.plane_count
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs::File, os::fd::OwnedFd, sync::Arc};

    use smithay::backend::allocator::{Format as DrmFormat, Fourcc, Modifier};

    use super::*;

    fn format() -> OutputFormat {
        OutputFormat {
            format: DrmFormat {
                code: Fourcc::Xrgb8888,
                modifier: Modifier::from(9),
            },
            plane_count: 1,
        }
    }

    fn dmabuf(size: (i32, i32), output_format: OutputFormat) -> Dmabuf {
        let fd: OwnedFd = File::open("/dev/null").unwrap().into();
        let mut builder = Dmabuf::builder(
            size,
            output_format.format.code,
            output_format.format.modifier,
            smithay::backend::allocator::dmabuf::DmabufFlags::empty(),
        );
        assert!(builder.add_plane(Arc::new(fd), 0, 256));
        builder.build().unwrap()
    }

    #[test]
    fn output_dmabuf_validation_accepts_the_negotiated_contract() {
        let output_format = format();
        assert!(
            validate_output_dmabuf(
                &dmabuf((1920, 1080), output_format),
                (1920, 1080),
                output_format,
            )
            .is_ok()
        );
    }

    #[test]
    fn output_dmabuf_validation_rejects_size_format_and_plane_mismatches() {
        let output_format = format();
        let buffer = dmabuf((1280, 720), output_format);
        assert!(validate_output_dmabuf(&buffer, (1920, 1080), output_format).is_err());
        let mut different = output_format;
        different.format.code = Fourcc::Argb8888;
        assert!(validate_output_dmabuf(&buffer, (1280, 720), different).is_err());
        let mut different_planes = output_format;
        different_planes.plane_count = 2;
        assert!(validate_output_dmabuf(&buffer, (1280, 720), different_planes).is_err());
    }
}
