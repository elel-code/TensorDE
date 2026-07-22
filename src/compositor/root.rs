use std::ffi::OsString;

use thiserror::Error;
use tracing::{info, warn};

use crate::{
    backend::BackendConfig,
    config::{Config, StartupCommand},
    ecs::{CompositorWorld, WorkspaceId},
    ipc::{
        Command as IpcCommand, IPC_PROTOCOL_VERSION, IpcError, IpcReply, IpcServer, Request,
        Response, ResultBody, StateSnapshot,
    },
    layout::{LayoutEngine, LayoutItem, LayoutState, Rect},
    protocol::{ProtocolError, WaylandRuntime},
    render::{DrmNodeError, DrmNodeId, RendererError, RendererTarget, VulkanRenderer},
    service::{EnvironmentValue, SystemdMode, session_environment},
    spawn::ProcessLauncher,
    startup::SessionAutostartPermit,
    xwayland::XWaylandConfig,
};

pub struct Compositor {
    protocol: WaylandRuntime,
    ipc: IpcServer,
    backend_config: BackendConfig,
    renderer: VulkanRenderer,
    launcher: ProcessLauncher,
    startup_commands: Vec<StartupCommand>,
    systemd: SystemdMode,
    xwayland: XWaylandConfig,
}

impl Compositor {
    pub fn new(config: Config) -> Result<Self, CompositorError> {
        let Config {
            initial_layout,
            layout_options,
            ipc_socket,
            gpu_preference,
            render_device,
            systemd,
            xwayland,
            startup_commands,
        } = config;
        let protocol =
            WaylandRuntime::new(LayoutEngine::with_options(initial_layout, layout_options))?;
        let requested_drm_node = render_device
            .as_deref()
            .map(DrmNodeId::from_path)
            .transpose()?;
        let renderer = VulkanRenderer::new(RendererTarget::with_device(
            gpu_preference,
            requested_drm_node,
        ))?;
        let ipc = IpcServer::bind(ipc_socket)?;
        Ok(Self {
            protocol,
            ipc,
            backend_config: BackendConfig {
                drm_node: renderer.selected().render_node,
                renderer_formats: renderer.selected().formats.clone(),
            },
            renderer,
            launcher: ProcessLauncher::new(systemd),
            startup_commands,
            systemd,
            xwayland,
        })
    }

    pub fn check_ready(&mut self) {
        let renderer_target = self.renderer.target();
        let selected_device = self.renderer.selected();
        let (
            preview_views,
            layout,
            layout_options,
            ecs_views,
            outputs,
            seat,
            xdg_output,
            protocol_globals,
        ) = {
            let state = self.protocol.state_mut();
            let mut preview_state = LayoutState::default();
            let preview_items = [LayoutItem::default(); 3];
            (
                state
                    .layout
                    .arrange(
                        &mut preview_state,
                        Rect::new(0, 0, 1920, 1080),
                        &preview_items,
                        Some(0),
                    )
                    .placements
                    .len(),
                state.layout.kind(),
                state.layout.options(),
                state.view_count(),
                state.output_count(),
                state.seat.name().to_owned(),
                state
                    .output_manager_state
                    .xdg_output_manager_global()
                    .is_some(),
                state.protocol_globals.capabilities(),
            )
        };
        info!(
            protocol = self.protocol.backend_name(),
            wayland_socket = ?self.protocol.socket_name(),
            ipc = %self.ipc.path().display(),
            vulkan = %renderer_target.api_version,
            descriptors = renderer_target.descriptor_heap.name(),
            gpu_preference = renderer_target.device.preference().name(),
            gpu = selected_device.name,
            gpu_type = ?selected_device.device_type,
            graphics_queue_family = selected_device.graphics_queue_family,
            layout = layout.name(),
            layout_options = ?layout_options,
            systemd = self.systemd.name(),
            spawn_strategy = self.launcher.strategy().name(),
            startup_commands = self.startup_commands.len(),
            xwayland = self.xwayland.enabled(),
            preview_views,
            ecs_views,
            outputs,
            seat,
            xdg_output,
            protocol_globals = ?protocol_globals,
            "compositor runtime is ready"
        );
    }

