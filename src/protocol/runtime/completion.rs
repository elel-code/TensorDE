//! Compositor-thread Compio completion loop.

use std::{
    io,
    os::fd::{AsFd, OwnedFd},
    time::Duration,
};

use compio::runtime::fd::PollFd;
use tensor_runtime::{
    EventfdCompletion, EventfdWake, LocalCompletionQueue, RuntimeStop, io_uring_runtime,
};
#[cfg(feature = "tty")]
use tracing::warn;

use super::{ProtocolError, WaylandRuntime};
use crate::protocol::state::RuntimeState;

#[cfg(feature = "tty")]
const MAX_DRM_COMPLETION_SOURCES: usize = 16;
#[cfg(feature = "tty")]
const MAIN_COMPLETION_CAPACITY: usize = 2 + MAX_DRM_COMPLETION_SOURCES;
#[cfg(not(feature = "tty"))]
const MAIN_COMPLETION_CAPACITY: usize = 2;

#[derive(Clone, Copy)]
enum SourceCommand {
    Rearm,
}

enum MainCompletion {
    Worker(io::Result<u64>),
    Legacy(io::Result<()>),
    #[cfg(feature = "tty")]
    Drm {
        device_id: u64,
        result: io::Result<()>,
    },
}

struct TurnCompletions {
    worker: Option<io::Result<u64>>,
    legacy: Option<io::Result<()>>,
    #[cfg(feature = "tty")]
    drm: [Option<DrmCompletion>; MAX_DRM_COMPLETION_SOURCES],
    #[cfg(feature = "tty")]
    drm_len: usize,
}

#[cfg(feature = "tty")]
struct DrmCompletion {
    device_id: u64,
    result: io::Result<()>,
}

impl TurnCompletions {
    fn new() -> Self {
        Self {
            worker: None,
            legacy: None,
            #[cfg(feature = "tty")]
            drm: std::array::from_fn(|_| None),
            #[cfg(feature = "tty")]
            drm_len: 0,
        }
    }

    fn record(&mut self, completion: MainCompletion) -> Result<(), ProtocolError> {
        let duplicate = match completion {
            MainCompletion::Worker(result) => self.worker.replace(result).is_some(),
            MainCompletion::Legacy(result) => self.legacy.replace(result).is_some(),
            #[cfg(feature = "tty")]
            MainCompletion::Drm { device_id, result } => {
                if self.drm[..self.drm_len]
                    .iter()
                    .flatten()
                    .any(|completion| completion.device_id == device_id)
                {
                    true
                } else {
                    let Some(slot) = self.drm.get_mut(self.drm_len) else {
                        return Err(ProtocolError::MainCompletion(
                            "DRM completion batch exceeded its fixed capacity".to_owned(),
                        ));
                    };
                    *slot = Some(DrmCompletion { device_id, result });
                    self.drm_len += 1;
                    false
                }
            }
        };
        if duplicate {
            return Err(ProtocolError::MainCompletion(
                "completion source published twice without a rearm".to_owned(),
            ));
        }
        Ok(())
    }
}

