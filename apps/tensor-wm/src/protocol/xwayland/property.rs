//! Completion-driven X11 property queries on a dedicated connection.

mod text;

use std::{
    io,
    os::fd::{AsFd, OwnedFd},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

use compio::{
    buf::IoBuf,
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
use rustix::{
    io::fcntl_dupfd_cloexec,
    net::{Shutdown, shutdown},
};
use tensor_runtime::WorkerRx;
use tensor_runtime::{EventfdWake, WakeSink, WorkerBridge, WorkerTx, io_uring_runtime};
use tensor_util::LogicalSize;
use x11rb::protocol::xproto::AtomEnum;

pub(crate) use text::X11WindowMetadata;

const REQUEST_CAPACITY: usize = 64;
const MAX_PROPERTY_WORDS: usize = 32;
const MAX_PROPERTY_BYTES: usize = MAX_PROPERTY_WORDS * size_of::<u32>();
const X11_GET_PROPERTY: u8 = 20;
const X11_PROTOCOL_MAJOR: u16 = 11;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct X11PropertyTarget {
    pub(crate) window: u32,
    pub(crate) generation: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum X11PropertyRequest {
    Initial {
        target: X11PropertyTarget,
        net_wm_state: u32,
        wm_protocols: u32,
        net_wm_name: u32,
        utf8_string: u32,
    },
    TransientFor {
        target: X11PropertyTarget,
    },
    NormalHints {
        target: X11PropertyTarget,
    },
    NetState {
        target: X11PropertyTarget,
        net_wm_state: u32,
    },
    WmHints {
        target: X11PropertyTarget,
    },
    Protocols {
        target: X11PropertyTarget,
        wm_protocols: u32,
    },
    Title {
        target: X11PropertyTarget,
        net_wm_name: u32,
        utf8_string: u32,
    },
    Class {
        target: X11PropertyTarget,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum X11PropertyUpdate {
    Initial(Box<X11InitialProperties>),
    TransientFor(Option<u32>),
    NormalHints(X11SizeHints),
    NetState(X11AtomList),
    WmHints(X11WmHints),
    Protocols(X11AtomList),
    Title(String),
    Class(String),
}

#[derive(Clone, Debug)]
pub(crate) struct X11InitialProperties {
    pub(crate) transient_for: Option<u32>,
    pub(crate) size_hints: X11SizeHints,
    pub(crate) net_state: X11AtomList,
    pub(crate) wm_hints: X11WmHints,
    pub(crate) protocols: X11AtomList,
    pub(crate) metadata: X11WindowMetadata,
}

#[derive(Clone, Debug)]
pub(crate) struct X11PropertyResult {
    pub(crate) target: X11PropertyTarget,
    pub(crate) update: X11PropertyUpdate,
}

impl X11PropertyResult {
    pub(crate) const fn targets(&self, target: X11PropertyTarget) -> bool {
        self.target.window == target.window && self.target.generation == target.generation
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct X11SizeHints {
    pub(crate) min_size: Option<LogicalSize<i32>>,
    pub(crate) max_size: Option<LogicalSize<i32>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct X11WmHints {
    pub(crate) accepts_input: bool,
}

impl Default for X11WmHints {
    fn default() -> Self {
        Self {
            // ICCCM specifies the input hint as true when the hint is absent.
            accepts_input: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct X11AtomList {
    values: [u32; MAX_PROPERTY_WORDS],
    len: u8,
}

impl X11AtomList {
    pub(crate) fn as_slice(&self) -> &[u32] {
        &self.values[..usize::from(self.len)]
    }

    pub(crate) fn contains(&self, atom: u32) -> bool {
        self.as_slice().contains(&atom)
    }

    pub(crate) fn set_member(&mut self, atom: u32, enabled: bool) -> bool {
        let len = usize::from(self.len);
        if let Some(index) = self.values[..len].iter().position(|value| *value == atom) {
            if !enabled {
                self.values.copy_within(index + 1..len, index);
                self.len -= 1;
            }
            return true;
        }
        if !enabled {
            return true;
        }
        if len == MAX_PROPERTY_WORDS {
            return false;
        }
        self.values[len] = atom;
        self.len += 1;
        true
    }

    fn from_words(words: PropertyWords) -> Self {
        Self {
            values: words.values,
            len: words.len,
        }
    }
}

pub(crate) struct X11PropertyRuntime {
    wake: Arc<EventfdWake>,
    socket: OwnedFd,
    stopping: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

pub(crate) struct X11PropertyRuntimeBuilder {
    wake: Arc<EventfdWake>,
    pending: WorkerRx<X11PropertyRequest>,
}

impl X11PropertyRuntime {
    pub(crate) fn prepare()
    -> Result<(X11PropertyRuntimeBuilder, WorkerTx<X11PropertyRequest>), io::Error> {
        let wake = Arc::new(EventfdWake::new().map_err(io::Error::other)?);
        let (requests, pending) = WorkerBridge::bounded_with_wake(
            REQUEST_CAPACITY,
            Arc::clone(&wake) as Arc<dyn WakeSink>,
        );
        Ok((X11PropertyRuntimeBuilder { wake, pending }, requests))
    }
}

impl X11PropertyRuntimeBuilder {
    pub(crate) fn start(
        self,
        display: u32,
        results: WorkerTx<X11PropertyResult>,
        failures: WorkerTx<String>,
    ) -> Result<X11PropertyRuntime, io::Error> {
        let Self { wake, pending } = self;
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_wake = Arc::clone(&wake);
        let thread_stopping = Arc::clone(&stopping);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("tensor-x11-properties".to_owned())
            .spawn(move || {
                let runtime = match io_uring_runtime(2) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let mut completion = match thread_wake.completion_reader() {
                        Ok(completion) => completion,
                        Err(error) => {
                            let _ = ready_tx.send(Err(io::Error::other(error)));
                            return;
                        }
                    };
                    let mut stream = match connect(display).await {
                        Ok(stream) => stream,
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                            return;
                        }
                    };
                    let socket = match fcntl_dupfd_cloexec(stream.as_fd(), 0) {
                        Ok(socket) => socket,
                        Err(error) => {
                            let _ = ready_tx.send(Err(io::Error::from(error)));
                            return;
                        }
                    };
                    if ready_tx.send(Ok(socket)).is_err() {
                        return;
                    }
                    while completion.completed().await.is_ok() {
                        if thread_stopping.load(Ordering::Acquire) {
                            return;
                        }
                        while let Some(request) = pending.try_recv() {
                            match execute(&mut stream, request).await {
                                Ok(result) => {
                                    if results.try_send(result).is_err() {
                                        let _ = failures.try_send(
                                            "X11 property result queue is unavailable".to_owned(),
                                        );
                                        return;
                                    }
                                }
                                Err(error) => {
                                    let _ = failures.try_send(error.to_string());
                                    return;
                                }
                            }
                        }
                    }
                });
            })?;

        match ready_rx.recv() {
            Ok(Ok(socket)) => Ok(X11PropertyRuntime {
                wake,
                socket,
                stopping,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(io::Error::other(
                    "X11 property runtime stopped during startup",
                ))
            }
        }
    }
}

impl Drop for X11PropertyRuntime {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = shutdown(&self.socket, Shutdown::Both);
        self.wake.wake();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

async fn connect(display: u32) -> io::Result<UnixStream> {
    let path = format!("/tmp/.X11-unix/X{display}");
    let mut stream = UnixStream::connect(path).await?;
    let mut setup = [0_u8; 12];
    setup[0] = if cfg!(target_endian = "little") {
        b'l'
    } else {
        b'B'
    };
    put_u16(&mut setup[2..4], X11_PROTOCOL_MAJOR);
    stream.write_all(setup).await.0?;

    let (result, header) = stream.read_exact([0_u8; 8]).await.into_parts();
    result?;
    let mut extra_len = usize::from(get_u16(&header[6..8])) * 4;
    while extra_len > 0 {
        let chunk_len = extra_len.min(4096);
        let chunk = [0_u8; 4096].slice(..chunk_len);
        stream.read_exact(chunk).await.0?;
        extra_len -= chunk_len;
    }
    if header[0] != 1 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "X11 property connection setup was rejected",
        ));
    }
    Ok(stream)
}

async fn execute(
    stream: &mut UnixStream,
    request: X11PropertyRequest,
) -> io::Result<X11PropertyResult> {
    match request {
        X11PropertyRequest::Initial {
            target,
            net_wm_state,
            wm_protocols,
            net_wm_name,
            utf8_string,
        } => {
            let mut requests = [0_u8; 192];
            requests[0..24].copy_from_slice(&get_property_request(
                target.window,
                AtomEnum::WM_TRANSIENT_FOR.into(),
                AtomEnum::WINDOW.into(),
                1,
            ));
            requests[24..48].copy_from_slice(&get_property_request(
                target.window,
                AtomEnum::WM_NORMAL_HINTS.into(),
                AtomEnum::WM_SIZE_HINTS.into(),
                18,
            ));
            requests[48..72].copy_from_slice(&get_property_request(
                target.window,
                net_wm_state,
                AtomEnum::ATOM.into(),
                MAX_PROPERTY_WORDS as u32,
            ));
            requests[72..96].copy_from_slice(&get_property_request(
                target.window,
                AtomEnum::WM_HINTS.into(),
                AtomEnum::WM_HINTS.into(),
                9,
            ));
            requests[96..120].copy_from_slice(&get_property_request(
                target.window,
                wm_protocols,
                AtomEnum::ATOM.into(),
                MAX_PROPERTY_WORDS as u32,
            ));
            requests[120..144].copy_from_slice(&get_property_request(
                target.window,
                net_wm_name,
                utf8_string,
                1024,
            ));
            requests[144..168].copy_from_slice(&get_property_request(
                target.window,
                AtomEnum::WM_NAME.into(),
                AtomEnum::STRING.into(),
                1024,
            ));
            requests[168..192].copy_from_slice(&get_property_request(
                target.window,
                AtomEnum::WM_CLASS.into(),
                AtomEnum::STRING.into(),
                1024,
            ));
            stream.write_all(requests).await.0?;
            let transient_for = parse_transient(read_words(stream).await?);
            let size_hints = parse_size_hints(read_words(stream).await?);
            let net_state = X11AtomList::from_words(read_words(stream).await?);
            let wm_hints = parse_wm_hints(read_words(stream).await?);
            let protocols = X11AtomList::from_words(read_words(stream).await?);
            let metadata =
                text::read_initial_metadata(stream, utf8_string, AtomEnum::STRING.into()).await?;
            Ok(X11PropertyResult {
                target,
                update: X11PropertyUpdate::Initial(Box::new(X11InitialProperties {
                    transient_for,
                    size_hints,
                    net_state,
                    wm_hints,
                    protocols,
                    metadata,
                })),
            })
        }
        X11PropertyRequest::TransientFor { target } => {
            query_one(
                stream,
                target,
                AtomEnum::WM_TRANSIENT_FOR.into(),
                AtomEnum::WINDOW.into(),
                1,
                |words| X11PropertyUpdate::TransientFor(parse_transient(words)),
            )
            .await
        }
        X11PropertyRequest::NormalHints { target } => {
            query_one(
                stream,
                target,
                AtomEnum::WM_NORMAL_HINTS.into(),
                AtomEnum::WM_SIZE_HINTS.into(),
                18,
                |words| X11PropertyUpdate::NormalHints(parse_size_hints(words)),
            )
            .await
        }
        X11PropertyRequest::NetState {
            target,
            net_wm_state,
        } => {
            query_one(
                stream,
                target,
                net_wm_state,
                AtomEnum::ATOM.into(),
                MAX_PROPERTY_WORDS as u32,
                |words| X11PropertyUpdate::NetState(X11AtomList::from_words(words)),
            )
            .await
        }
        X11PropertyRequest::WmHints { target } => {
            query_one(
                stream,
                target,
                AtomEnum::WM_HINTS.into(),
                AtomEnum::WM_HINTS.into(),
                9,
                |words| X11PropertyUpdate::WmHints(parse_wm_hints(words)),
            )
            .await
        }
        X11PropertyRequest::Protocols {
            target,
            wm_protocols,
        } => {
            query_one(
                stream,
                target,
                wm_protocols,
                AtomEnum::ATOM.into(),
                MAX_PROPERTY_WORDS as u32,
                |words| X11PropertyUpdate::Protocols(X11AtomList::from_words(words)),
            )
            .await
        }
        X11PropertyRequest::Title {
            target,
            net_wm_name,
            utf8_string,
        } => {
            let mut requests = [0_u8; 48];
            requests[0..24].copy_from_slice(&get_property_request(
                target.window,
                net_wm_name,
                utf8_string,
                1024,
            ));
            requests[24..48].copy_from_slice(&get_property_request(
                target.window,
                AtomEnum::WM_NAME.into(),
                AtomEnum::STRING.into(),
                1024,
            ));
            stream.write_all(requests).await.0?;
            let title = text::read_title(stream, utf8_string, AtomEnum::STRING.into()).await?;
            Ok(X11PropertyResult {
                target,
                update: X11PropertyUpdate::Title(title),
            })
        }
        X11PropertyRequest::Class { target } => {
            stream
                .write_all(get_property_request(
                    target.window,
                    AtomEnum::WM_CLASS.into(),
                    AtomEnum::STRING.into(),
                    1024,
                ))
                .await
                .0?;
            let class = text::read_class(stream, AtomEnum::STRING.into()).await?;
            Ok(X11PropertyResult {
                target,
                update: X11PropertyUpdate::Class(class),
            })
        }
    }
}

async fn query_one(
    stream: &mut UnixStream,
    target: X11PropertyTarget,
    property: u32,
    property_type: u32,
    length: u32,
    map: impl FnOnce(PropertyWords) -> X11PropertyUpdate,
) -> io::Result<X11PropertyResult> {
    stream
        .write_all(get_property_request(
            target.window,
            property,
            property_type,
            length,
        ))
        .await
        .0?;
    let words = read_words(stream).await?;
    Ok(X11PropertyResult {
        target,
        update: map(words),
    })
}

fn get_property_request(window: u32, property: u32, property_type: u32, length: u32) -> [u8; 24] {
    let mut request = [0_u8; 24];
    request[0] = X11_GET_PROPERTY;
    put_u16(&mut request[2..4], 6);
    put_u32(&mut request[4..8], window);
    put_u32(&mut request[8..12], property);
    put_u32(&mut request[12..16], property_type);
    put_u32(&mut request[20..24], length);
    request
}

#[derive(Clone, Copy, Debug, Default)]
struct PropertyWords {
    values: [u32; MAX_PROPERTY_WORDS],
    len: u8,
}

async fn read_words(stream: &mut UnixStream) -> io::Result<PropertyWords> {
    let (result, header) = stream.read_exact([0_u8; 32]).await.into_parts();
    result?;
    let value_len = property_word_count(&header)?;
    let mut words = PropertyWords::default();
    if value_len == 0 {
        return Ok(words);
    }
    let body_len = value_len * size_of::<u32>();
    let body = [0_u8; MAX_PROPERTY_BYTES].slice(..body_len);
    let (result, body) = stream.read_exact(body).await.into_parts();
    result?;
    for (index, chunk) in body[..value_len * 4].chunks_exact(4).enumerate() {
        words.values[index] = get_u32(chunk);
    }
    words.len = value_len as u8;
    Ok(words)
}

fn property_word_count(header: &[u8; 32]) -> io::Result<usize> {
    if header[0] != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("X11 property request failed with error code {}", header[1]),
        ));
    }
    let body_len = usize::try_from(get_u32(&header[4..8]))
        .unwrap_or(usize::MAX)
        .checked_mul(4)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "X11 reply length overflow"))?;
    let bytes_after = get_u32(&header[12..16]);
    let value_len = usize::try_from(get_u32(&header[16..20])).unwrap_or(usize::MAX);
    match header[1] {
        0 if body_len == 0 && value_len == 0 && bytes_after == 0 => Ok(0),
        32 => {
            let value_bytes = value_len.checked_mul(size_of::<u32>()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "X11 property length overflow")
            })?;
            if bytes_after != 0 || value_len > MAX_PROPERTY_WORDS {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "X11 property reply exceeded the fixed query capacity",
                ));
            }
            if body_len != value_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "X11 property reply length did not match its value count",
                ));
            }
            Ok(value_len)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "X11 property reply did not contain 32-bit values",
        )),
    }
}

