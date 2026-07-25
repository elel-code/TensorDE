//! Bounded asynchronous compositor diagnostics.
//!
//! Smithay callbacks and Vulkan frame work may emit tracing events, but neither
//! may wait for a terminal, journal, or filesystem write. This boundary turns
//! each formatted record into a bounded value-only message and gives a single
//! Compio-owned drain thread sole ownership of the output handle. Compio uses
//! io_uring when the host supports it and falls back to polling otherwise.
//!
//! `sequence::run` blocks termination signals before this worker exists, so
//! the worker inherits the compositor's signal mask and never competes with
//! calloop's signalfd source.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use compio::{
    fs::{self as compio_fs, OpenOptions},
    io::{AsyncWriteAtExt, AsyncWriteExt},
    runtime::Runtime,
};
use thiserror::Error;
use tracing_subscriber::{EnvFilter, fmt::MakeWriter};

const LOG_FILE_ENV: &str = "TENSOR_LOG_FILE";
const LOG_BUFFERED_RECORDS: usize = 8 * 1024;
const MAX_LOG_RECORD_BYTES: usize = 8 * 1024;
const LOG_IDLE_POLL: Duration = Duration::from_millis(25);
const TRUNCATED_RECORD_SUFFIX: &[u8] = b" [tensor: log record truncated]\n";

pub(crate) fn initialize() -> Result<Logging, LoggingError> {
    let filter = EnvFilter::builder()
        .with_default_directive("tensor_compositor=info".parse().unwrap())
        .from_env_lossy();
    let file_path = env::var_os(LOG_FILE_ENV).map(PathBuf::from);
    let target = match &file_path {
        Some(path) => {
            create_log_directory(path)?;
            LogTarget::File(path.clone())
        }
        None => LogTarget::Stderr,
    };
    let asynchronous = AsyncLogGuard::start(target)?;
    let writer = asynchronous.writer();

    let subscriber = if file_path.is_some() {
        tracing_subscriber::fmt()
            .compact()
            .with_ansi(false)
            .with_writer(writer)
            .with_env_filter(filter)
            .try_init()
    } else {
        tracing_subscriber::fmt()
            .compact()
            .with_writer(writer)
            .with_env_filter(filter)
            .try_init()
    };
    subscriber.map_err(|error| LoggingError::Subscriber(error.to_string()))?;

    Ok(Logging {
        asynchronous,
        file_path,
    })
}

fn create_log_directory(path: &Path) -> Result<(), LoggingError> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|source| LoggingError::LogDirectory {
        path: parent.to_owned(),
        source,
    })
}

pub(crate) struct Logging {
    asynchronous: AsyncLogGuard,
    file_path: Option<PathBuf>,
}

impl Logging {
    pub(crate) fn file_path(&self) -> Option<&Path> {
        self.file_path.as_deref()
    }

    /// Stop accepting records and join the drain thread after it flushes.
    ///
    /// Call this after the compositor event loop returns so SIGTERM / shutdown
    /// lines enqueued on the way out are not lost when the process exits.
    pub(crate) fn shutdown(self) {
        self.asynchronous.shutdown();
    }
}

struct AsyncLogGuard {
    sender: SyncSender<Vec<u8>>,
    state: Arc<LogState>,
    worker: Option<JoinHandle<()>>,
}

