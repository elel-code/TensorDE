use std::{os::unix::net::UnixStream, path::PathBuf, sync::mpsc, time::Duration};

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::wp::{
    commit_timing::v1::client::{wp_commit_timer_v1, wp_commit_timing_manager_v1},
    fifo::v1::client::{wp_fifo_manager_v1, wp_fifo_v1},
};

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TimingError {
    DuplicateFifo,
    InvalidTimestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FifoStep {
    BarrierApplied,
    WaitCommitQueued,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TimerStep {
    CommitQueued,
}

#[derive(Default)]
struct TimingClient;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for TimingClient {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(TimingClient: ignore wl_compositor::WlCompositor);
delegate_noop!(TimingClient: ignore wl_surface::WlSurface);
delegate_noop!(TimingClient: ignore wp_fifo_manager_v1::WpFifoManagerV1);
delegate_noop!(TimingClient: ignore wp_fifo_v1::WpFifoV1);
delegate_noop!(TimingClient: ignore wp_commit_timing_manager_v1::WpCommitTimingManagerV1);
delegate_noop!(TimingClient: ignore wp_commit_timer_v1::WpCommitTimerV1);

#[test]
fn surface_timing_wire_reports_uniqueness_and_timestamp_errors() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();

    let (result_tx, result_rx) = mpsc::channel();
    let duplicate_client = spawn_duplicate_fifo_client(socket_path.clone(), result_tx.clone());
    assert_eq!(
        dispatch_until_timing_error(&mut runtime, &result_rx),
        TimingError::DuplicateFifo
    );
    duplicate_client.join().unwrap();

    let invalid_client = spawn_invalid_timestamp_client(socket_path, result_tx);
    assert_eq!(
        dispatch_until_timing_error(&mut runtime, &result_rx),
        TimingError::InvalidTimestamp
    );
    invalid_client.join().unwrap();
}

#[test]
fn offscreen_fifo_wait_is_released_at_the_idle_boundary() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TimingClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let manager = globals
            .bind::<wp_fifo_manager_v1::WpFifoManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let fifo = manager.get_fifo(&surface, &handle, ());

        fifo.set_barrier();
        surface.commit();
        queue.roundtrip(&mut TimingClient).unwrap();
        step_tx.send(FifoStep::BarrierApplied).unwrap();
        release_rx.recv().unwrap();

        fifo.wait_barrier();
        surface.commit();
        queue.roundtrip(&mut TimingClient).unwrap();
        step_tx.send(FifoStep::WaitCommitQueued).unwrap();
        release_rx.recv().unwrap();

        fifo.destroy();
        surface.destroy();
        manager.destroy();
    });

    assert_eq!(
        dispatch_until_fifo_step(&mut runtime, &step_rx),
        FifoStep::BarrierApplied
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_timing
            .active_fifo_barrier_count(),
        1
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_timing
            .applied_fifo_commit_count(),
        1
    );
    release_tx.send(()).unwrap();

    assert_eq!(
        dispatch_until_fifo_step(&mut runtime, &step_rx),
        FifoStep::WaitCommitQueued
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_timing
            .applied_fifo_commit_count(),
        1
    );
    runtime.state.on_loop_idle();
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_timing
            .active_fifo_barrier_count(),
        0
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_timing
            .applied_fifo_commit_count(),
        2
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

#[test]
fn future_commit_is_applied_only_after_timerfd_completion() {
    let mut runtime = WaylandRuntime::with_appearance(
        LayoutEngine::new(crate::layout::LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap();
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is required");
    let socket_path = PathBuf::from(runtime_dir).join(runtime.socket_name());
    let _socket_completions = runtime.prepare_for_test(false).unwrap();
    let (step_tx, step_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::sync_channel(0);

    let client = std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TimingClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let manager = globals
            .bind::<wp_commit_timing_manager_v1::WpCommitTimingManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let timer = manager.get_timer(&surface, &handle, ());
        let (seconds, nanoseconds) = monotonic_after(Duration::from_millis(500));
        timer.set_timestamp(
            u32::try_from(seconds >> 32).unwrap(),
            seconds as u32,
            nanoseconds,
        );
        surface.commit();
        queue.roundtrip(&mut TimingClient).unwrap();
        step_tx.send(TimerStep::CommitQueued).unwrap();
        release_rx.recv().unwrap();

        timer.destroy();
        surface.destroy();
        manager.destroy();
    });

    assert_eq!(
        dispatch_until_timer_step(&mut runtime, &step_rx),
        TimerStep::CommitQueued
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_timing
            .scheduled_commit_timer_count(),
        1
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_timing
            .applied_timed_commit_count(),
        0
    );

    std::thread::sleep(Duration::from_millis(550));
    assert!(runtime.state.complete_commit_timer());
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_timing
            .scheduled_commit_timer_count(),
        0
    );
    assert_eq!(
        runtime
            .state
            .protocol_globals
            .surface_timing
            .applied_timed_commit_count(),
        1
    );
    release_tx.send(()).unwrap();
    client.join().unwrap();
}

fn spawn_duplicate_fifo_client(
    socket_path: PathBuf,
    result: mpsc::Sender<TimingError>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TimingClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let manager = globals
            .bind::<wp_fifo_manager_v1::WpFifoManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let _first = manager.get_fifo(&surface, &handle, ());
        let _second = manager.get_fifo(&surface, &handle, ());
        assert!(queue.roundtrip(&mut TimingClient).is_err());
        result.send(TimingError::DuplicateFifo).unwrap();
    })
}

fn spawn_invalid_timestamp_client(
    socket_path: PathBuf,
    result: mpsc::Sender<TimingError>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let connection =
            Connection::from_socket(UnixStream::connect(socket_path).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<TimingClient>(&connection).unwrap();
        let handle = queue.handle();
        let compositor = globals
            .bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())
            .unwrap();
        let manager = globals
            .bind::<wp_commit_timing_manager_v1::WpCommitTimingManagerV1, _, _>(&handle, 1..=1, ())
            .unwrap();
        let surface = compositor.create_surface(&handle, ());
        let timer = manager.get_timer(&surface, &handle, ());
        timer.set_timestamp(0, 0, 1_000_000_000);
        assert!(queue.roundtrip(&mut TimingClient).is_err());
        result.send(TimingError::InvalidTimestamp).unwrap();
    })
}

fn dispatch_until_timing_error(
    runtime: &mut WaylandRuntime,
    results: &mpsc::Receiver<TimingError>,
) -> TimingError {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(result) = results.try_recv() {
            return result;
        }
    }
    panic!("surface-timing client did not complete before the dispatch limit");
}

fn dispatch_until_fifo_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<FifoStep>,
) -> FifoStep {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("FIFO client did not complete before the dispatch limit");
}

fn dispatch_until_timer_step(
    runtime: &mut WaylandRuntime,
    steps: &mpsc::Receiver<TimerStep>,
) -> TimerStep {
    for _ in 0..200 {
        runtime
            .event_loop
            .dispatch(Duration::from_millis(5), &mut runtime.state)
            .unwrap();
        if let Ok(step) = steps.try_recv() {
            return step;
        }
    }
    panic!("commit-timing client did not complete before the dispatch limit");
}

fn monotonic_after(delay: Duration) -> (u64, u32) {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let seconds = u64::try_from(now.tv_sec).unwrap();
    let nanos = u64::try_from(now.tv_nsec).unwrap() + u64::from(delay.subsec_nanos());
    (
        seconds + delay.as_secs() + nanos / 1_000_000_000,
        u32::try_from(nanos % 1_000_000_000).unwrap(),
    )
}
