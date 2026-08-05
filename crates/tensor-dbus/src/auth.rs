use std::os::fd::OwnedFd;

use compio::{
    buf::{IntoInner, IoBuf},
    driver::{
        ToSharedFd,
        op::{RecvFlags, RecvMsg, RecvResultExt, VecBufResultExt},
    },
    io::AsyncWriteExt,
    net::UnixStream,
};

use crate::{Error, Guid, Result, unix_fd};

const MAX_AUTH_LINE: usize = 4096;
const MAX_AUTH_COMMANDS: usize = 16;

pub(crate) struct Authenticated {
    pub(crate) unix_fd: bool,
    pub(crate) server_guid: Guid,
    pub(crate) buffered: Vec<u8>,
    pub(crate) unix_fds: Vec<OwnedFd>,
}

pub(crate) async fn authenticate_client(
    stream: &mut UnixStream,
    expected_guid: Option<Guid>,
) -> Result<Authenticated> {
    let uid = rustix::process::getuid().as_raw().to_string();
    let mut request = Vec::with_capacity(32 + uid.len() * 2);
    request.extend_from_slice(b"\0AUTH EXTERNAL ");
    push_hex(&mut request, uid.as_bytes());
    request.extend_from_slice(b"\r\n");
    write_all(stream, request).await?;

    let mut reader = LineReader::new(256);
    let response = reader.read(stream).await?;
    let Some(server_guid) = ok_server_guid(&response) else {
        return Err(authentication_response(&response));
    };
    if let Some(expected) = expected_guid
        && expected != server_guid
    {
        return Err(Error::Authentication(format!(
            "server GUID mismatch: expected {expected}, received {server_guid}"
        )));
    }
    write_all(stream, b"NEGOTIATE_UNIX_FD\r\n".to_vec()).await?;
    let response = reader.read(stream).await?;
    let unix_fd = match response.as_slice() {
        b"AGREE_UNIX_FD" => true,
        response if response.starts_with(b"ERROR") => false,
        _ => return Err(authentication_response(&response)),
    };
    write_all(stream, b"BEGIN\r\n".to_vec()).await?;
    Ok(reader.finish(unix_fd, server_guid))
}

pub(crate) async fn authenticate_server(
    stream: &mut UnixStream,
    server_guid: Guid,
    peer_uid: u32,
) -> Result<Authenticated> {
    // A peer may pipeline BEGIN and its first message. Reading one byte at a
    // time prevents the authentication layer from crossing that boundary and
    // keeps SCM_RIGHTS ownership with the message reader.
    let mut reader = LineReader::new(1);
    let mut first = true;
    let mut authenticated = false;
    let mut unix_fd = false;
    for command_index in 0..MAX_AUTH_COMMANDS {
        let mut line = reader.read(stream).await.map_err(|error| {
            Error::Authentication(format!(
                "server handshake failed before command {} (authenticated={authenticated}, unix_fd={unix_fd}): {error}",
                command_index + 1
            ))
        })?;
        if first {
            first = false;
            if line.first() != Some(&0) {
                return Err(Error::Authentication(
                    "first client byte is not NUL".to_owned(),
                ));
            }
            line.remove(0);
        }
        if !authenticated {
            if let Some(argument) = line.strip_prefix(b"AUTH EXTERNAL ") {
                if external_uid(argument) == Some(peer_uid) {
                    write_all(stream, format!("OK {server_guid}\r\n").into_bytes()).await?;
                    authenticated = true;
                } else {
                    write_all(stream, b"REJECTED EXTERNAL\r\n".to_vec()).await?;
                }
            } else if line == b"AUTH EXTERNAL" {
                write_all(stream, b"DATA\r\n".to_vec()).await?;
                let response = reader.read(stream).await?;
                let argument = response.strip_prefix(b"DATA ").unwrap_or_default();
                if argument.is_empty() || external_uid(argument) == Some(peer_uid) {
                    write_all(stream, format!("OK {server_guid}\r\n").into_bytes()).await?;
                    authenticated = true;
                } else {
                    write_all(stream, b"REJECTED EXTERNAL\r\n".to_vec()).await?;
                }
            } else {
                write_all(stream, b"REJECTED EXTERNAL\r\n".to_vec()).await?;
            }
            continue;
        }

        match line.as_slice() {
            b"NEGOTIATE_UNIX_FD" => {
                unix_fd = true;
                write_all(stream, b"AGREE_UNIX_FD\r\n".to_vec()).await?;
            }
            b"BEGIN" => return Ok(reader.finish(unix_fd, server_guid)),
            b"CANCEL" => {
                authenticated = false;
                unix_fd = false;
                write_all(stream, b"REJECTED EXTERNAL\r\n".to_vec()).await?;
            }
            _ => write_all(stream, b"ERROR Unsupported command\r\n".to_vec()).await?,
        }
    }
    Err(Error::Authentication(format!(
        "handshake exceeded the {MAX_AUTH_COMMANDS}-command limit"
    )))
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn push_hex(output: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize]);
        output.push(HEX[(byte & 0xf) as usize]);
    }
}

