//! Compio-backed delivery of compositor media keys to Tensor Shell.
//!
//! Tensorland owns the physical keyboard and the KDL binding, but it does not
//! own MPRIS policy.  A small worker keeps the session-bus connection affine to
//! one Compio runtime thread and accepts only bounded, value-only actions from
//! the compositor turn.

use std::{
    future::Future,
    io,
    sync::mpsc,
    thread::{self, JoinHandle},
};

use async_channel::{Receiver, Sender, TrySendError};
use futures_util::{FutureExt, pin_mut, select_biased};
use tensor_dbus::{Connection, freedesktop::mpris::MprisAction, tensor::shell};
use tensor_runtime::io_uring_runtime;
use thiserror::Error;
use tracing::warn;

/// Maximum number of media actions waiting behind one in-flight D-Bus call.
pub(crate) const MAX_PENDING_MEDIA_ACTIONS: usize = 16;

/// Policy action selected by a Tensorland media-key binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaAction {
    Previous,
    PlayPause,
    Next,
}

impl MediaAction {
    const fn name(self) -> &'static str {
        match self {
            Self::Previous => "previous",
            Self::PlayPause => "play-pause",
            Self::Next => "next",
        }
    }
}

impl From<MediaAction> for MprisAction {
    fn from(action: MediaAction) -> Self {
        match action {
            MediaAction::Previous => Self::Previous,
            MediaAction::PlayPause => Self::PlayPause,
            MediaAction::Next => Self::Next,
        }
    }
}

/// Cloneable, non-blocking handle used by compositor-thread input dispatch.
#[derive(Clone, Debug)]
pub(crate) struct MediaActionSubmitter {
    requests: Sender<MediaAction>,
}

impl MediaActionSubmitter {
    pub(crate) fn submit(&self, action: MediaAction) -> Result<(), MediaSubmitError> {
        match self.requests.try_send(action) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(MediaSubmitError::QueueFull { action }),
            Err(TrySendError::Closed(_)) => Err(MediaSubmitError::WorkerStopped { action }),
        }
    }
}

/// Persistent session-bus worker.  It is deliberately separate from the
/// compositor event loop so a slow or unavailable Shell never blocks input.
pub(crate) struct MediaActionWorker {
    submitter: MediaActionSubmitter,
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl MediaActionWorker {
    pub(crate) fn new() -> Result<Self, MediaWorkerError> {
        let (requests, receiver) = async_channel::bounded(MAX_PENDING_MEDIA_ACTIONS);
        let (stop, stop_receiver) = async_channel::bounded(1);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("tensor-media-dbus".to_owned())
            .spawn(move || run(receiver, stop_receiver, ready_tx))
            .map_err(MediaWorkerError::Spawn)?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                submitter: MediaActionSubmitter { requests },
                stop,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(MediaWorkerError::StartupDisconnected)
            }
        }
    }

    pub(crate) fn submitter(&self) -> MediaActionSubmitter {
        self.submitter.clone()
    }
}

impl Drop for MediaActionWorker {
    fn drop(&mut self) {
        self.stop.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    receiver: Receiver<MediaAction>,
    stop: Receiver<()>,
    ready: mpsc::SyncSender<Result<(), MediaWorkerError>>,
) {
    let runtime = match io_uring_runtime(4) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(MediaWorkerError::Runtime(error)));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }

    runtime.block_on(run_service(receiver, stop));
}

async fn run_service(receiver: Receiver<MediaAction>, stop: Receiver<()>) {
    let mut connection = None;
    loop {
        let Some(action) = stop_or(&stop, receiver.recv()).await else {
            return;
        };
        let Ok(action) = action else {
            return;
        };
        let Some((next_connection, result)) = stop_or(&stop, dispatch(connection, action)).await
        else {
            return;
        };
        connection = next_connection;
        if let Err(error) = result {
            warn!(action = action.name(), %error, "Tensor Shell media action failed");
        }
    }
}

async fn stop_or<T>(stop: &Receiver<()>, future: impl Future<Output = T>) -> Option<T> {
    let stop = stop.recv().fuse();
    let future = future.fuse();
    pin_mut!(stop, future);
    select_biased! {
        _ = stop => None,
        output = future => Some(output),
    }
}

async fn dispatch(
    mut connection: Option<Connection>,
    action: MediaAction,
) -> (Option<Connection>, Result<(), tensor_dbus::Error>) {
    if connection.is_none() {
        match Connection::session_bus().await {
            Ok(created) => connection = Some(created),
            Err(error) => return (None, Err(error)),
        }
    }
    let result = shell::perform_media_action(
        connection
            .as_mut()
            .expect("media connection was created before dispatch"),
        action.into(),
    )
    .await;
    if !connection
        .as_ref()
        .expect("media connection was created before dispatch")
        .is_usable()
    {
        connection = None;
    }
    (connection, result)
}

#[derive(Debug, Error, Eq, PartialEq)]
pub(crate) enum MediaSubmitError {
    #[error("media action queue is full for {action:?}")]
    QueueFull { action: MediaAction },
    #[error("media action worker stopped before accepting {action:?}")]
    WorkerStopped { action: MediaAction },
}

#[derive(Debug, Error)]
pub enum MediaWorkerError {
    #[error("failed to create the media D-Bus worker runtime: {0}")]
    Runtime(#[source] io::Error),
    #[error("failed to spawn the media D-Bus worker: {0}")]
    Spawn(#[source] io::Error),
    #[error("media D-Bus worker stopped during initialization")]
    StartupDisconnected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_actions_lower_to_the_versioned_shell_endpoint() {
        assert_eq!(
            MprisAction::from(MediaAction::Previous),
            MprisAction::Previous
        );
        assert_eq!(
            MprisAction::from(MediaAction::PlayPause),
            MprisAction::PlayPause
        );
        assert_eq!(MprisAction::from(MediaAction::Next), MprisAction::Next);
    }

    #[test]
    fn action_queue_is_bounded_without_waiting() {
        let (sender, receiver) = async_channel::bounded(1);
        let submitter = MediaActionSubmitter { requests: sender };
        submitter.submit(MediaAction::Next).unwrap();
        assert_eq!(
            submitter.submit(MediaAction::Previous),
            Err(MediaSubmitError::QueueFull {
                action: MediaAction::Previous
            })
        );
        assert_eq!(receiver.try_recv(), Ok(MediaAction::Next));
    }

    #[test]
    fn closed_action_queue_reports_stopped_worker() {
        let (sender, receiver) = async_channel::bounded(1);
        receiver.close();
        let submitter = MediaActionSubmitter { requests: sender };

        assert_eq!(
            submitter.submit(MediaAction::PlayPause),
            Err(MediaSubmitError::WorkerStopped {
                action: MediaAction::PlayPause
            })
        );
    }
}