impl WaylandRuntime {
    /// Run the product completion loop on the compositor thread.
    ///
    /// The legacy source is only the transitional Smithay XWM aggregate. It is
    /// awaited as one submitted Compio operation and dispatched nonblocking
    /// after its CQE; worker wakes are completed eventfd reads.
    pub fn run_with_completions<C>(
        &mut self,
        wake: &EventfdWake,
        stop: &RuntimeStop,
        mut completion_handler: C,
    ) -> Result<(), ProtocolError>
    where
        C: FnMut(&mut RuntimeState),
    {
        if !self.prepared {
            return Err(ProtocolError::RuntimeNotPrepared);
        }
        let legacy_fd = rustix::io::fcntl_dupfd_cloexec(self.event_loop.as_fd(), 0)
            .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        let runtime = io_uring_runtime(MAIN_COMPLETION_CAPACITY)
            .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        runtime.block_on(async {
            let worker_reader = wake
                .completion_reader()
                .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
            let legacy_reader = PollFd::new(legacy_fd)
                .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
            let completions = LocalCompletionQueue::bounded(MAIN_COMPLETION_CAPACITY);
            let worker_commands = LocalCompletionQueue::bounded(1);
            let legacy_commands = LocalCompletionQueue::bounded(1);
            let worker_task = compio::runtime::spawn(wait_for_worker_completions(
                worker_reader,
                completions.clone(),
                worker_commands.clone(),
            ));
            let legacy_task = compio::runtime::spawn(wait_for_legacy_completions(
                legacy_reader,
                completions.clone(),
                legacy_commands.clone(),
            ));
            #[cfg(feature = "tty")]
            let mut drm_sources = DrmCompletionSources::new();
            #[cfg(feature = "tty")]
            drm_sources.synchronize(&self.state, &completions).await?;

            self.state.on_loop_idle();
            while !stop.is_stopped() {
                let first = completions
                    .recv()
                    .await
                    .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
                let mut turn = TurnCompletions::new();
                turn.record(first)?;
                while let Some(completion) = completions
                    .try_recv()
                    .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?
                {
                    turn.record(completion)?;
                }

                if let Some(result) = turn.worker {
                    result.map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
                    completion_handler(&mut self.state);
                    if !stop.is_stopped() {
                        worker_commands
                            .try_send(SourceCommand::Rearm)
                            .map_err(|_| {
                                ProtocolError::MainCompletion(
                                    "worker completion rearm queue is full".to_owned(),
                                )
                            })?;
                    }
                }
                #[cfg(feature = "tty")]
                if !stop.is_stopped() {
                    drm_sources.synchronize(&self.state, &completions).await?;
                    for index in 0..turn.drm_len {
                        let completion = turn.drm[index]
                            .take()
                            .expect("recorded DRM completion must occupy its batch slot");
                        completion.result.map_err(|error| {
                            ProtocolError::MainCompletion(format!(
                                "DRM device {} wait failed: {error}",
                                completion.device_id
                            ))
                        })?;
                        if !drm_sources.contains(completion.device_id) {
                            continue;
                        }
                        if let Err(error) = self.state.dispatch_drm_completion(completion.device_id)
                        {
                            warn!(
                                device_id = completion.device_id,
                                %error,
                                "DRM event completion could not be consumed"
                            );
                        }
                        drm_sources.rearm(completion.device_id)?;
                    }
                }
                if let Some(result) = turn.legacy {
                    result.map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
                    self.event_loop
                        .dispatch(Some(Duration::ZERO), &mut self.state)
                        .map_err(ProtocolError::Run)?;
                    if !stop.is_stopped() {
                        legacy_commands
                            .try_send(SourceCommand::Rearm)
                            .map_err(|_| {
                                ProtocolError::MainCompletion(
                                    "legacy completion rearm queue is full".to_owned(),
                                )
                            })?;
                    }
                }
                #[cfg(feature = "tty")]
                self.state.finish_completion_turn();
                self.state.on_loop_idle();
            }

            let _ = worker_task.cancel().await;
            let _ = legacy_task.cancel().await;
            #[cfg(feature = "tty")]
            drm_sources.shutdown().await;
            Ok(())
        })
    }
}

async fn wait_for_worker_completions(
    mut reader: EventfdCompletion,
    completions: LocalCompletionQueue<MainCompletion>,
    commands: LocalCompletionQueue<SourceCommand>,
) {
    loop {
        let result = reader.completed().await;
        let failed = result.is_err();
        if completions
            .try_send(MainCompletion::Worker(result))
            .is_err()
            || failed
        {
            return;
        }
        if commands.recv().await.is_err() {
            return;
        }
    }
}

async fn wait_for_legacy_completions(
    reader: PollFd<OwnedFd>,
    completions: LocalCompletionQueue<MainCompletion>,
    commands: LocalCompletionQueue<SourceCommand>,
) {
    loop {
        let result = reader.read_ready().await;
        let failed = result.is_err();
        if completions
            .try_send(MainCompletion::Legacy(result))
            .is_err()
            || failed
        {
            return;
        }
        if commands.recv().await.is_err() {
            return;
        }
    }
}

