//! Wayland subsurface client-side decoration frame.

use std::fs::File;

use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
};
use wayland_client::{Proxy, QueueHandle};

use crate::event::ToplevelState;
use crate::native::connection::NativeError;
use crate::native::protocols::core::shm;
use crate::native::shell::types::{NativeShellState, NativeSurfaceId};

use super::buttons::Buttons;
use super::geometry::{content_insets, PartLayout, BORDER_SIZE, HEADER_SIZE};
use super::input::{
    coarse_for_part, header_hit, refine_edge, FrameAction, FrameCursor, FramePartKind, HitLocation,
    MouseState,
};
use super::paint::{paint_border_strip, paint_header, EdgeSide, Pixmap};
use super::theme::ColorTheme;

struct PartSurface {
    wl: wl_surface::WlSurface,
    sub: wl_subsurface::WlSubsurface,
    buffer: Option<wl_buffer::WlBuffer>,
    pool: Option<wl_shm_pool::WlShmPool>,
    file: Option<File>,
    kind: FramePartKind,
    /// Current logical size of this part.
    width: u32,
    height: u32,
}

impl PartSurface {
    fn destroy(self) {
        if let Some(b) = self.buffer {
            b.destroy();
        }
        if let Some(p) = self.pool {
            p.destroy();
        }
        self.sub.destroy();
        self.wl.destroy();
    }
}

/// Per-toplevel client-side decoration frame (subsurfaces + input state).
pub struct ClientSideFrame {
    parent: NativeSurfaceId,
    parts: Vec<PartSurface>,
    buttons: Buttons,
    mouse: MouseState,
    theme: ColorTheme,
    title: String,
    /// Content logical size (the application surface).
    content_w: u32,
    content_h: u32,
    scale: f64,
    resizable: bool,
    maximized: bool,
    fullscreen: bool,
    activated: bool,
    /// True when compositor granted client-side mode (or no decoration manager).
    enabled: bool,
    /// Hide chrome entirely (user preference None, or fullscreen).
    hidden: bool,
    hide_titlebar: bool,
    dirty: bool,
    can_maximize: bool,
    can_minimize: bool,
}

impl ClientSideFrame {
    pub fn new(
        parent: NativeSurfaceId,
        content_w: u32,
        content_h: u32,
        title: String,
    ) -> Self {
        Self {
            parent,
            parts: Vec::new(),
            buttons: Buttons::default(),
            mouse: MouseState::default(),
            theme: ColorTheme::default(),
            title,
            content_w: content_w.max(1),
            content_h: content_h.max(1),
            scale: 1.0,
            resizable: true,
            maximized: false,
            fullscreen: false,
            activated: false,
            enabled: true,
            hidden: false,
            hide_titlebar: false,
            dirty: true,
            can_maximize: true,
            can_minimize: true,
        }
    }

    #[allow(dead_code)]
    pub fn parent(&self) -> NativeSurfaceId {
        self.parent
    }

    #[allow(dead_code)]
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub fn set_theme(&mut self, theme: ColorTheme) {
        self.theme = theme;
        self.dirty = true;
    }

    pub fn set_scale(&mut self, scale: f64) {
        let scale = scale.max(1.0);
        if (self.scale - scale).abs() > 0.001 {
            self.scale = scale;
            self.dirty = true;
        }
    }

    pub fn set_content_size(&mut self, w: u32, h: u32) {
        let w = w.max(1);
        let h = h.max(1);
        if self.content_w != w || self.content_h != h {
            self.content_w = w;
            self.content_h = h;
            self.dirty = true;
        }
    }

    pub fn set_toplevel_state(&mut self, state: ToplevelState) {
        let maximized = state.contains(ToplevelState::MAXIMIZED);
        let fullscreen = state.contains(ToplevelState::FULLSCREEN);
        let activated = state.contains(ToplevelState::ACTIVATED);
        if self.maximized != maximized
            || self.fullscreen != fullscreen
            || self.activated != activated
        {
            self.maximized = maximized;
            self.fullscreen = fullscreen;
            self.activated = activated;
            self.dirty = true;
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.dirty = true;
        }
    }