    pub(crate) fn spawn_startup_commands(&self, _permit: SessionAutostartPermit) {
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

    pub(crate) fn publish_session_environment(
        &mut self,
    ) -> Result<Vec<EnvironmentValue>, CompositorError> {
        let wayland = self
            .protocol
            .socket_name()
            .ok_or(CompositorError::MissingWaylandSocket)?;
        let environment = session_environment(
            wayland.to_os_string(),
            OsString::from(self.ipc.path()),
            self.protocol.xwayland_display(),
        );
        self.launcher.set_environment(environment.clone());
        Ok(environment)
    }

    pub fn prepare_runtime(&mut self) -> Result<(), CompositorError> {
        self.protocol.prepare(self.xwayland.enabled())?;
        self.protocol.prepare_backend(&self.backend_config)?;
        Ok(())
    }

    pub fn run(self) -> Result<(), CompositorError> {
        let Self {
            mut protocol,
            ipc,
            backend_config: _,
            renderer,
            launcher,
            startup_commands,
            systemd,
            xwayland,
        } = self;
        let runtime_owners = (renderer, launcher, startup_commands, systemd, xwayland);
        let stop_signal = protocol.stop_signal();
        protocol.run_with_ipc(&ipc, move |request, state| {
            let reflow = matches!(&request.command, IpcCommand::SetLayout { .. });
            let reply =
                handle_ipc_request(request, &mut state.layout, &mut state.world, &stop_signal);
            if reflow {
                state.reflow_default_workspace();
            }
            reply
        })?;
        drop(runtime_owners);
        Ok(())
    }
}

fn handle_ipc_request(
    request: Request,
    layout: &mut LayoutEngine,
    world: &mut CompositorWorld,
    stop_signal: &smithay::reexports::calloop::LoopSignal,
) -> IpcReply {
    let request_id = request.request_id;
    if request.version != IPC_PROTOCOL_VERSION {
        return IpcReply::new(Response::error(
            request_id,
            "unsupported_version",
            format!(
                "protocol version {} is unsupported; expected {IPC_PROTOCOL_VERSION}",
                request.version
            ),
        ));
    }

    let result = match request.command {
        IpcCommand::Ping => ResultBody::Pong,
        IpcCommand::GetState => ResultBody::State(StateSnapshot {
            layout: layout.kind(),
            view_count: world.view_count(WorkspaceId::new(0)),
        }),
        IpcCommand::SetLayout { layout: kind } => {
            *layout = LayoutEngine::with_options(kind, layout.options());
            world.reset_layout_states();
            ResultBody::Accepted
        }
        IpcCommand::Quit => {
            return IpcReply::stop_after_flush(
                Response::new(request_id, ResultBody::Accepted),
                stop_signal.clone(),
            );
        }
    };
    IpcReply::new(Response::new(request_id, result))
}

#[derive(Debug, Error)]
pub enum CompositorError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Ipc(#[from] IpcError),
    #[error(transparent)]
    Renderer(#[from] RendererError),
    #[error(transparent)]
    DrmNode(#[from] DrmNodeError),
    #[error("Smithay did not provide a Wayland socket name")]
    MissingWaylandSocket,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ecs::ViewId, layout::LayoutKind};
    use smithay::reexports::calloop::EventLoop;

    fn stop_signal() -> smithay::reexports::calloop::LoopSignal {
        EventLoop::<()>::try_new().unwrap().get_signal()
    }

    #[test]
    fn ipc_rejects_unknown_protocol_versions() {
        let mut world = CompositorWorld::new();
        let mut layout = LayoutEngine::new(LayoutKind::Scrolling1D);
        let mut request = Request::new(11, IpcCommand::Ping);
        request.version = IPC_PROTOCOL_VERSION + 1;

        let response = handle_ipc_request(request, &mut layout, &mut world, &stop_signal());

        assert_eq!(response.response.request_id, 11);
        assert!(matches!(response.response.result, ResultBody::Error(_)));
    }

    #[test]
    fn ipc_layout_change_is_visible_in_state() {
        let mut world = CompositorWorld::new();
        world
            .spawn_view(ViewId::new(1), WorkspaceId::new(0))
            .unwrap();
        let options = crate::layout::LayoutOptions {
            gap: 17,
            ..Default::default()
        };
        let mut layout = LayoutEngine::with_options(LayoutKind::Scrolling1D, options);

        let changed = handle_ipc_request(
            Request::new(
                12,
                IpcCommand::SetLayout {
                    layout: LayoutKind::Spatial2D,
                },
            ),
            &mut layout,
            &mut world,
            &stop_signal(),
        );
        assert!(matches!(changed.response.result, ResultBody::Accepted));

        let state = handle_ipc_request(
            Request::new(13, IpcCommand::GetState),
            &mut layout,
            &mut world,
            &stop_signal(),
        );
        let ResultBody::State(state) = state.response.result else {
            panic!("expected IPC state response");
        };
        assert_eq!(state.layout, LayoutKind::Spatial2D);
        assert_eq!(state.view_count, 1);
        assert_eq!(layout.options(), options);
    }
}