#[cfg(feature = "tty")]
struct DrmCompletionSource {
    device_id: u64,
    commands: LocalCompletionQueue<SourceCommand>,
    task: compio::runtime::JoinHandle<()>,
}

/// Dynamic DRM waits owned by the compositor-thread Compio runtime. The map is
/// allocated once for the maximum supported cards; page-flip turns neither
/// allocate nor cancel unrelated device operations.
#[cfg(feature = "tty")]
struct DrmCompletionSources {
    generation: Option<u64>,
    sources: [Option<DrmCompletionSource>; MAX_DRM_COMPLETION_SOURCES],
}

#[cfg(feature = "tty")]
impl DrmCompletionSources {
    fn new() -> Self {
        Self {
            generation: None,
            sources: std::array::from_fn(|_| None),
        }
    }

    fn contains(&self, device_id: u64) -> bool {
        self.sources
            .iter()
            .flatten()
            .any(|source| source.device_id == device_id)
    }

    async fn synchronize(
        &mut self,
        state: &RuntimeState,
        completions: &LocalCompletionQueue<MainCompletion>,
    ) -> Result<(), ProtocolError> {
        let generation = state.drm_completion_generation();
        if self.generation == Some(generation) {
            return Ok(());
        }
        let mut device_ids = [0; MAX_DRM_COMPLETION_SOURCES];
        let device_count = state
            .write_drm_completion_device_ids(&mut device_ids)
            .map_err(ProtocolError::MainCompletion)?;
        let device_ids = &device_ids[..device_count];

        self.retain(device_ids).await;
        for &device_id in device_ids {
            if self.contains(device_id) {
                continue;
            }
            let fd = state
                .duplicate_drm_completion_fd(device_id)
                .map_err(ProtocolError::MainCompletion)?;
            self.insert(device_id, fd, completions.clone())?;
        }
        self.generation = Some(generation);
        Ok(())
    }

    fn insert(
        &mut self,
        device_id: u64,
        fd: OwnedFd,
        completions: LocalCompletionQueue<MainCompletion>,
    ) -> Result<(), ProtocolError> {
        if self.contains(device_id) {
            return Err(ProtocolError::MainCompletion(format!(
                "DRM device {device_id} was submitted twice"
            )));
        }
        let Some(slot) = self.sources.iter_mut().find(|source| source.is_none()) else {
            return Err(ProtocolError::MainCompletion(
                "DRM completion-source capacity is exhausted".to_owned(),
            ));
        };
        let reader =
            PollFd::new(fd).map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        let commands = LocalCompletionQueue::bounded(1);
        let task = compio::runtime::spawn(wait_for_drm_completions(
            device_id,
            reader,
            completions,
            commands.clone(),
        ));
        *slot = Some(DrmCompletionSource {
            device_id,
            commands,
            task,
        });
        Ok(())
    }

    fn rearm(&self, device_id: u64) -> Result<(), ProtocolError> {
        let Some(source) = self
            .sources
            .iter()
            .flatten()
            .find(|source| source.device_id == device_id)
        else {
            return Ok(());
        };
        source.commands.try_send(SourceCommand::Rearm).map_err(|_| {
            ProtocolError::MainCompletion(format!(
                "DRM device {device_id} completion rearm queue is full"
            ))
        })
    }

    async fn retain(&mut self, device_ids: &[u64]) {
        for index in 0..self.sources.len() {
            let removed = self.sources[index]
                .as_ref()
                .is_some_and(|source| !device_ids.contains(&source.device_id));
            if removed {
                let source = self.sources[index]
                    .take()
                    .expect("checked DRM completion source");
                let _ = source.task.cancel().await;
            }
        }
    }

    async fn shutdown(&mut self) {
        for source in &mut self.sources {
            if let Some(source) = source.take() {
                let _ = source.task.cancel().await;
            }
        }
    }
}