impl AsyncLogGuard {
    fn start(target: LogTarget) -> Result<Self, LoggingError> {
        let (sender, receiver) = mpsc::sync_channel(LOG_BUFFERED_RECORDS);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let state = Arc::new(LogState::active());
        let worker_state = state.clone();
        let worker = thread::Builder::new()
            .name("tensor-log-drain".to_owned())
            .spawn(move || log_worker(target, receiver, worker_state, ready_sender))
            .map_err(LoggingError::WorkerSpawn)?;

        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                sender,
                state,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                let _ = worker.join();
                Err(LoggingError::WorkerStopped)
            }
        }
    }

    fn writer(&self) -> AsyncLogMakeWriter {
        AsyncLogMakeWriter {
            sender: self.sender.clone(),
            state: self.state.clone(),
        }
    }

    fn shutdown(mut self) {
        self.shutdown_worker();
    }

    fn shutdown_worker(&mut self) {
        // The global tracing subscriber retains its MakeWriter, so closing the
        // sender cannot be the shutdown signal. New records become no-ops and
        // the drain worker flushes records already accepted into its queue.
        self.state.active.store(false, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for AsyncLogGuard {
    fn drop(&mut self) {
        self.shutdown_worker();
    }
}

#[derive(Default)]
struct LogState {
    active: AtomicBool,
    dropped: AtomicU64,
}

impl LogState {
    fn active() -> Self {
        Self {
            active: AtomicBool::new(true),
            dropped: AtomicU64::new(0),
        }
    }
}

#[derive(Clone)]
struct AsyncLogMakeWriter {
    sender: SyncSender<Vec<u8>>,
    state: Arc<LogState>,
}

impl<'writer> MakeWriter<'writer> for AsyncLogMakeWriter {
    type Writer = AsyncLogWriter;

    fn make_writer(&'writer self) -> Self::Writer {
        AsyncLogWriter {
            sender: self.sender.clone(),
            state: self.state.clone(),
            record: Vec::with_capacity(256),
            truncated: false,
        }
    }
}

struct AsyncLogWriter {
    sender: SyncSender<Vec<u8>>,
    state: Arc<LogState>,
    record: Vec<u8>,
    truncated: bool,
}

impl AsyncLogWriter {
    fn submit(&mut self) {
        if self.record.is_empty() || !self.state.active.load(Ordering::Acquire) {
            self.record.clear();
            self.truncated = false;
            return;
        }
        if self.truncated {
            let prefix_len = MAX_LOG_RECORD_BYTES - TRUNCATED_RECORD_SUFFIX.len();
            self.record.truncate(prefix_len);
            if self.record.last() == Some(&b'\n') {
                self.record.pop();
            }
            self.record.extend_from_slice(TRUNCATED_RECORD_SUFFIX);
            self.truncated = false;
        }
        let record = std::mem::take(&mut self.record);
        match self.sender.try_send(record) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
            Err(TrySendError::Full(_)) => {
                self.state.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl io::Write for AsyncLogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.state.active.load(Ordering::Acquire) {
            return Ok(bytes.len());
        }
        let available = MAX_LOG_RECORD_BYTES.saturating_sub(self.record.len());
        let accepted = bytes.len().min(available);
        self.record.extend_from_slice(&bytes[..accepted]);
        self.truncated |= accepted < bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.submit();
        Ok(())
    }
}

impl Drop for AsyncLogWriter {
    fn drop(&mut self) {
        self.submit();
    }
}

enum LogTarget {
    Stderr,
    File(PathBuf),
}

enum LogSink {
    Stderr(compio_fs::Stderr),
    File {
        file: compio_fs::File,
        next_offset: u64,
    },
}

impl LogSink {
    async fn open(target: LogTarget) -> Result<Self, LoggingError> {
        match target {
            LogTarget::Stderr => Ok(Self::Stderr(compio_fs::stderr())),
            LogTarget::File(path) => {
                let mut options = OpenOptions::new();
                options.create(true);
                options.write(true);
                let file = options
                    .open(&path)
                    .await
                    .map_err(|source| LoggingError::LogFile {
                        path: path.clone(),
                        source,
                    })?;
                let next_offset = file
                    .metadata()
                    .await
                    .map_err(|source| LoggingError::LogFile { path, source })?
                    .len();
                Ok(Self::File { file, next_offset })
            }
        }
    }

    async fn write_record(&mut self, record: Vec<u8>) -> io::Result<()> {
        match self {
            Self::Stderr(stderr) => {
                let (result, _) = stderr.write_all(record).await.into_parts();
                result
            }
            Self::File { file, next_offset } => {
                let length = record.len() as u64;
                let (result, _) = file.write_all_at(record, *next_offset).await.into_parts();
                result?;
                *next_offset = next_offset.saturating_add(length);
                Ok(())
            }
        }
    }
}

fn log_worker(
    target: LogTarget,
    receiver: Receiver<Vec<u8>>,
    state: Arc<LogState>,
    ready: SyncSender<Result<(), LoggingError>>,
) {
    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(source) => {
            let _ = ready.send(Err(LoggingError::Runtime(source)));
            return;
        }
    };
    let sink = match runtime.block_on(LogSink::open(target)) {
        Ok(sink) => sink,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }

    drain_records(&runtime, sink, receiver, state);
}

fn drain_records(
    runtime: &Runtime,
    mut sink: LogSink,
    receiver: Receiver<Vec<u8>>,
    state: Arc<LogState>,
) {
    let mut reported_sink_failure = false;
    loop {
        match receiver.recv_timeout(LOG_IDLE_POLL) {
            Ok(record) => {
                write_dropped_notice(runtime, &mut sink, &state, &mut reported_sink_failure);
                write_record(runtime, &mut sink, record, &mut reported_sink_failure);
            }
            Err(mpsc::RecvTimeoutError::Timeout) if state.active.load(Ordering::Acquire) => {}
            Err(mpsc::RecvTimeoutError::Timeout) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                drain_accepted_records(
                    runtime,
                    &mut sink,
                    &receiver,
                    &state,
                    &mut reported_sink_failure,
                );
                write_dropped_notice(runtime, &mut sink, &state, &mut reported_sink_failure);
                return;
            }
        }

        if !state.active.load(Ordering::Acquire) {
            drain_accepted_records(
                runtime,
                &mut sink,
                &receiver,
                &state,
                &mut reported_sink_failure,
            );
            write_dropped_notice(runtime, &mut sink, &state, &mut reported_sink_failure);
            return;
        }
    }
}

fn drain_accepted_records(
    runtime: &Runtime,
    sink: &mut LogSink,
    receiver: &Receiver<Vec<u8>>,
    state: &LogState,
    reported_sink_failure: &mut bool,
) {
    while let Ok(record) = receiver.try_recv() {
        write_dropped_notice(runtime, sink, state, reported_sink_failure);
        write_record(runtime, sink, record, reported_sink_failure);
    }
}

fn write_dropped_notice(
    runtime: &Runtime,
    sink: &mut LogSink,
    state: &LogState,
    reported_sink_failure: &mut bool,
) {
    let dropped = state.dropped.swap(0, Ordering::Relaxed);
    if dropped != 0 {
        let record =
            format!("tensor diagnostics: dropped {dropped} log records after queue saturation\n")
                .into_bytes();
        write_record(runtime, sink, record, reported_sink_failure);
    }
}

fn write_record(
    runtime: &Runtime,
    sink: &mut LogSink,
    record: Vec<u8>,
    reported_sink_failure: &mut bool,
) {
    match runtime.block_on(sink.write_record(record)) {
        Ok(()) => *reported_sink_failure = false,
        Err(error) if !*reported_sink_failure => {
            // This is the dedicated drain thread, never a compositor callback.
            // Preserve one visible diagnostic when the configured log target has
            // failed, but avoid a synchronous error storm on every record.
            eprintln!("tensor: asynchronous log drain failed: {error}");
            *reported_sink_failure = true;
        }
        Err(_) => {}
    }
}

#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("could not create compositor log directory {path}: {source}")]
    LogDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not open compositor log file {path}: {source}")]
    LogFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not start asynchronous log worker: {0}")]
    WorkerSpawn(#[source] io::Error),
    #[error("asynchronous log worker stopped before initialization")]
    WorkerStopped,
    #[error("could not start Compio log runtime: {0}")]
    Runtime(#[source] io::Error),
    #[error("failed to initialize tracing subscriber: {0}")]
    Subscriber(String),
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Write as _,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    static LOG_TEST_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn asynchronous_file_worker_appends_and_drains_on_shutdown() {
        let root = test_root();
        let path = root.join("nested").join("tensor.log");
        create_log_directory(&path).unwrap();

        write_with_worker(&path, b"first\n");
        write_with_worker(&path, b"second\n");

        assert_eq!(fs::read_to_string(&path).unwrap(), "first\nsecond\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writer_drops_oversized_records_without_blocking_the_caller() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let state = Arc::new(LogState::active());
        let mut writer = AsyncLogWriter {
            sender,
            state,
            record: Vec::new(),
            truncated: false,
        };
        writer
            .write_all(&vec![b'x'; MAX_LOG_RECORD_BYTES + 1])
            .unwrap();
        drop(writer);

        let record = receiver.recv().unwrap();
        assert!(record.len() <= MAX_LOG_RECORD_BYTES);
        assert!(record.ends_with(TRUNCATED_RECORD_SUFFIX));
    }

    #[test]
    fn queue_saturation_is_lossy_and_counted() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        sender.try_send(b"first\n".to_vec()).unwrap();
        let state = Arc::new(LogState::active());
        let mut writer = AsyncLogWriter {
            sender,
            state: state.clone(),
            record: Vec::new(),
            truncated: false,
        };
        writer.write_all(b"second\n").unwrap();
        drop(writer);

        assert_eq!(state.dropped.load(Ordering::Relaxed), 1);
    }

    fn write_with_worker(path: &Path, line: &[u8]) {
        let guard = AsyncLogGuard::start(LogTarget::File(path.to_owned())).unwrap();
        let mut writer = guard.writer().make_writer();
        writer.write_all(line).unwrap();
        drop(writer);
        drop(guard);
    }

    fn test_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "tensor-log-test-{}-{}",
            std::process::id(),
            LOG_TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
