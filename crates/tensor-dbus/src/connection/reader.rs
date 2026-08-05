use std::{future::Future, io, os::fd::OwnedFd, pin::Pin};

use compio::{
    buf::{IntoInner, IoBuf},
    driver::{
        ToSharedFd,
        op::{RecvFlags, RecvMsg, RecvResultExt, VecBufResultExt},
    },
    net::UnixStream,
};

use crate::{
    Error, Message, Result, unix_fd,
    wire::{FIXED_HEADER_LEN, decode_message, frame_len},
};

type ReadFuture = Pin<Box<dyn Future<Output = ReadOutput>>>;
const COMMON_FRAME_CAPACITY: usize = 512;

struct ReadOutput {
    stream: UnixStream,
    result: Result<Message>,
}

pub(super) struct MessageReader {
    stream: Option<UnixStream>,
    operation: Option<ReadFuture>,
    buffered: Vec<u8>,
    unix_fds: Vec<OwnedFd>,
    unix_fd: bool,
    failed: bool,
}

impl MessageReader {
    pub(super) fn new(
        stream: UnixStream,
        unix_fd: bool,
        buffered: Vec<u8>,
        unix_fds: Vec<OwnedFd>,
    ) -> Self {
        Self {
            stream: Some(stream),
            operation: None,
            buffered,
            unix_fds,
            unix_fd,
            failed: false,
        }
    }

    pub(super) const fn is_failed(&self) -> bool {
        self.failed
    }

    /// Reads one frame while retaining every in-flight operation in `self`.
    /// Dropping the outer future therefore pauses, rather than cancels, I/O.
    pub(super) async fn read(&mut self) -> Result<Message> {
        if self.failed {
            return Err(Error::ConnectionUnusable);
        }
        if self.operation.is_none() {
            self.start_read();
        }
        let output = self.operation.as_mut().unwrap().await;
        self.operation = None;
        self.stream = Some(output.stream);
        output.result.inspect_err(|_| self.failed = true)
    }

    fn start_read(&mut self) {
        let mut stream = self.stream.take().unwrap();
        let unix_fd = self.unix_fd;
        let buffered = std::mem::take(&mut self.buffered);
        let unix_fds = std::mem::take(&mut self.unix_fds);
        self.operation = Some(Box::pin(async move {
            let result = read_message_from(&mut stream, unix_fd, buffered, unix_fds).await;
            ReadOutput { stream, result }
        }));
    }
}

async fn read_message_from(
    stream: &mut UnixStream,
    unix_fd_negotiated: bool,
    mut bytes: Vec<u8>,
    mut unix_fds: Vec<OwnedFd>,
) -> Result<Message> {
    if bytes.capacity() < COMMON_FRAME_CAPACITY {
        bytes.reserve(COMMON_FRAME_CAPACITY - bytes.capacity());
    }
    while bytes.len() < FIXED_HEADER_LEN {
        bytes = read_chunk(stream, bytes, FIXED_HEADER_LEN, &mut unix_fds).await?;
    }
    validate_received_unix_fds(unix_fd_negotiated, &unix_fds)?;
    let fixed: &[u8; FIXED_HEADER_LEN] = bytes.as_slice().try_into().unwrap();
    let total = frame_len(fixed)?;
    bytes.reserve_exact(total - FIXED_HEADER_LEN);
    while bytes.len() < total {
        bytes = read_chunk(stream, bytes, total, &mut unix_fds).await?;
    }
    validate_received_unix_fds(unix_fd_negotiated, &unix_fds)?;
    decode_message(bytes, unix_fds)
}

async fn read_chunk(
    stream: &mut UnixStream,
    bytes: Vec<u8>,
    end: usize,
    unix_fds: &mut Vec<OwnedFd>,
) -> Result<Vec<u8>> {
    let previous = bytes.len();
    let buffer = bytes.slice(previous..end);
    let control = unix_fd::ControlBuffer::new();
    let operation = RecvMsg::new(
        stream.to_shared_fd(),
        [buffer],
        control,
        RecvFlags::CMSG_CLOEXEC,
    );
    let result = compio::runtime::submit(operation)
        .await
        .into_inner()
        .map_addr();
    // SAFETY: RecvMsg's successful byte and control lengths come directly
    // from the kernel and are bounded by the submitted buffers.
    let compio::BufResult(result, ([buffer], control)) = unsafe { result.map_vec_advanced() };
    let bytes = buffer.into_inner();
    let (read, control_len, _address, flags) = result.map_err(Error::Io)?;
    unix_fd::decode(&control, control_len, flags, unix_fds)?;
    if read == 0 {
        return Err(Error::Io(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "D-Bus connection closed during a message",
        )));
    }
    Ok(bytes)
}

fn validate_received_unix_fds(negotiated: bool, unix_fds: &[OwnedFd]) -> Result<()> {
    if !negotiated && !unix_fds.is_empty() {
        return Err(Error::UnixFdUnsupported);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs::File, os::fd::OwnedFd, time::Duration};

    use compio::{
        driver::{DriverType, ProactorBuilder},
        io::AsyncWriteExt,
        runtime::RuntimeBuilder,
    };

    use super::*;
    use crate::{
        MessageKind,
        wire::{Outgoing, encode_outgoing},
    };

    #[test]
    fn received_fds_require_negotiation() {
        let fd: OwnedFd = File::open("/dev/null").unwrap().into();
        assert!(matches!(
            validate_received_unix_fds(false, &[fd]),
            Err(Error::UnixFdUnsupported)
        ));
        assert!(validate_received_unix_fds(true, &[]).is_ok());
    }

    #[test]
    fn cancelled_read_resumes_a_partially_received_frame() {
        let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        let mut proactor = ProactorBuilder::new();
        proactor.driver_type(DriverType::IoUring);
        let mut builder = RuntimeBuilder::new();
        builder.with_proactor(proactor);
        let runtime = builder.build().expect("io_uring runtime is required");

        runtime.block_on(async move {
            let client = UnixStream::from_std(client).unwrap();
            let mut server = UnixStream::from_std(server).unwrap();
            let encoded = encode_outgoing(
                Outgoing {
                    kind: MessageKind::Signal,
                    flags: 0,
                    serial: 7,
                    reply_serial: None,
                    path: Some("/org/tensor/Test"),
                    interface: Some("org.tensor.Test"),
                    member: Some("Changed"),
                    error_name: None,
                    destination: None,
                },
                &"split-frame",
            )
            .unwrap();
            let bytes = encoded.bytes;
            let sender = compio::runtime::spawn(async move {
                let split = 8;
                let compio::BufResult(result, _) = server.write_all(bytes[..split].to_vec()).await;
                result.unwrap();
                compio::runtime::time::sleep(Duration::from_millis(40)).await;
                let compio::BufResult(result, _) = server.write_all(bytes[split..].to_vec()).await;
                result.unwrap();
            });

            let mut reader = MessageReader::new(client, false, Vec::new(), Vec::new());
            let timed =
                compio::runtime::time::timeout(Duration::from_millis(10), reader.read()).await;
            assert!(timed.is_err(), "the split frame should still be incomplete");

            let message = reader.read().await.unwrap();
            assert_eq!(message.kind(), MessageKind::Signal);
            assert_eq!(message.serial(), 7);
            assert_eq!(message.body::<String>().unwrap(), "split-frame");
            sender.await.unwrap();
        });
    }
}
