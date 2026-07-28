//! Layer-shell and surface region helpers on [`NativeShell`].

use wayland_client::Proxy;
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1;

use super::api::NativeShell;
use super::types::{LayerRecord, NativeSurfaceId};
use crate::geometry::LogicalSize;
use crate::layer_shell::{LayerAnchor, LayerKeyboardInteractivity, LayerSurfaceLayer};
use crate::native::connection::NativeError;
use crate::native::protocols::core::shm;

enum SurfaceRegionKind {
    Opaque,
    Input,
}

impl NativeShell {
    /// Create a `zwlr_layer_surface_v1` (panel / bar / overlay).
    /// Convenience constructor; prefer [`Self::create_layer_surface_full`] for
    /// structured attributes.
    #[allow(clippy::too_many_arguments)]
    pub fn create_layer_surface(
        &mut self,
        namespace: impl Into<String>,
        layer: LayerSurfaceLayer,
        width: u32,
        height: u32,
        anchor: LayerAnchor,
        exclusive_zone: i32,
        keyboard: LayerKeyboardInteractivity,
    ) -> Result<NativeSurfaceId, NativeError> {
        use crate::layer_shell::{LayerMargins, LayerSurfaceState};
        let initial = LayerSurfaceState {
            size: LogicalSize::new(width.max(1), height.max(1)),
            anchor,
            exclusive_zone,
            exclusive_edge: None,
            margins: LayerMargins::default(),
            keyboard_interactivity: keyboard,
            layer,
        };
        self.create_layer_surface_full(namespace, None, initial)
    }

    /// Create a layer surface from full attributes (optional output binding).
    ///
    /// Attaches a solid SHM fill so the surface maps without a GPU client.
    /// Prefer [`Self::create_layer_surface_gpu`] for Vulkan / wgpu present.
    pub fn create_layer_surface_full(
        &mut self,
        namespace: impl Into<String>,
        output: Option<u32>,
        state: crate::layer_shell::LayerSurfaceState,
    ) -> Result<NativeSurfaceId, NativeError> {
        self.create_layer_surface_inner(namespace, output, state, false)
    }

    /// Create a **bufferless** layer surface for Vulkan / wgpu swapchain present.
    ///
    /// Initial commit has no `wl_buffer` attach (same model as
    /// [`Self::create_toplevel_gpu`]). Configure does not auto-attach SHM.
    /// Callers own present via raw-window-handle / `VK_KHR_wayland_surface`.
    pub fn create_layer_surface_gpu(
        &mut self,
        namespace: impl Into<String>,
        output: Option<u32>,
        state: crate::layer_shell::LayerSurfaceState,
    ) -> Result<NativeSurfaceId, NativeError> {
        self.create_layer_surface_inner(namespace, output, state, true)
    }

