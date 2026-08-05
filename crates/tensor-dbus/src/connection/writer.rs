use std::{future::Future, io, pin::Pin};

use compio::{
    buf::IoBuf,
    io::{AsyncWriteExt, ancillary::AsyncWriteAncillary as _},
    net::UnixStream,
};

use crate::{Error, Result, unix_fd, wire::EncodedMessage};

type WriteFuture = Pin<Box<dyn Future<Output = (UnixStream, Result<()>)>>>;

pub(super) struct MessageWriter {
    stream: Option<UnixStream>,
    operation: Option<WriteFuture>,
    unix_fd: bool,
    failed: bool,
}

impl MessageWriter {
    pub(super) fn new(stream: UnixStream, unix_fd: bool) -> Self {
        Self {
            stream: Some(stream),
            operation: None,
            unix_fd,
            failed: false,
        }
    }

    pub(super) const fn is_failed(&self) -> bool {
        self.failed
    }

    pub(super) async fn flush(&mut self) -> Result<()> {
        if self.failed {
            return Err(Error::ConnectionUnusable);
        }
        self.finish_active().await
    }

    pub(super) async fn close(mut self) -> Result<()> {
        let write_result = self.finish_active().await;
        let close_result = match self.stream.take() {
            Some(stream) => stream.close().await.map_err(Error::Io),
            None => Ok(()),
        };
        write_result.and(close_result)
    }

    /// Serializes writes and retains the active future across caller
    /// cancellation. A later write first finishes the abandoned operation.
    pub(super) async fn write(&mut self, encoded: EncodedMessage) -> Result<()> {
        if self.failed {
            return Err(Error::ConnectionUnusable);
        }
        self.flush().await?;
        if !self.unix_fd && !encoded.unix_fds.is_empty() {
            return Err(Error::UnixFdUnsupported);
        }
        let stream = self.stream.take().unwrap();
        self.operation = Some(Box::pin(write_message(stream, encoded)));
        self.finish_active().await
    }

    async fn finish_active(&mut self) -> Result<()> {
        let Some(operation) = &mut self.operation else {
            return Ok(());
        };
        let (stream, result) = operation.await;
        self.operation = None;
        self.stream = Some(stream);
        result.inspect_err(|_| {
            self.failed = true;
        })
    }
}

async fn write_message(
    mut stream: UnixStream,
    encoded: EncodedMessage,
) -> (UnixStream, Result<()>) {
    let EncodedMessage { bytes, unix_fds } = encoded;
    let result = if unix_fds.is_empty() {
        let compio::BufResult(result, _) = stream.write_all(bytes).await;
        result.map_err(Error::Io)
    } else {
        write_with_fds(&mut stream, bytes, unix_fds).await
    };
    (stream, result)
}

async fn write_with_fds(
    stream: &mut UnixStream,
    bytes: Vec<u8>,
    unix_fds: Vec<zvariant::OwnedFd>,
) -> Result<()> {
    let control = unix_fd::encode(&unix_fds)?;
    let total = bytes.len();
    let compio::BufResult(result, (bytes, _control)) =
        stream.write_with_ancillary(bytes, control).await;
    let written = result.map_err(Error::Io)?;
    drop(unix_fds);
    if written == 0 {
        return Err(Error::Io(io::Error::new(
            io::ErrorKind::WriteZero,
            "D-Bus connection accepted no message bytes with SCM_RIGHTS",
        )));
    }
    if written < total {
        let compio::BufResult(result, _) = stream.write_all(bytes.slice(written..)).await;
        result.map_err(Error::Io)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use compio::{
        driver::{DriverType, ProactorBuilder},
        io::AsyncReadExt,
        runtime::RuntimeBuilder,
    };

    use super::*;

    #[test]
    fn cancelled_write_is_completed_before_the_next_message() {
        const FIRST_LEN: usize = 4 * 1024 * 1024;
        const SECOND_LEN: usize = 32;

        let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        let mut proactor = ProactorBuilder::new();
        proactor.driver_type(DriverType::IoUring);
        let mut builder = RuntimeBuilder::new();
        builder.with_proactor(proactor);
        let runtime = builder.build().expect("io_uring runtime is required");

        runtime.block_on(async move {
            let client = UnixStream::from_std(client).unwrap();
            let mut server = UnixStream::from_std(server).unwrap();
            let mut writer = MessageWriter::new(client, false);
            let first = EncodedMessage {
                bytes: vec![0xa5; FIRST_LEN],
                unix_fds: Vec::new(),
            };
            let timed =
                compio::runtime::time::timeout(Duration::from_millis(10), writer.write(first))
                    .await;
            assert!(timed.is_err(), "the peer is not draining the large write");

            let receiver = compio::runtime::spawn(async move {
                let compio::BufResult(result, bytes) = server
                    .read_exact(Vec::with_capacity(FIRST_LEN + SECOND_LEN))
                    .await;
                result.unwrap();
                bytes
            });
            writer
                .write(EncodedMessage {
                    bytes: vec![0x5a; SECOND_LEN],
                    unix_fds: Vec::new(),
                })
                .await
                .unwrap();
            let bytes = receiver.await.unwrap();
            assert!(bytes[..FIRST_LEN].iter().all(|byte| *byte == 0xa5));
            assert!(bytes[FIRST_LEN..].iter().all(|byte| *byte == 0x5a));
        });
    }
}
