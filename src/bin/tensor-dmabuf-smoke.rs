//! A real linux-dmabuf client used to prove Tensor's native presentation path.
//!
//! The client deliberately has no SHM fallback: successful completion means a
//! GBM-backed dma-buf was accepted through \`zwp_linux_dmabuf_v1\`, sampled by
//! Tensor, submitted through Vulkan/KMS, presented, and then released.

#[path = "tensor-dmabuf-smoke/buffer_pool.rs"]
mod buffer_pool;
#[path = "tensor-dmabuf-smoke/feedback.rs"]
mod feedback;
#[path = "tensor-dmabuf-smoke/state.rs"]
mod state;

use std::{
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use buffer_pool::{BufferPool, find_render_node};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use clap::Parser;
use state::SmokeState;
use thiserror::Error;
use wayland_client::{Connection, globals::registry_queue_init, protocol::wl_compositor};
use wayland_protocols::{
    wp::{
        linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1, presentation_time::client::wp_presentation,
    },
    xdg::shell::client::xdg_wm_base,
};

const DEFAULT_WIDTH: u32 = 320;
const DEFAULT_HEIGHT: u32 = 240;
const DEFAULT_FRAMES: usize = 3;
const DEFAULT_TIMEOUT_SECONDS: u64 = 12;

#[derive(Debug, Parser)]
#[command(about = "Exercise Tensor's linux-dmabuf to Vulkan/KMS presentation path")]
struct Args {
    /// Tensor Wayland socket name or absolute socket path. Omit to use WAYLAND_DISPLAY.
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Width of every submitted dma-buf, in pixels.
    #[arg(long, default_value_t = DEFAULT_WIDTH)]
    width: u32,
    /// Height of every submitted dma-buf, in pixels.
    #[arg(long, default_value_t = DEFAULT_HEIGHT)]
    height: u32,
    /// Number of distinct buffers to submit; two or three proves replacement and release.
    #[arg(long, default_value_t = DEFAULT_FRAMES)]
    frames: usize,
    /// Fail if the complete presentation loop takes longer than this many seconds.
    #[arg(long, default_value_t = DEFAULT_TIMEOUT_SECONDS)]
    timeout: u64,
}

fn main() -> Result<(), SmokeError> {
    let args = Args::parse();
    validate_args(&args)?;
    run(args)
}

fn validate_args(args: &Args) -> Result<(), SmokeError> {
    if args.width == 0 || args.height == 0 {
        return Err(SmokeError::InvalidDimensions {
            width: args.width,
            height: args.height,
        });
    }
    if !(2..=3).contains(&args.frames) {
        return Err(SmokeError::InvalidFrameCount(args.frames));
    }
    if args.timeout == 0 {
        return Err(SmokeError::InvalidTimeout);
    }
    Ok(())
}

fn run(args: Args) -> Result<(), SmokeError> {
    let connection = connect(&args)?;
    let (globals, mut queue) = registry_queue_init::<SmokeState>(&connection)
        .map_err(|error| SmokeError::Wayland(error.to_string()))?;
    let queue_handle = queue.handle();

    let compositor = globals
        .bind::<wl_compositor::WlCompositor, _, _>(&queue_handle, 1..=6, ())
        .map_err(|error| SmokeError::Wayland(error.to_string()))?;
    let wm_base = globals
        .bind::<xdg_wm_base::XdgWmBase, _, _>(&queue_handle, 1..=7, ())
        .map_err(|error| SmokeError::Wayland(error.to_string()))?;
    let dmabuf = globals
        .bind::<zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1, _, _>(&queue_handle, 4..=4, ())
        .map_err(|error| SmokeError::Wayland(error.to_string()))?;
    let presentation = globals
        .bind::<wp_presentation::WpPresentation, _, _>(&queue_handle, 1..=2, ())
        .map_err(|error| SmokeError::Wayland(error.to_string()))?;

    let surface = compositor.create_surface(&queue_handle, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &queue_handle, ());
    let toplevel = xdg_surface.get_toplevel(&queue_handle, ());
    toplevel.set_title("Tensor dma-buf smoke".to_owned());
    toplevel.set_app_id("dev.tensor.DmabufSmoke".to_owned());

    let mut state = SmokeState::new(surface, xdg_surface, toplevel, presentation, args.frames);
    let feedback = dmabuf.get_default_feedback(&queue_handle, ());
    queue
        .roundtrip(&mut state)
        .map_err(|error| SmokeError::Wayland(error.to_string()))?;
    state.check_failure()?;
    feedback.destroy();

    let feedback = state.take_feedback()?;
    let render_device = find_render_node(feedback.main_device)?;
    let candidates = feedback.preferred_formats()?;
    let pool = BufferPool::allocate(
        &render_device,
        &candidates,
        args.width,
        args.height,
        args.frames,
    )?;
    println!(
        "tensor-dmabuf-smoke: selected render_device={} format={:#010x} modifier={:#018x} buffers={}",
        render_device.display(),
        pool.format.fourcc,
        pool.format.modifier,
        args.frames,
    );

    state.request_buffers(&dmabuf, &queue_handle, &pool)?;
    state.commit_initial_surface();

    let deadline = Instant::now() + Duration::from_secs(args.timeout);
    let mut event_loop = EventLoop::<SmokeState>::try_new()
        .map_err(|error| SmokeError::EventLoop(error.to_string()))?;
    WaylandSource::new(connection, queue)
        .insert(event_loop.handle())
        .map_err(|error| SmokeError::EventLoop(error.to_string()))?;

    loop {
        state.check_failure()?;
        if state.is_healthy() {
            println!("tensor-dmabuf-smoke: PASS {}", state.success_report());
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(SmokeError::Timeout(state.progress()));
        }
        event_loop
            .dispatch(Some(remaining.min(Duration::from_millis(100))), &mut state)
            .map_err(|error| SmokeError::EventLoop(error.to_string()))?;
    }
}

fn connect(args: &Args) -> Result<Connection, SmokeError> {
    match &args.socket {
        None => {
            Connection::connect_to_env().map_err(|error| SmokeError::Wayland(error.to_string()))
        }
        Some(socket) => {
            let path = socket_path(socket)?;
            let stream =
                UnixStream::connect(&path).map_err(|source| SmokeError::Socket { path, source })?;
            Connection::from_socket(stream).map_err(|error| SmokeError::Wayland(error.to_string()))
        }
    }
}

fn socket_path(socket: &Path) -> Result<PathBuf, SmokeError> {
    if socket.is_absolute() {
        return Ok(socket.to_owned());
    }
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").ok_or(SmokeError::RuntimeDirMissing)?;
    Ok(PathBuf::from(runtime_dir).join(socket))
}

#[derive(Debug, Error)]
enum SmokeError {
    #[error("width and height must be non-zero, got {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error(
        "--frames must be two or three so the smoke test proves replacement and release, got {0}"
    )]
    InvalidFrameCount(usize),
    #[error("--timeout must be greater than zero")]
    InvalidTimeout,
    #[error("XDG_RUNTIME_DIR is required when --socket is a relative socket name")]
    RuntimeDirMissing,
    #[error("failed to connect to Wayland socket {path}: {source}")]
    Socket {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Wayland operation failed: {0}")]
    Wayland(String),
    #[error("failed to drive the Wayland event loop: {0}")]
    EventLoop(String),
    #[error("failed to read DRM directory {path}: {source}")]
    ReadDrmDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("linux-dmabuf feedback device {0} does not resolve to a /dev/dri/renderD* node")]
    RenderNodeNotFound(u64),
    #[error("failed to open render node {path}: {source}")]
    OpenRenderNode {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("GBM failed to {context}: {source}")]
    Gbm {
        context: &'static str,
        source: std::io::Error,
    },
    #[error("could not allocate an explicit-modifier GBM dma-buf: {0}")]
    NoGbmAllocation(String),
    #[error("buffer dimensions exceed Wayland's signed range")]
    DimensionsTooLarge,
    #[error("invalid linux-dmabuf feedback: {0}")]
    InvalidFeedback(String),
    #[error("dma-buf presentation health check failed: {0}")]
    Health(String),
    #[error("dma-buf presentation health check timed out: {0}")]
    Timeout(String),
}