fn external_uid(argument: &[u8]) -> Option<u32> {
    if argument.is_empty() || !argument.len().is_multiple_of(2) {
        return None;
    }
    let mut decoded = Vec::with_capacity(argument.len() / 2);
    for pair in argument.chunks_exact(2) {
        decoded.push((hex(pair[0])? << 4) | hex(pair[1])?);
    }
    std::str::from_utf8(&decoded).ok()?.parse().ok()
}

const fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn ok_server_guid(response: &[u8]) -> Option<Guid> {
    Guid::parse_bytes(response.strip_prefix(b"OK ")?)
}

fn authentication_response(response: &[u8]) -> Error {
    Error::Authentication(String::from_utf8_lossy(response).into_owned())
}

async fn write_all(stream: &mut UnixStream, buffer: Vec<u8>) -> Result<()> {
    let compio::BufResult(result, _) = stream.write_all(buffer).await;
    result.map_err(Error::Io)
}

struct LineReader {
    buffered: Vec<u8>,
    unix_fds: Vec<OwnedFd>,
    chunk_capacity: usize,
}

impl LineReader {
    fn new(chunk_capacity: usize) -> Self {
        Self {
            buffered: Vec::with_capacity(256),
            unix_fds: Vec::new(),
            chunk_capacity,
        }
    }

    async fn read(&mut self, stream: &mut UnixStream) -> Result<Vec<u8>> {
        loop {
            if let Some(line) = self.take_line()? {
                return Ok(line);
            }
            if self.buffered.len() > MAX_AUTH_LINE {
                return Err(line_too_long());
            }
            let previous = self.buffered.len();
            if self.buffered.capacity() - previous < self.chunk_capacity {
                self.buffered.reserve_exact(self.chunk_capacity);
            }
            let buffer =
                std::mem::take(&mut self.buffered).slice(previous..previous + self.chunk_capacity);
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
            // SAFETY: the kernel-reported lengths are bounded by the submitted buffers.
            let compio::BufResult(result, ([buffer], control)) =
                unsafe { result.map_vec_advanced() };
            self.buffered = buffer.into_inner();
            let (read, control_len, _address, flags) = result.map_err(Error::Io)?;
            unix_fd::decode(&control, control_len, flags, &mut self.unix_fds)?;
            if read == 0 {
                return Err(Error::Authentication(
                    "connection closed during authentication".to_owned(),
                ));
            }
        }
    }

    fn take_line(&mut self) -> Result<Option<Vec<u8>>> {
        let Some(end) = self.buffered.windows(2).position(|bytes| bytes == b"\r\n") else {
            return Ok(None);
        };
        if end > MAX_AUTH_LINE {
            return Err(line_too_long());
        }
        let remaining = self.buffered.split_off(end + 2);
        let mut line = std::mem::replace(&mut self.buffered, remaining);
        line.truncate(end);
        Ok(Some(line))
    }

    fn finish(self, unix_fd: bool, server_guid: Guid) -> Authenticated {
        Authenticated {
            unix_fd,
            server_guid,
            buffered: self.buffered,
            unix_fds: self.unix_fds,
        }
    }
}

fn line_too_long() -> Error {
    Error::Authentication("response line is too long".to_owned())
}

#[cfg(test)]
mod tests {
    use compio::{
        io::AsyncWriteExt,
        net::UnixStream,
        runtime::{Runtime, RuntimeBuilder},
    };

    use super::*;
    use crate::{
        Connection, MethodCallFlags,
        wire::{MethodCall as WireMethodCall, encode_method_call},
    };

    fn runtime() -> Runtime {
        RuntimeBuilder::new()
            .build()
            .expect("Compio runtime is required")
    }

    fn stream_pair() -> (UnixStream, UnixStream) {
        let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        (
            UnixStream::from_std(client).unwrap(),
            UnixStream::from_std(server).unwrap(),
        )
    }

    fn external_argument(uid: u32) -> Vec<u8> {
        let mut argument = Vec::new();
        push_hex(&mut argument, uid.to_string().as_bytes());
        argument
    }

    async fn write(stream: &mut UnixStream, bytes: Vec<u8>) {
        let compio::BufResult(result, _) = stream.write_all(bytes).await;
        result.unwrap();
    }

    #[test]
    fn auth_reader_keeps_bytes_after_a_complete_line() {
        let mut reader = LineReader {
            buffered: b"OK 0123456789abcdef0123456789abcdef\r\nAGREE_UNIX_FD\r\n".to_vec(),
            unix_fds: Vec::new(),
            chunk_capacity: 256,
        };
        let line = reader.take_line().unwrap().unwrap();
        assert!(line.starts_with(b"OK "));
        assert_eq!(reader.buffered, b"AGREE_UNIX_FD\r\n");
    }