#[cfg(feature = "tty")]
async fn wait_for_drm_completions(
    device_id: u64,
    reader: PollFd<OwnedFd>,
    completions: LocalCompletionQueue<MainCompletion>,
    commands: LocalCompletionQueue<SourceCommand>,
) {
    loop {
        let result = reader.read_ready().await;
        let failed = result.is_err();
        if completions
            .try_send(MainCompletion::Drm { device_id, result })
            .is_err()
            || failed
        {
            return;
        }
        if commands.recv().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, sync::Arc};

    use tensor_runtime::WakeSink;

    use super::*;
    use crate::{
        layout::{LayoutEngine, LayoutKind},
        scene::SceneAppearance,
    };

    #[test]
    fn worker_eventfd_completion_runs_exactly_one_turn_then_stops() {
        let mut runtime = WaylandRuntime::with_appearance(
            LayoutEngine::new(LayoutKind::Scrolling1D),
            SceneAppearance::default(),
        )
        .unwrap();
        runtime.prepared = true;
        let wake = Arc::new(EventfdWake::new().unwrap());
        let writer = std::thread::spawn({
            let wake = Arc::clone(&wake);
            move || {
                wake.wake();
                wake.wake();
            }
        });
        let stop = RuntimeStop::default();
        let turns = Cell::new(0);

        runtime
            .run_with_completions(&wake, &stop, |_| {
                turns.set(turns.get() + 1);
                stop.stop();
            })
            .unwrap();
        writer.join().unwrap();

        assert_eq!(turns.get(), 1);
        assert!(stop.is_stopped());
    }

    #[cfg(feature = "tty")]
    #[test]
    fn drm_source_completion_is_one_shot_until_explicit_rearm() {
        let runtime = io_uring_runtime(MAIN_COMPLETION_CAPACITY).unwrap();
        runtime.block_on(async {
            let wake = EventfdWake::new().unwrap();
            let fd = rustix::io::fcntl_dupfd_cloexec(wake.as_fd(), 0).unwrap();
            let completions = LocalCompletionQueue::bounded(MAIN_COMPLETION_CAPACITY);
            let mut sources = DrmCompletionSources::new();
            sources.insert(7, fd, completions.clone()).unwrap();

            wake.wake();
            let MainCompletion::Drm { device_id, result } = completions.recv().await.unwrap()
            else {
                panic!("expected a DRM completion");
            };
            assert_eq!(device_id, 7);
            result.unwrap();
            assert_eq!(wake.drain().unwrap(), 1);
            assert!(completions.try_recv().unwrap().is_none());

            sources.rearm(7).unwrap();
            wake.wake();
            let MainCompletion::Drm { device_id, result } = completions.recv().await.unwrap()
            else {
                panic!("expected a rearmed DRM completion");
            };
            assert_eq!(device_id, 7);
            result.unwrap();
            assert_eq!(wake.drain().unwrap(), 1);

            sources.shutdown().await;
        });
    }

    #[cfg(feature = "tty")]
    #[test]
    fn removing_one_drm_source_preserves_other_submitted_waits() {
        let runtime = io_uring_runtime(MAIN_COMPLETION_CAPACITY).unwrap();
        runtime.block_on(async {
            let removed = EventfdWake::new().unwrap();
            let retained = EventfdWake::new().unwrap();
            let removed_fd = rustix::io::fcntl_dupfd_cloexec(removed.as_fd(), 0).unwrap();
            let retained_fd = rustix::io::fcntl_dupfd_cloexec(retained.as_fd(), 0).unwrap();
            let completions = LocalCompletionQueue::bounded(MAIN_COMPLETION_CAPACITY);
            let mut sources = DrmCompletionSources::new();
            sources.insert(1, removed_fd, completions.clone()).unwrap();
            sources.insert(2, retained_fd, completions.clone()).unwrap();

            sources.retain(&[2]).await;
            removed.wake();
            retained.wake();
            let MainCompletion::Drm { device_id, result } = completions.recv().await.unwrap()
            else {
                panic!("expected the retained DRM completion");
            };
            assert_eq!(device_id, 2);
            result.unwrap();
            assert_eq!(removed.drain().unwrap(), 1);
            assert_eq!(retained.drain().unwrap(), 1);
            assert!(completions.try_recv().unwrap().is_none());

            sources.shutdown().await;
        });
    }
}
