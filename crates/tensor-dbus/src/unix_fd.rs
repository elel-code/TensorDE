use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

use compio::io::ancillary::{AncillaryBuf, ReturnFlags, ancillary_space};

use crate::{Error, Result, wire::MAX_UNIX_FDS};

const CONTROL_CAPACITY: usize = ancillary_space::<RawFd>() * MAX_UNIX_FDS;

pub(crate) type ControlBuffer = AncillaryBuf<CONTROL_CAPACITY>;

pub(crate) fn encode(fds: &[zvariant::OwnedFd]) -> Result<ControlBuffer> {
    if fds.len() > MAX_UNIX_FDS {
        return Err(Error::UnixFdLimit {
            count: fds.len(),
            limit: MAX_UNIX_FDS,
        });
    }
    let mut control = ControlBuffer::new();
    {
        let mut builder = control.builder();
        for fd in fds {
            builder
                .push(libc::SOL_SOCKET, libc::SCM_RIGHTS, &fd.as_raw_fd())
                .map_err(|error| Error::InvalidAncillary(error.to_string()))?;
        }
    }
    Ok(control)
}

pub(crate) fn decode(
    control: &ControlBuffer,
    control_len: usize,
    flags: ReturnFlags,
    fds: &mut Vec<OwnedFd>,
) -> Result<()> {
    if control_len > control.len() {
        return Err(Error::InvalidAncillary(
            "reported control length exceeds the receive buffer".to_owned(),
        ));
    }

    let bytes = &control[..control_len];
    let header_size = std::mem::size_of::<libc::cmsghdr>();
    let data_offset = unsafe { libc::CMSG_LEN(0) as usize };
    let alignment = std::mem::align_of::<libc::cmsghdr>();
    let mut received = Vec::new();
    let mut offset = 0;
    while offset + header_size <= bytes.len() {
        // SAFETY: the range was checked and read_unaligned avoids imposing an
        // alignment requirement on the borrowed byte slice.
        let header =
            unsafe { std::ptr::read_unaligned(bytes.as_ptr().add(offset).cast::<libc::cmsghdr>()) };
        let message_len = header.cmsg_len as usize;
        let message_end = offset
            .checked_add(message_len)
            .ok_or_else(|| Error::InvalidAncillary("control-message length overflow".to_owned()))?;
        if message_len < data_offset {
            return Err(Error::InvalidAncillary(
                "invalid control-message length".to_owned(),
            ));
        }
        if message_end > bytes.len() {
            if header.cmsg_level == libc::SOL_SOCKET && header.cmsg_type == libc::SCM_RIGHTS {
                let available = &bytes[offset + data_offset..];
                let complete = available.len() - available.len() % std::mem::size_of::<RawFd>();
                own_rights(&available[..complete], &mut received)?;
            }
            return if flags.contains(ReturnFlags::CTRUNC) {
                Err(Error::AncillaryTruncated)
            } else {
                Err(Error::InvalidAncillary(
                    "invalid control-message length".to_owned(),
                ))
            };
        }
        if header.cmsg_level == libc::SOL_SOCKET && header.cmsg_type == libc::SCM_RIGHTS {
            let data_start = offset.checked_add(data_offset).ok_or_else(|| {
                Error::InvalidAncillary("control-message data offset overflow".to_owned())
            })?;
            let data = &bytes[data_start..message_end];
            own_rights(data, &mut received)?;
        }
        offset = align(message_end, alignment).ok_or_else(|| {
            Error::InvalidAncillary("control-message alignment overflow".to_owned())
        })?;
    }
    if offset < bytes.len() && bytes[offset..].iter().any(|byte| *byte != 0) {
        return Err(Error::InvalidAncillary(
            "non-zero trailing bytes after control messages".to_owned(),
        ));
    }
    if flags.contains(ReturnFlags::CTRUNC) {
        return Err(Error::AncillaryTruncated);
    }
    let count = fds
        .len()
        .checked_add(received.len())
        .ok_or(Error::UnixFdLimit {
            count: usize::MAX,
            limit: MAX_UNIX_FDS,
        })?;
    if count > MAX_UNIX_FDS {
        return Err(Error::UnixFdLimit {
            count,
            limit: MAX_UNIX_FDS,
        });
    }
    fds.append(&mut received);
    Ok(())
}

fn own_rights(bytes: &[u8], received: &mut Vec<OwnedFd>) -> Result<()> {
    if !bytes.len().is_multiple_of(std::mem::size_of::<RawFd>()) {
        return Err(Error::InvalidAncillary(
            "SCM_RIGHTS payload is not an array of file descriptors".to_owned(),
        ));
    }
    let mut invalid = false;
    for raw in bytes.chunks_exact(std::mem::size_of::<RawFd>()) {
        let raw = RawFd::from_ne_bytes(raw.try_into().unwrap());
        if raw < 0 {
            invalid = true;
            continue;
        }
        // SAFETY: SCM_RIGHTS installs a new descriptor in this process,
        // transferring ownership to the receiver.
        received.push(unsafe { OwnedFd::from_raw_fd(raw) });
    }
    if invalid {
        return Err(Error::InvalidAncillary(
            "SCM_RIGHTS contained a negative file descriptor".to_owned(),
        ));
    }
    Ok(())
}

const fn align(value: usize, alignment: usize) -> Option<usize> {
    match value.checked_add(alignment - 1) {
        Some(value) => Some(value & !(alignment - 1)),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        os::fd::{AsFd, IntoRawFd},
    };

    use super::*;

    static FD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn fd_limit_error_closes_the_uncommitted_descriptor() {
        let _guard = FD_TEST_LOCK.lock().unwrap();
        let mut existing: Vec<OwnedFd> = (0..MAX_UNIX_FDS)
            .map(|_| File::open("/dev/null").unwrap().into())
            .collect();
        let (control, raw) = control_with_unowned_descriptor();
        assert!(matches!(
            decode(&control, control.len(), ReturnFlags::empty(), &mut existing),
            Err(Error::UnixFdLimit { .. })
        ));
        assert_descriptor_closed(raw);
    }

    #[test]
    fn truncation_error_closes_every_complete_descriptor() {
        let _guard = FD_TEST_LOCK.lock().unwrap();
        let (control, raw) = control_with_unowned_descriptor();
        assert!(matches!(
            decode(
                &control,
                control.len(),
                ReturnFlags::CTRUNC,
                &mut Vec::new()
            ),
            Err(Error::AncillaryTruncated)
        ));
        assert_descriptor_closed(raw);
    }

    fn control_with_unowned_descriptor() -> (ControlBuffer, RawFd) {
        let file = File::open("/dev/null").unwrap();
        let raw = rustix::io::dup(file.as_fd()).unwrap().into_raw_fd();
        let mut control = ControlBuffer::new();
        control
            .builder()
            .push(libc::SOL_SOCKET, libc::SCM_RIGHTS, &raw)
            .unwrap();
        (control, raw)
    }

    fn assert_descriptor_closed(raw: RawFd) {
        // SAFETY: F_GETFD only inspects the descriptor number and does not
        // dereference memory. EBADF is the expected result after ownership was
        // dropped by the decoder's error path.
        assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EBADF)
        );
    }
}