    #[test]
    fn external_identity_is_hex_encoded_decimal_uid() {
        assert_eq!(external_uid(b"31303030"), Some(1000));
        assert_eq!(external_uid(b"30"), Some(0));
        assert_eq!(external_uid(b""), None);
        assert_eq!(external_uid(b"zz"), None);
    }

    #[test]
    fn auth_ok_requires_a_complete_server_guid() {
        assert_eq!(
            ok_server_guid(b"OK 0123456789abcdef0123456789ABCDEF")
                .unwrap()
                .to_string(),
            "0123456789abcdef0123456789abcdef"
        );
        assert!(ok_server_guid(b"OK").is_none());
        assert!(ok_server_guid(b"OK short").is_none());
        assert!(ok_server_guid(b"OK 0123456789abcdef0123456789abcdeg").is_none());
    }

    #[test]
    fn server_keeps_a_pipelined_first_message_after_begin() {
        runtime().block_on(async {
            let (mut client, server) = stream_pair();
            let guid = Guid::generate().unwrap();
            let encoded = encode_method_call(
                WireMethodCall {
                    serial: 7,
                    flags: MethodCallFlags::default(),
                    destination: None,
                    path: "/org/tensor/Peer",
                    interface: None,
                    member: "Pipelined",
                },
                &"first-frame",
            )
            .unwrap();
            assert!(encoded.unix_fds.is_empty());
            let mut request = b"\0AUTH EXTERNAL ".to_vec();
            request.extend_from_slice(&external_argument(rustix::process::getuid().as_raw()));
            request.extend_from_slice(b"\r\nNEGOTIATE_UNIX_FD\r\nBEGIN\r\n");
            request.extend_from_slice(&encoded.bytes);

            let server = compio::runtime::spawn(async move {
                let mut connection = Connection::accept_peer(server, guid).await.unwrap();
                let message = connection.receive().await.unwrap();
                assert_eq!(message.serial(), 7);
                assert_eq!(message.member(), Some("Pipelined"));
                assert_eq!(message.body::<String>().unwrap(), "first-frame");
            });
            write(&mut client, request).await;
            server.await.unwrap();
        });
    }

    #[test]
    fn server_accepts_external_data_challenge_form() {
        runtime().block_on(async {
            let (mut client, server) = stream_pair();
            let guid = Guid::generate().unwrap();
            let mut request = b"\0AUTH EXTERNAL\r\nDATA ".to_vec();
            request.extend_from_slice(&external_argument(rustix::process::getuid().as_raw()));
            request.extend_from_slice(b"\r\nBEGIN\r\n");
            let accepted = compio::runtime::spawn(async move {
                Connection::accept_peer(server, guid).await.unwrap()
            });
            write(&mut client, request).await;
            let connection = accepted.await.unwrap();
            assert_eq!(
                connection.peer_credentials().unwrap().user_id,
                rustix::process::getuid().as_raw()
            );
        });
    }

    #[test]
    fn server_rejects_a_forged_external_uid() {
        runtime().block_on(async {
            let (mut client, server) = stream_pair();
            let guid = Guid::generate().unwrap();
            let actual = rustix::process::getuid().as_raw();
            let forged = actual.checked_add(1).unwrap_or(actual.saturating_sub(1));
            let mut request = b"\0AUTH EXTERNAL ".to_vec();
            request.extend_from_slice(&external_argument(forged));
            request.extend_from_slice(b"\r\n");
            let rejected =
                compio::runtime::spawn(async move { Connection::accept_peer(server, guid).await });
            write(&mut client, request).await;
            drop(client);
            assert!(rejected.await.unwrap().is_err());
        });
    }

    #[test]
    fn server_rejects_handshake_without_initial_nul() {
        runtime().block_on(async {
            let (mut client, server) = stream_pair();
            let guid = Guid::generate().unwrap();
            let mut request = b"AUTH EXTERNAL ".to_vec();
            request.extend_from_slice(&external_argument(rustix::process::getuid().as_raw()));
            request.extend_from_slice(b"\r\n");
            let rejected =
                compio::runtime::spawn(async move { Connection::accept_peer(server, guid).await });
            write(&mut client, request).await;
            let error = match rejected.await.unwrap() {
                Ok(_) => panic!("server accepted a handshake without the initial NUL"),
                Err(error) => error,
            };
            assert!(matches!(error, Error::Authentication(message) if message.contains("not NUL")));
        });
    }

    #[test]
    fn server_bounds_authentication_commands() {
        runtime().block_on(async {
            let (mut client, server) = stream_pair();
            let guid = Guid::generate().unwrap();
            let mut request = vec![0];
            for _ in 0..MAX_AUTH_COMMANDS {
                request.extend_from_slice(b"AUTH unsupported\r\n");
            }
            let rejected =
                compio::runtime::spawn(async move { Connection::accept_peer(server, guid).await });
            write(&mut client, request).await;
            let error = match rejected.await.unwrap() {
                Ok(_) => panic!("server accepted more than the authentication command limit"),
                Err(error) => error,
            };
            assert!(
                matches!(error, Error::Authentication(message) if message.contains("command limit"))
            );
        });
    }
}