    fn create_layer_surface_inner(
        &mut self,
        namespace: impl Into<String>,
        output: Option<u32>,
        state: crate::layer_shell::LayerSurfaceState,
        bufferless: bool,
    ) -> Result<NativeSurfaceId, NativeError> {
        let namespace = namespace.into();
        crate::layer_shell::validate_layer_state(
            &state,
            Some(&namespace),
            self.state.layer_shell_version,
        )
        .map_err(|e| NativeError::Protocol(e.to_string()))?;
        let qh = self.queue.handle();
        let compositor = self
            .state
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?;
        let shell = self
            .state
            .layer_shell
            .as_ref()
            .ok_or_else(|| NativeError::Protocol("zwlr_layer_shell_v1 missing".into()))?;

        // Output proxies are keyed by registry global name.
        let output_proxy = output.and_then(|name| self.state.output_proxies.get(&name).cloned());

        let wl = compositor.create_surface(&qh, ());
        // Fractional-scale clients keep buffer_scale at 1 (same as toplevel GPU).
        wl.set_buffer_scale(1);

        let viewport = self
            .state
            .viewporter
            .as_ref()
            .map(|vp| vp.get_viewport(&wl, &qh, ()));
        let dest_w = state.size.width.max(1) as i32;
        let dest_h = state.size.height.max(1) as i32;
        // Zero size means compositor-chosen; only set destination when both axes are fixed.
        if state.size.width > 0
            && state.size.height > 0
            && let Some(vp) = viewport.as_ref()
        {
            vp.set_destination(dest_w, dest_h);
        }

        let fractional = self
            .state
            .fractional_manager
            .as_ref()
            .map(|mgr| mgr.get_fractional_scale(&wl, &qh, ()));

        let layer_surface = shell.get_layer_surface(
            &wl,
            output_proxy.as_ref(),
            state.layer.into(),
            namespace,
            &qh,
            (),
        );
        apply_layer_state_to_role(&layer_surface, &state, self.state.layer_shell_version)?;
        // Bufferless (GPU) path: commit role only — no SHM attach.
        // SHM path: solid fill so the surface is visible without a renderer.
        let (buffer, pool, file) = if bufferless {
            wl.commit();
            (None, None, None)
        } else {
            let shm = self
                .state
                .shm
                .as_ref()
                .ok_or_else(|| NativeError::Registry("wl_shm".into()))?;
            let bw = state.size.width.max(1);
            let bh = state.size.height.max(1);
            let (file, pool, buffer) =
                shm::create_solid_buffer(shm, &qh, bw, bh, [0xff, 0x18, 0x18, 0x22])
                    .map_err(|e| NativeError::Io(e.to_string()))?;
            wl.commit();
            (Some(buffer), Some(pool), Some(file))
        };

        let id = self.state.alloc_id();
        self.state
            .layer_objects
            .insert(layer_surface.id().protocol_id(), id);
        self.state
            .wl_surface_objects
            .insert(wl.id().protocol_id(), id);
        if let Some(ref frac) = fractional {
            self.state
                .fractional_objects
                .insert(frac.id().protocol_id(), id);
        }
        self.state.layers.insert(
            id,
            LayerRecord {
                wl,
                layer: layer_surface,
                buffer,
                _pool: pool,
                _file: file,
                viewport,
                fractional,
                scale_factor: 1.0,
                configured: false,
                pending_size: Some((state.size.width, state.size.height)),
                logical_w: state.size.width,
                logical_h: state.size.height,
                state,
            },
        );
        self.connection.mark_dirty();
        Ok(id)
    }

