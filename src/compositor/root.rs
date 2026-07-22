use thiserror::Error;
use tracing::{info, warn};

#[cfg(feature = "systemd")]
use std::ffi::OsString;

use crate::{
    config::{Config, StartupCommand},
    ecs::CompositorWorld,
    ipc::{IpcError, IpcServer},
    layout::{LayoutEngine, Rect},
    protocol::{ProtocolError, WaylandRuntime},
    render::RendererTarget,
    service::{SystemdMode, session_environment},
    spawn::ProcessLauncher,
    xwayland::XWaylandConfig,
};

#[cfg(feature = "systemd")]
use crate::service::EnvironmentValue;

pub struct Compositor {
    protocol: WaylandRuntime,
    ipc: IpcServer,
    world: CompositorWorld,
    layout: LayoutEngine,
    renderer: RendererTarget,
    launcher: ProcessLauncher,
    startup_commands: Vec<StartupCommand>,
    systemd: SystemdMode,
    xwayland: XWaylandConfig,
}

impl Compositor {
    pub fn new(config: Config) -> Result<Self, CompositorError> {
        let Config {
            initial_layout,
            ipc_socket,
            gpu_preference,
            systemd,
            xwayland,
            startup_commands,
        } = config;
        let protocol = WaylandRuntime::new()?;
        let ipc = IpcServer::bind(ipc_socket)?;
        let environment = session_environment(
            protocol
                .socket_name()
                .ok_or(CompositorError::MissingWaylandSocket)?
                .to_os_string(),
            ipc.path().as_os_str().to_os_string(),
        );
        Ok(Self {
            protocol,
            ipc,
            world: CompositorWorld::new(),
            layout: LayoutEngine::new(initial_layout),
            renderer: RendererTarget::with_gpu_preference(gpu_preference),
            launcher: ProcessLauncher::new(systemd).with_environment(environment),
            startup_commands,
            systemd,
            xwayland,
        })
    }

    pub fn check_ready(&mut self) {
        let preview = self.layout.arrange(Rect::new(0, 0, 1920, 1080), 3);
        info!(
            protocol = self.protocol.backend_name(),
            wayland_socket = ?self.protocol.socket_name(),
            ipc = %self.ipc.path().display(),
            vulkan = %self.renderer.api_version,
            descriptors = self.renderer.descriptor_heap.name(),
            gpu = self.renderer.device.preference().name(),
            layout = self.layout.kind().name(),
            systemd = self.systemd.name(),
            spawn_strategy = self.launcher.strategy().name(),
            startup_commands = self.startup_commands.len(),
            xwayland = self.xwayland.enabled(),
            preview_views = preview.len(),
            ecs_views = self.world.view_count(0),
            "compositor skeleton is ready"
        );
    }

    pub fn systemd_mode(&self) -> SystemdMode {
        self.systemd
    }

    pub fn spawn_startup_commands(&self) {
        for command in &self.startup_commands {
            let Some((program, args)) = command.argv.split_first() else {
                continue;
            };
            match self.launcher.spawn(program, args) {
                Ok(process) => info!(
                    program,
                    pid = process.pid(),
                    strategy = process.strategy().name(),
                    "startup command launched"
                ),
                Err(error) => warn!(program, %error, "startup command failed"),
            }
        }
    }

    #[cfg(feature = "systemd")]
    pub fn session_environment(&self) -> Result<Vec<EnvironmentValue>, CompositorError> {
        let wayland = self
            .protocol
            .socket_name()
            .ok_or(CompositorError::MissingWaylandSocket)?;
        Ok(session_environment(
            wayland.to_os_string(),
            OsString::from(self.ipc.path()),
        ))
    }

    pub fn prepare_runtime(&mut self) -> Result<(), CompositorError> {
        self.protocol.prepare(self.xwayland.enabled())?;
        Ok(())
    }

    pub fn run(mut self) -> Result<(), CompositorError> {
        self.protocol.run()?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CompositorError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Ipc(#[from] IpcError),
    #[error("Smithay did not provide a Wayland socket name")]
    MissingWaylandSocket,
}
