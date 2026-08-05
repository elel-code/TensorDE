use std::{
    fs::{self, OpenOptions},
    os::{fd::OwnedFd, unix::fs::MetadataExt},
    path::{Path, PathBuf},
};

use gbm::{BufferObject, BufferObjectFlags, Device as GbmDevice, Format as GbmFormat, Modifier};

use super::{SmokeError, feedback::DmabufFormat};

pub(super) fn find_render_node(device: u64) -> Result<PathBuf, SmokeError> {
    let directory = Path::new("/dev/dri");
    for entry in fs::read_dir(directory).map_err(|source| SmokeError::ReadDrmDirectory {
        path: directory.to_owned(),
        source,
    })? {
        let entry = entry.map_err(|source| SmokeError::ReadDrmDirectory {
            path: directory.to_owned(),
            source,
        })?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("renderD") {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|source| SmokeError::ReadDrmDirectory {
                path: entry.path(),
                source,
            })?;
        if metadata.rdev() == device {
            return Ok(entry.path());
        }
    }
    Err(SmokeError::RenderNodeNotFound(device))
}

pub(super) struct BufferPool {
    // Buffers must drop before their GBM device: each buffer carries a GBM
    // device pointer, while the device owns the underlying DRM file descriptor.
    pub(super) buffers: Vec<BufferObject<()>>,
    _device: GbmDevice<OwnedFd>,
    pub(super) format: DmabufFormat,
}

impl BufferPool {
    pub(super) fn allocate(
        render_device: &Path,
        candidates: &[DmabufFormat],
        width: u32,
        height: u32,
        count: usize,
    ) -> Result<Self, SmokeError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(render_device)
            .map_err(|source| SmokeError::OpenRenderNode {
                path: render_device.to_owned(),
                source,
            })?;
        let drm_fd: OwnedFd = file.into();
        let device = GbmDevice::new(drm_fd).map_err(|source| SmokeError::Gbm {
            context: "create GBM device",
            source,
        })?;
        let mut attempted = Vec::new();
        for candidate in candidates {
            let Ok(fourcc) = GbmFormat::try_from(candidate.fourcc) else {
                continue;
            };
            let modifier = Modifier::from(candidate.modifier);
            let mut buffers = Vec::with_capacity(count);
            let mut failed = None;
            for slot in 0..count {
                let result = device.create_buffer_object_with_modifiers2(
                    width,
                    height,
                    fourcc,
                    std::iter::once(modifier),
                    BufferObjectFlags::RENDERING,
                );
                let mut buffer = match result {
                    Ok(buffer) => buffer,
                    Err(error) => {
                        failed = Some(format!("slot {slot}: {error}"));
                        break;
                    }
                };
                if buffer.modifier() != modifier || buffer.plane_count() != 1 {
                    failed = Some(format!(
                        "slot {slot}: GBM returned modifier {:#018x} with {} planes",
                        u64::from(buffer.modifier()),
                        buffer.plane_count(),
                    ));
                    break;
                }
                if let Err(error) = fill_buffer(&mut buffer) {
                    eprintln!(
                        "tensor-dmabuf-smoke: GBM CPU fill unavailable for slot {slot}: {error}; continuing with a GPU-valid buffer"
                    );
                }
                buffers.push(buffer);
            }
            if buffers.len() == count {
                return Ok(Self {
                    buffers,
                    _device: device,
                    format: *candidate,
                });
            }
            attempted.push(format!(
                "fourcc={:#010x} modifier={:#018x}: {}",
                candidate.fourcc,
                candidate.modifier,
                failed.unwrap_or_else(|| "allocation did not complete".to_owned()),
            ));
        }
        Err(SmokeError::NoGbmAllocation(attempted.join("; ")))
    }
}

fn fill_buffer(buffer: &mut BufferObject<()>) -> std::io::Result<()> {
    let width = buffer.width();
    let height = buffer.height();
    buffer.map_mut(0, 0, width, height, |mapping| {
        mapping.buffer_mut().fill(0xff)
    })
}
