use std::{io, os::fd::AsFd};

use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};
use tensor_host::DrmFormat;
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_feedback_v1::{
    self, ZwpLinuxDmabufFeedbackV1,
};
use wayland_server::Resource;

const FORMAT_ENTRY_SIZE: usize = 16;

#[derive(Debug)]
pub(super) struct DmabufFeedback {
    main_device: u64,
    table: std::os::fd::OwnedFd,
    table_size: u32,
    indices: Box<[u8]>,
}

impl DmabufFeedback {
    pub(super) fn new(main_device: u64, formats: &[DrmFormat]) -> io::Result<Self> {
        if formats.is_empty() || formats.len() > usize::from(u16::MAX) + 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "linux-dmabuf feedback requires 1..=65536 formats",
            ));
        }
        let bytes = encode_format_table(formats)?;
        let table_size = u32::try_from(bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "format table exceeds u32"))?;
        let table = memfd_create(
            "tensor-dmabuf-feedback",
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )?;
        write_all(&table, &bytes)?;
        fcntl_add_seals(
            &table,
            SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE | SealFlags::SEAL,
        )?;
        let indices = (0..formats.len())
            .flat_map(|index| (index as u16).to_ne_bytes())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            main_device,
            table,
            table_size,
            indices,
        })
    }

    pub(super) fn send(&self, feedback: &ZwpLinuxDmabufFeedbackV1) {
        if feedback.version() <= 5 {
            feedback.main_device(self.main_device.to_ne_bytes().to_vec());
        }
        feedback.format_table(self.table.as_fd(), self.table_size);
        feedback.tranche_target_device(self.main_device.to_ne_bytes().to_vec());
        let flags = if feedback.version() >= 6 {
            zwp_linux_dmabuf_feedback_v1::TrancheFlags::Sampling
        } else {
            zwp_linux_dmabuf_feedback_v1::TrancheFlags::empty()
        };
        feedback.tranche_flags(flags);
        feedback.tranche_formats(self.indices.to_vec());
        feedback.tranche_done();
        feedback.done();
    }
}

fn encode_format_table(formats: &[DrmFormat]) -> io::Result<Vec<u8>> {
    let capacity = formats
        .len()
        .checked_mul(FORMAT_ENTRY_SIZE)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "format table overflows"))?;
    let mut bytes = Vec::with_capacity(capacity);
    for format in formats {
        bytes.extend_from_slice(&format.code.raw().to_ne_bytes());
        bytes.extend_from_slice(&0_u32.to_ne_bytes());
        bytes.extend_from_slice(&format.modifier.raw().to_ne_bytes());
    }
    Ok(bytes)
}

fn write_all(fd: &impl AsFd, mut bytes: &[u8]) -> io::Result<()> {
    let mut offset = 0;
    while !bytes.is_empty() {
        match rustix::io::pwrite(fd, bytes, offset) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => {
                bytes = &bytes[written..];
                offset += written as u64;
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rustix::fs::{SealFlags, fcntl_get_seals};
    use tensor_host::{Fourcc, Modifier};

    use super::*;

    #[test]
    fn format_table_is_native_endian_and_has_no_staging_padding() {
        let formats = [
            DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9)),
            DrmFormat::new(Fourcc::ARGB8888, Modifier::LINEAR),
        ];
        let table = encode_format_table(&formats).unwrap();
        assert_eq!(table.len(), 2 * FORMAT_ENTRY_SIZE);
        assert_eq!(&table[0..4], &Fourcc::XRGB8888.raw().to_ne_bytes());
        assert_eq!(&table[4..8], &[0; 4]);
        assert_eq!(&table[8..16], &9_u64.to_ne_bytes());
    }

    #[test]
    fn feedback_table_is_immutable_after_publication() {
        let feedback = DmabufFeedback::new(
            7,
            &[DrmFormat::new(Fourcc::XRGB8888, Modifier::from_raw(9))],
        )
        .unwrap();
        let seals = fcntl_get_seals(&feedback.table).unwrap();
        assert!(seals.contains(SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE));
    }
}
