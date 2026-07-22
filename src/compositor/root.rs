use thiserror::Error;
use tracing::info;

use crate::{
    config::Config,
    ecs::CompositorWorld,
    ipc::{IpcError, IpcServer},
    layout::{LayoutEngine, Rect},
    protocol::{ProtocolError, WaylandRuntime},
    render::RendererTarget,
};

pub struct Compositor {
    protocol: WaylandRuntime,
    ipc: IpcServer,
    world: CompositorWorld,
    layout: LayoutEngine,
    renderer: RendererTarget,
}

impl Compositor {
    pub fn new(config: Config) -> Result<Self, CompositorError> {
        Ok(Self {
            protocol: WaylandRuntime::new()?,
            ipc: IpcServer::bind(config.ipc_socket)?,
            world: CompositorWorld::new(),
            layout: LayoutEngine::new(config.initial_layout),
            renderer: RendererTarget::default(),
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
            layout = self.layout.kind().name(),
            preview_views = preview.len(),
            ecs_views = self.world.view_count(0),
            "compositor skeleton is ready"
        );
    }
}

#[derive(Debug, Error)]
pub enum CompositorError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Ipc(#[from] IpcError),
}