    pub fn set_hidden(&mut self, hidden: bool) {
        if self.hidden != hidden {
            self.hidden = hidden;
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub fn set_resizable(&mut self, resizable: bool) {
        self.resizable = resizable;
    }

    pub fn is_visible(&self) -> bool {
        self.enabled && !self.hidden && !self.fullscreen
    }

    #[allow(dead_code)]
    pub fn insets(&self) -> super::geometry::DecorationInsets {
        if !self.is_visible() {
            return super::geometry::DecorationInsets::ZERO;
        }
        content_insets(self.hide_titlebar, false)
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Ensure subsurfaces exist when subcompositor is available.
    pub fn ensure_parts(
        &mut self,
        parent_wl: &wl_surface::WlSurface,
        compositor: &wl_compositor::WlCompositor,
        subcompositor: &wl_subcompositor::WlSubcompositor,
        qh: &QueueHandle<NativeShellState>,
        state: &mut NativeShellState,
    ) {
        if !self.parts.is_empty() {
            return;
        }
        let kinds = [
            FramePartKind::Top,
            FramePartKind::Left,
            FramePartKind::Right,
            FramePartKind::Bottom,
            FramePartKind::Header,
        ];
        for kind in kinds {
            let wl = compositor.create_surface(qh, ());
            wl.set_buffer_scale(1);
            let sub = subcompositor.get_subsurface(&wl, parent_wl, qh, ());
            // Desync so we can commit frame parts independently of content.
            sub.set_desync();
            let part_id = state.alloc_id();
            state
                .wl_surface_objects
                .insert(wl.id().protocol_id(), part_id);
            state.csd_part_owners.insert(part_id, (self.parent, kind));
            // Map part surface id → parent for pointer focus rewrite.
            state.csd_surface_to_parent.insert(part_id, self.parent);
            self.parts.push(PartSurface {
                wl,
                sub,
                buffer: None,
                pool: None,
                file: None,
                kind,
                width: 0,
                height: 0,
            });
        }
        self.dirty = true;
    }

    pub fn destroy_parts(&mut self, state: &mut NativeShellState) {
        for part in self.parts.drain(..) {
            let pid = part.wl.id().protocol_id();
            if let Some(sid) = state.wl_surface_objects.remove(&pid) {
                state.csd_part_owners.remove(&sid);
                state.csd_surface_to_parent.remove(&sid);
            }
            part.destroy();
        }
    }

    /// Resize/reposition subsurfaces and repaint dirty buffers.
    pub fn redraw(
        &mut self,
        shm_global: &wl_shm::WlShm,
        qh: &QueueHandle<NativeShellState>,
    ) -> Result<(), NativeError> {
        if self.parts.is_empty() {
            return Ok(());
        }

        let visible = self.is_visible();
        if !visible {
            for part in &self.parts {
                part.wl.attach(None, 0, 0);
                part.wl.commit();
            }
            self.dirty = false;
            return Ok(());
        }

        if !self.dirty {
            return Ok(());
        }

        self.buttons
            .set_capabilities(self.can_maximize, self.can_minimize);
        self.buttons.arrange(self.content_w);

        // Maximized/tiled windows drop resize borders (header stays).
        let hide_borders = self.maximized;
        let layout = PartLayout::for_content(self.content_w, self.content_h, self.hide_titlebar);
        let scale = self.scale as f32;
        let colors = self.theme.for_state(self.activated);
        let hover = self.mouse.location;

        for part in &mut self.parts {
            let hide_part = match part.kind {
                FramePartKind::Header => self.hide_titlebar,
                FramePartKind::Top
                | FramePartKind::Left
                | FramePartKind::Right
                | FramePartKind::Bottom => hide_borders,
            };
            if hide_part {
                part.sub.set_position(0, 0);
                part.wl.attach(None, 0, 0);
                part.wl.commit();
                continue;
            }

            let rect = match part.kind {
                FramePartKind::Top => layout.top,
                FramePartKind::Left => layout.left,
                FramePartKind::Right => layout.right,
                FramePartKind::Bottom => layout.bottom,
                FramePartKind::Header => layout.header,
            };

            part.sub.set_position(rect.x, rect.y);
            part.width = rect.width.max(1);
            part.height = rect.height.max(1);

            let pixmap = match part.kind {
                FramePartKind::Header => paint_header(
                    part.width,
                    scale,
                    colors,
                    &self.title,
                    &self.buttons,
                    hover,
                    self.maximized,
                ),
                FramePartKind::Top => paint_border_strip(
                    part.width,
                    part.height,
                    scale,
                    colors.edge,
                    colors.border,
                    EdgeSide::Bottom,
                ),
                FramePartKind::Bottom => paint_border_strip(
                    part.width,
                    part.height,
                    scale,
                    colors.edge,
                    colors.border,
                    EdgeSide::Top,
                ),
                FramePartKind::Left => paint_border_strip(
                    part.width,
                    part.height,
                    scale,
                    colors.edge,
                    colors.border,
                    EdgeSide::Right,
                ),
                FramePartKind::Right => paint_border_strip(
                    part.width,
                    part.height,
                    scale,
                    colors.edge,
                    colors.border,
                    EdgeSide::Left,
                ),
            };

            attach_pixmap(part, shm_global, qh, &pixmap)?;
            part.wl.damage_buffer(0, 0, i32::MAX, i32::MAX);
            part.wl.commit();
        }

        self.dirty = false;
        Ok(())
    }

    /// Pointer entered a decoration part (surface-local coords).
    pub fn on_pointer_enter(
        &mut self,
        kind: FramePartKind,
        x: f64,
        y: f64,
    ) -> FrameCursor {
        let loc = self.hit(kind, x, y);
        self.mouse.moved(loc, x, y);
        self.dirty = true; // hover chrome
        loc.cursor(self.resizable && !self.maximized)
    }

    pub fn on_pointer_motion(
        &mut self,
        kind: FramePartKind,
        x: f64,
        y: f64,
    ) -> FrameCursor {
        let loc = self.hit(kind, x, y);
        let changed = loc != self.mouse.location;
        self.mouse.moved(loc, x, y);
        if changed {
            self.dirty = true;
        }
        loc.cursor(self.resizable && !self.maximized)
    }

    pub fn on_pointer_leave(&mut self) {
        self.mouse.left();
        self.dirty = true;
    }

    pub fn on_pointer_button(&mut self, button: u32, pressed: bool) -> Option<FrameAction> {
        self.mouse.click(
            button,
            pressed,
            self.resizable && !self.maximized && !self.fullscreen,
            self.maximized,
            self.can_maximize,
        )
    }

    fn hit(&self, kind: FramePartKind, x: f64, y: f64) -> HitLocation {
        match kind {
            FramePartKind::Header => header_hit(&self.buttons, x, y),
            other => {
                let (w, h) = self
                    .parts
                    .iter()
                    .find(|p| p.kind == other)
                    .map(|p| (p.width, p.height))
                    .unwrap_or((BORDER_SIZE, BORDER_SIZE));
                refine_edge(coarse_for_part(other), x, y, w, h)
            }
        }
    }

    #[allow(dead_code)]
    pub fn header_size() -> u32 {
        HEADER_SIZE
    }
}

fn attach_pixmap(
    part: &mut PartSurface,
    shm_global: &wl_shm::WlShm,
    qh: &QueueHandle<NativeShellState>,
    pixmap: &Pixmap,
) -> Result<(), NativeError> {
    // Drop previous buffer resources.
    if let Some(b) = part.buffer.take() {
        b.destroy();
    }
    if let Some(p) = part.pool.take() {
        p.destroy();
    }
    part.file = None;

    // Convert BGRA pixmap bytes to a proper SHM buffer. Our Pixmap already
    // stores LE ARGB8888 (B,G,R,A) which matches wl_shm Argb8888.
    let (file, pool, buffer) = create_argb_buffer_from_bytes(
        shm_global,
        qh,
        pixmap.width,
        pixmap.height,
        &pixmap.pixels,
    )
    .map_err(|e| NativeError::Io(e.to_string()))?;
    part.wl.attach(Some(&buffer), 0, 0);
    part.buffer = Some(buffer);
    part.pool = Some(pool);
    part.file = Some(file);
    Ok(())
}

fn create_argb_buffer_from_bytes<State: 'static>(
    shm_global: &wl_shm::WlShm,
    qh: &QueueHandle<State>,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> std::io::Result<(File, wl_shm_pool::WlShmPool, wl_buffer::WlBuffer)>
where
    State: wayland_client::Dispatch<wl_shm_pool::WlShmPool, ()>
        + wayland_client::Dispatch<wl_buffer::WlBuffer, ()>,
{
    use std::io::Write;
    use std::os::fd::AsFd;

    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|p| p.checked_mul(4))
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "size overflow"))?;
    if pixels.len() < expected {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "pixel buffer too short",
        ));
    }
    let stride = width.saturating_mul(4);
    let mut file = shm::create_memfd(expected.max(4))?;
    file.write_all(&pixels[..expected])?;
    file.flush()?;
    let pool = shm_global.create_pool(file.as_fd(), expected as i32, qh, ());
    let buffer = pool.create_buffer(
        0,
        width as i32,
        height as i32,
        stride as i32,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );
    Ok((file, pool, buffer))
}