    /// Apply double-buffered layer surface state (call commit after).
    pub fn set_layer_surface_state(
        &mut self,
        id: NativeSurfaceId,
        new_state: crate::layer_shell::LayerSurfaceState,
    ) -> Result<(), NativeError> {
        let version = self.state.layer_shell_version;
        crate::layer_shell::validate_layer_state(&new_state, None, version)
            .map_err(|e| NativeError::Protocol(e.to_string()))?;
        let record = self
            .state
            .layers
            .get_mut(&id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown layer {id:?}")))?;
        if record.state == new_state {
            return Ok(());
        }
        if record.state.layer != new_state.layer && version < 2 {
            return Err(NativeError::Protocol(
                crate::layer_shell::LayerSurfaceError::DynamicLayerUnsupported.to_string(),
            ));
        }
        apply_layer_state_to_role(&record.layer, &new_state, version)?;
        record.state = new_state;
        record.logical_w = new_state.size.width;
        record.logical_h = new_state.size.height;
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn layer_surface_state(
        &self,
        id: NativeSurfaceId,
    ) -> Result<crate::layer_shell::LayerSurfaceState, NativeError> {
        self.state
            .layers
            .get(&id)
            .map(|l| l.state)
            .ok_or_else(|| NativeError::Protocol(format!("unknown layer {id:?}")))
    }

    pub fn destroy_layer_surface(&mut self, id: NativeSurfaceId) -> Result<(), NativeError> {
        self.state.cancel_touch_for_surface(id);
        self.state.clear_surface_protocol_state(id);
        let Some(record) = self.state.layers.remove(&id) else {
            return Err(NativeError::Protocol(format!("unknown layer {id:?}")));
        };
        self.state
            .layer_objects
            .remove(&record.layer.id().protocol_id());
        self.state
            .wl_surface_objects
            .remove(&record.wl.id().protocol_id());
        if let Some(ref frac) = record.fractional {
            self.state
                .fractional_objects
                .remove(&frac.id().protocol_id());
            frac.destroy();
        }
        if let Some(vp) = record.viewport {
            vp.destroy();
        }
        record.layer.destroy();
        if let Some(buffer) = record.buffer {
            buffer.destroy();
        }
        if let Some(pool) = record._pool {
            pool.destroy();
        }
        record.wl.destroy();
        self.connection.mark_dirty();
        Ok(())
    }

    pub fn is_layer_configured(&self, id: NativeSurfaceId) -> bool {
        self.state.layers.get(&id).is_some_and(|l| l.configured)
    }

    pub fn layer_count(&self) -> usize {
        self.state.layers.len()
    }

    /// Set `wl_surface.set_opaque_region` (double-buffered; commit to apply).
    ///
    /// Wallpaper / fullscreen GPU layers typically use
    /// [`crate::SurfaceRegion::full`] so the compositor can skip blending.
    pub fn set_opaque_region(
        &mut self,
        id: NativeSurfaceId,
        region: crate::SurfaceRegion,
    ) -> Result<(), NativeError> {
        self.apply_surface_region(id, region, SurfaceRegionKind::Opaque)
    }

    /// Set `wl_surface.set_input_region` (double-buffered; commit to apply).
    ///
    /// [`crate::SurfaceRegion::Empty`] is pointer passthrough (clicks fall
    /// through to surfaces below) — the usual wallpaper policy.
    pub fn set_input_region(
        &mut self,
        id: NativeSurfaceId,
        region: crate::SurfaceRegion,
    ) -> Result<(), NativeError> {
        self.apply_surface_region(id, region, SurfaceRegionKind::Input)
    }

    fn apply_surface_region(
        &mut self,
        id: NativeSurfaceId,
        region: crate::SurfaceRegion,
        kind: SurfaceRegionKind,
    ) -> Result<(), NativeError> {
        let qh = self.queue.handle();
        let compositor = self
            .state
            .compositor
            .as_ref()
            .ok_or_else(|| NativeError::Registry("wl_compositor".into()))?
            .clone();
        let wl = self
            .state
            .wl_surface(id)
            .ok_or_else(|| NativeError::Protocol(format!("unknown surface {id:?}")))?
            .clone();

        match region {
            crate::SurfaceRegion::Default => match kind {
                SurfaceRegionKind::Opaque => wl.set_opaque_region(None),
                SurfaceRegionKind::Input => wl.set_input_region(None),
            },
            crate::SurfaceRegion::Empty | crate::SurfaceRegion::Rectangles(_) => {
                let wl_region = compositor.create_region(&qh, ());
                if let crate::SurfaceRegion::Rectangles(rects) = region {
                    for rect in rects.into_iter().filter(|r| !r.is_empty()) {
                        wl_region.add(
                            rect.origin.x,
                            rect.origin.y,
                            rect.size.width.max(1) as i32,
                            rect.size.height.max(1) as i32,
                        );
                    }
                }
                // Empty: created region with no rects → no hits.
                match kind {
                    SurfaceRegionKind::Opaque => wl.set_opaque_region(Some(&wl_region)),
                    SurfaceRegionKind::Input => wl.set_input_region(Some(&wl_region)),
                }
                wl_region.destroy();
            }
        }
        self.connection.mark_dirty();
        Ok(())
    }
}

fn apply_layer_state_to_role(
    role: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    state: &crate::layer_shell::LayerSurfaceState,
    version: u32,
) -> Result<(), NativeError> {
    role.set_size(state.size.width, state.size.height);
    role.set_anchor(state.anchor.to_wire());
    role.set_exclusive_zone(state.exclusive_zone);
    role.set_margin(
        state.margins.top,
        state.margins.right,
        state.margins.bottom,
        state.margins.left,
    );
    role.set_keyboard_interactivity(state.keyboard_interactivity.into());
    if version >= 2 {
        role.set_layer(state.layer.into());
    }
    if version >= 5
        && let Some(edge) = state.exclusive_edge
    {
        role.set_exclusive_edge(edge.to_wire());
    }
    Ok(())
}