fn parse_transient(words: PropertyWords) -> Option<u32> {
    (words.len > 0).then_some(words.values[0])
}

fn parse_size_hints(words: PropertyWords) -> X11SizeHints {
    if words.len < 9 {
        return X11SizeHints::default();
    }
    let flags = words.values[0];
    X11SizeHints {
        min_size: (flags & (1 << 4) != 0)
            .then(|| LogicalSize::new(words.values[5] as i32, words.values[6] as i32)),
        max_size: (flags & (1 << 5) != 0)
            .then(|| LogicalSize::new(words.values[7] as i32, words.values[8] as i32)),
    }
}

fn parse_wm_hints(words: PropertyWords) -> X11WmHints {
    if words.len < 2 || words.values[0] & 1 == 0 {
        return X11WmHints::default();
    }
    X11WmHints {
        accepts_input: words.values[1] != 0,
    }
}

fn put_u16(output: &mut [u8], value: u16) {
    output.copy_from_slice(&native_u16(value));
}

fn put_u32(output: &mut [u8], value: u32) {
    output.copy_from_slice(&native_u32(value));
}

fn get_u16(input: &[u8]) -> u16 {
    u16::from_ne_bytes(input.try_into().expect("two-byte X11 field"))
}

fn get_u32(input: &[u8]) -> u32 {
    u32::from_ne_bytes(input.try_into().expect("four-byte X11 field"))
}

