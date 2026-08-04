//! Compositor-thread Compio completion loop.

use std::{io, os::fd::OwnedFd};

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
const MAIN_COMPLETION_CAPACITY: usize = 4 + MAX_DRM_COMPLETION_SOURCES;
#[cfg(not(feature = "tty"))]
const MAIN_COMPLETION_CAPACITY: usize = 3;

#[derive(Clone, Copy)]
enum SourceCommand {
    Rearm,
}

enum MainCompletion {
    Worker(io::Result<u64>),
    CommitTimer(io::Result<()>),
    IdleTimer(io::Result<()>),
    #[cfg(feature = "tty")]
    CursorTimer(io::Result<()>),
    #[cfg(feature = "tty")]
    Drm {
        device_id: u64,
        result: io::Result<()>,
    },
}

struct TurnCompletions {
    worker: Option<io::Result<u64>>,
    commit_timer: Option<io::Result<()>>,
    idle_timer: Option<io::Result<()>>,
    #[cfg(feature = "tty")]
    cursor_timer: Option<io::Result<()>>,
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
            commit_timer: None,
            idle_timer: None,
            #[cfg(feature = "tty")]
            cursor_timer: None,
            #[cfg(feature = "tty")]
            drm: std::array::from_fn(|_| None),
            #[cfg(feature = "tty")]
            drm_len: 0,
        }
    }

    fn record(&mut self, completion: MainCompletion) -> Result<(), ProtocolError> {
        let duplicate = match completion {
            MainCompletion::Worker(result) => self.worker.replace(result).is_some(),
            MainCompletion::CommitTimer(result) => self.commit_timer.replace(result).is_some(),
            MainCompletion::IdleTimer(result) => self.idle_timer.replace(result).is_some(),
            #[cfg(feature = "tty")]
            MainCompletion::CursorTimer(result) => self.cursor_timer.replace(result).is_some(),
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
    /// Worker wakes, timer deadlines, and DRM events enter this loop only
    /// after their submitted io_uring operations complete.
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
        let commit_timer_fd = self
            .state
            .duplicate_commit_timer_fd()
            .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        let idle_timer_fd = self
            .state
            .duplicate_idle_timer_fd()
            .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        #[cfg(feature = "tty")]
        let cursor_timer_fd = self
            .state
            .duplicate_cursor_animation_timer_fd()
            .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        let runtime = io_uring_runtime(MAIN_COMPLETION_CAPACITY)
            .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        runtime.block_on(async {
            let worker_reader = wake
                .completion_reader()
                .map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
            let completions = LocalCompletionQueue::bounded(MAIN_COMPLETION_CAPACITY);
            let worker_commands = LocalCompletionQueue::bounded(1);
            let worker_task = compio::runtime::spawn(wait_for_worker_completions(
                worker_reader,
                completions.clone(),
                worker_commands.clone(),
            ));
            let commit_timer_source = commit_timer_fd
                .map(|fd| {
                    TimerCompletionSource::new(
                        fd,
                        completions.clone(),
                        MainCompletion::CommitTimer,
                        "commit-timing",
                    )
                })
                .transpose()?;
            let idle_timer_source = idle_timer_fd
                .map(|fd| {
                    TimerCompletionSource::new(
                        fd,
                        completions.clone(),
                        MainCompletion::IdleTimer,
                        "idle-notify",
                    )
                })
                .transpose()?;
            #[cfg(feature = "tty")]
            let cursor_timer_source = cursor_timer_fd
                .map(|fd| {
                    TimerCompletionSource::new(
                        fd,
                        completions.clone(),
                        MainCompletion::CursorTimer,
                        "cursor-animation",
                    )
                })
                .transpose()?;
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

                if let Some(result) = turn.commit_timer {
                    let rearm = match result {
                        Ok(()) => self.state.complete_commit_timer(),
                        Err(error) => {
                            self.state.commit_timer_completion_failed(&error);
                            false
                        }
                    };
                    if rearm
                        && !stop.is_stopped()
                        && let Some(source) = &commit_timer_source
                    {
                        source.rearm()?;
                    }
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
                if let Some(result) = turn.idle_timer {
                    let rearm = match result {
                        Ok(()) => self.state.complete_idle_timer(),
                        Err(error) => {
                            self.state.idle_timer_completion_failed(&error);
                            false
                        }
                    };
                    if rearm
                        && !stop.is_stopped()
                        && let Some(source) = &idle_timer_source
                    {
                        source.rearm()?;
                    }
                }
                #[cfg(feature = "tty")]
                if let Some(result) = turn.cursor_timer {
                    let rearm = match result {
                        Ok(()) => self.state.complete_cursor_animation_timer(),
                        Err(error) => {
                            self.state.cursor_animation_timer_failed(&error);
                            false
                        }
                    };
                    if rearm
                        && !stop.is_stopped()
                        && let Some(source) = &cursor_timer_source
                    {
                        source.rearm()?;
                    }
                }
                #[cfg(feature = "tty")]
                self.state.finish_completion_turn();
                self.state.on_loop_idle();
            }

            let _ = worker_task.cancel().await;
            if let Some(source) = commit_timer_source {
                source.shutdown().await;
            }
            if let Some(source) = idle_timer_source {
                source.shutdown().await;
            }
            #[cfg(feature = "tty")]
            if let Some(source) = cursor_timer_source {
                source.shutdown().await;
            }
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

struct TimerCompletionSource {
    commands: LocalCompletionQueue<SourceCommand>,
    task: compio::runtime::JoinHandle<()>,
    name: &'static str,
}

impl TimerCompletionSource {
    fn new(
        fd: OwnedFd,
        completions: LocalCompletionQueue<MainCompletion>,
        publish: fn(io::Result<()>) -> MainCompletion,
        name: &'static str,
    ) -> Result<Self, ProtocolError> {
        let reader =
            PollFd::new(fd).map_err(|error| ProtocolError::MainCompletion(error.to_string()))?;
        let commands = LocalCompletionQueue::bounded(1);
        let task = compio::runtime::spawn(wait_for_timer_completions(
            reader,
            completions,
            commands.clone(),
            publish,
        ));
        Ok(Self {
            commands,
            task,
            name,
        })
    }

    fn rearm(&self) -> Result<(), ProtocolError> {
        self.commands.try_send(SourceCommand::Rearm).map_err(|_| {
            ProtocolError::MainCompletion(format!("{} completion rearm queue is full", self.name))
        })
    }

    async fn shutdown(self) {
        let _ = self.task.cancel().await;
    }
}

async fn wait_for_timer_completions(
    reader: PollFd<OwnedFd>,
    completions: LocalCompletionQueue<MainCompletion>,
    commands: LocalCompletionQueue<SourceCommand>,
    publish: fn(io::Result<()>) -> MainCompletion,
) {
    loop {
        // PollFd submits one IORING_OP_POLL_ADD and resolves only from its CQE.
        let result = reader.read_ready().await;
        let failed = result.is_err();
        if completions.try_send(publish(result)).is_err() || failed {
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

    #[test]
    fn commit_timer_completion_is_one_shot_until_explicit_rearm() {
        assert_timer_completion_is_one_shot(
            MainCompletion::CommitTimer,
            |completion| match completion {
                MainCompletion::CommitTimer(result) => result,
                _ => panic!("expected a commit-timer completion"),
            },
            "commit-timing",
        );
    }

    #[test]
    fn idle_timer_completion_is_one_shot_until_explicit_rearm() {
        assert_timer_completion_is_one_shot(
            MainCompletion::IdleTimer,
            |completion| match completion {
                MainCompletion::IdleTimer(result) => result,
                _ => panic!("expected an idle-timer completion"),
            },
            "idle-notify",
        );
    }

    #[cfg(feature = "tty")]
    #[test]
    fn cursor_timer_completion_is_one_shot_until_explicit_rearm() {
        assert_timer_completion_is_one_shot(
            MainCompletion::CursorTimer,
            |completion| match completion {
                MainCompletion::CursorTimer(result) => result,
                _ => panic!("expected a cursor-timer completion"),
            },
            "cursor-animation",
        );
    }

    fn assert_timer_completion_is_one_shot(
        publish: fn(io::Result<()>) -> MainCompletion,
        take_result: fn(MainCompletion) -> io::Result<()>,
        name: &'static str,
    ) {
        let runtime = io_uring_runtime(MAIN_COMPLETION_CAPACITY).unwrap();
        runtime.block_on(async {
            let timer = rustix::time::timerfd_create(
                rustix::time::TimerfdClockId::Monotonic,
                rustix::time::TimerfdFlags::CLOEXEC | rustix::time::TimerfdFlags::NONBLOCK,
            )
            .unwrap();
            let fd = rustix::io::fcntl_dupfd_cloexec(&timer, 0).unwrap();
            let completions = LocalCompletionQueue::bounded(MAIN_COMPLETION_CAPACITY);
            let source =
                TimerCompletionSource::new(fd, completions.clone(), publish, name).unwrap();
            let arm = || {
                rustix::time::timerfd_settime(
                    &timer,
                    rustix::time::TimerfdTimerFlags::empty(),
                    &rustix::time::Itimerspec {
                        it_interval: rustix::time::Timespec::default(),
                        it_value: rustix::time::Timespec {
                            tv_sec: 0,
                            tv_nsec: 1,
                        },
                    },
                )
                .unwrap();
            };

            arm();
            take_result(completions.recv().await.unwrap()).unwrap();
            let mut expirations = [0_u8; 8];
            assert_eq!(
                rustix::io::read(&timer, &mut expirations[..]).unwrap(),
                expirations.len()
            );
            assert!(completions.try_recv().unwrap().is_none());

            source.rearm().unwrap();
            arm();
            take_result(completions.recv().await.unwrap()).unwrap();
            source.shutdown().await;
        });
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