const fn native_u16(value: u16) -> [u8; 2] {
    value.to_ne_bytes()
}

const fn native_u32(value: u32) -> [u8; 4] {
    value.to_ne_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_encoding_is_fixed_and_native_endian() {
        let request = get_property_request(7, 8, 9, 10);
        assert_eq!(request[0], X11_GET_PROPERTY);
        assert_eq!(get_u16(&request[2..4]), 6);
        assert_eq!(get_u32(&request[4..8]), 7);
        assert_eq!(get_u32(&request[8..12]), 8);
        assert_eq!(get_u32(&request[12..16]), 9);
        assert_eq!(get_u32(&request[20..24]), 10);
    }

    #[test]
    fn atom_list_updates_without_allocation_or_duplicates() {
        let mut atoms = X11AtomList::default();
        assert!(atoms.set_member(7, true));
        assert!(atoms.set_member(7, true));
        assert!(atoms.set_member(9, true));
        assert_eq!(atoms.as_slice(), &[7, 9]);
        assert!(atoms.set_member(7, false));
        assert_eq!(atoms.as_slice(), &[9]);
    }

    #[test]
    fn full_atom_list_rejects_growth_without_overwriting_values() {
        let mut atoms = X11AtomList::default();
        for atom in 0..MAX_PROPERTY_WORDS as u32 {
            assert!(atoms.set_member(atom, true));
        }
        assert!(!atoms.set_member(MAX_PROPERTY_WORDS as u32, true));
        assert_eq!(atoms.as_slice().len(), MAX_PROPERTY_WORDS);
        assert_eq!(atoms.as_slice()[0], 0);
        assert_eq!(atoms.as_slice()[MAX_PROPERTY_WORDS - 1], 31);
    }

    #[test]
    fn xid_generation_rejects_a_late_completion_for_a_reused_window() {
        let result = X11PropertyResult {
            target: X11PropertyTarget {
                window: 7,
                generation: 3,
            },
            update: X11PropertyUpdate::TransientFor(None),
        };
        assert!(result.targets(result.target));
        assert!(!result.targets(X11PropertyTarget {
            window: 7,
            generation: 4,
        }));
    }

    #[test]
    fn size_hints_use_icccm_min_and_max_slots() {
        let mut words = PropertyWords {
            len: 18,
            ..PropertyWords::default()
        };
        words.values[0] = (1 << 4) | (1 << 5);
        words.values[5..=8].copy_from_slice(&[320, 200, 1920, 1080]);
        let hints = parse_size_hints(words);
        assert_eq!(hints.min_size, Some(LogicalSize::new(320, 200)));
        assert_eq!(hints.max_size, Some(LogicalSize::new(1920, 1080)));
    }

    #[test]
    fn wm_hints_default_to_accepting_input_and_honor_the_input_flag() {
        assert!(parse_wm_hints(PropertyWords::default()).accepts_input);

        let mut words = PropertyWords {
            len: 9,
            ..PropertyWords::default()
        };
        words.values[0] = 1;
        words.values[1] = 0;
        assert!(!parse_wm_hints(words).accepts_input);
        words.values[1] = 1;
        assert!(parse_wm_hints(words).accepts_input);

        words.values[0] = 0;
        words.values[1] = 0;
        assert!(parse_wm_hints(words).accepts_input);
    }

    #[test]
    fn property_reply_rejects_truncation_and_inconsistent_lengths() {
        let mut header = [0_u8; 32];
        header[0] = 1;
        header[1] = 32;
        put_u32(&mut header[4..8], 2);
        put_u32(&mut header[16..20], 2);
        assert_eq!(property_word_count(&header).unwrap(), 2);

        put_u32(&mut header[12..16], 4);
        assert!(property_word_count(&header).is_err());
        put_u32(&mut header[12..16], 0);
        put_u32(&mut header[16..20], 1);
        assert!(property_word_count(&header).is_err());
        header[1] = 16;
        assert!(property_word_count(&header).is_err());
    }
}
