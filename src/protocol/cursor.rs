use std::{
    collections::HashMap,
    os::fd::OwnedFd,
    time::{Duration, Instant},
};

use cursor_icon::CursorIcon;
use tensor_protocol::SurfaceSampleTransform;
use tensor_util::{LogicalPoint, LogicalRect, OutputScale, Point, Rect, Size};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::{
    ecs::SurfaceBufferId,
    render::{CursorOverlay, CursorOverlays, CursorTexture},
};

mod animation;

const MAX_TABLET_CURSORS: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::protocol) enum CursorImage {
    Hidden,
    Named(CursorIcon),
    Surface(WlSurface),
}

impl CursorImage {
    pub(in crate::protocol) const fn default_named() -> Self {
        Self::Named(CursorIcon::Default)
    }
}

/// Cursor state remains in the protocol boundary because a client cursor
/// surface is a thread-affine Wayland object. The renderer receives only the
/// fixed-capacity value-only cursor batch needed for the current output frame.
pub(crate) struct CursorState {
    image: CursorImage,
    tablets: Vec<TabletCursor>,
    logical_size: u32,
    hide_when_typing: bool,
    hidden_for_typing: bool,
    theme_name: String,
    theme: xcursor::CursorTheme,
    named_rasters: HashMap<(CursorIcon, OutputScale), Option<CursorRasterSequence>>,
    animation_epoch: Instant,
    animation_timer: Option<OwnedFd>,
    animation_deadline: Option<Instant>,
    retired_surfaces: Vec<WlSurface>,
}

#[derive(Clone)]
struct TabletCursor {
    tool: tensor_event::TabletToolId,
    image: CursorImage,
    location: LogicalPoint<f64>,
}

#[derive(Clone, Copy)]
struct CursorOutput {
    logical: LogicalRect<i32>,
    scale: OutputScale,
    viewport: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol) struct CursorRaster {
    pub(in crate::protocol) buffer_id: SurfaceBufferId,
    pub(in crate::protocol) size: Size,
    pub(in crate::protocol) hotspot: Point,
    pub(in crate::protocol) sample_transform: SurfaceSampleTransform,
}

struct CursorRasterSequence {
    frames: Vec<CursorRasterFrame>,
    duration_ms: u64,
    current: usize,
}

struct CursorRasterFrame {
    raster: CursorRaster,
    delay_ms: u32,
}

impl CursorRasterSequence {
    fn current(&self) -> Option<CursorRaster> {
        self.frames.get(self.current).map(|frame| frame.raster)
    }

    fn frame_at(&self, elapsed: Duration) -> Option<(usize, Duration)> {
        if self.frames.len() <= 1 || self.duration_ms == 0 {
            return None;
        }
        let mut position = u64::try_from(elapsed.as_millis() % u128::from(self.duration_ms))
            .expect("animation position is bounded by u64 duration");
        for (index, frame) in self.frames.iter().enumerate() {
            let delay = u64::from(frame.delay_ms);
            if position < delay {
                return Some((index, Duration::from_millis(delay - position)));
            }
            position -= delay;
        }
        None
    }
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            image: CursorImage::default_named(),
            tablets: Vec::with_capacity(MAX_TABLET_CURSORS),
            logical_size: 24,
            hide_when_typing: false,
            hidden_for_typing: false,
            theme_name: "default".to_owned(),
            theme: xcursor::CursorTheme::load("default"),
            named_rasters: HashMap::with_capacity(16),
            animation_epoch: Instant::now(),
            animation_timer: animation::create_timer(),
            animation_deadline: None,
            retired_surfaces: Vec::with_capacity(MAX_TABLET_CURSORS + 1),
        }
    }
}

impl CursorState {
    pub(crate) fn configure(
        &mut self,
        theme: String,
        size: u32,
        hide_when_typing: bool,
    ) -> Vec<SurfaceBufferId> {
        let size = size.max(1);
        let changed = self.theme_name != theme || self.logical_size != size;
        let released = if changed {
            self.named_rasters
                .drain()
                .flat_map(|(_, sequence)| {
                    sequence
                        .into_iter()
                        .flat_map(|sequence| sequence.frames)
                        .map(|frame| frame.raster.buffer_id)
                })
                .collect()
        } else {
            Vec::new()
        };
        if self.theme_name != theme {
            self.theme = xcursor::CursorTheme::load(&theme);
            self.theme_name = theme;
        }
        self.logical_size = size;
        self.hide_when_typing = hide_when_typing;
        if !hide_when_typing {
            self.hidden_for_typing = false;
        }
        if changed {
            self.animation_epoch = Instant::now();
            self.disarm_animation_timer();
        }
        released
    }

    pub(crate) fn prepare_named_rasters(
        &mut self,
        scale: OutputScale,
        mut upload: impl FnMut(Size, &[u8]) -> Option<SurfaceBufferId>,
    ) {
        let now = Instant::now();
        if let CursorImage::Named(icon) = self.image {
            self.prepare_named_raster(icon, scale, &mut upload);
            self.select_named_frame(icon, scale, now);
        }
        for index in 0..self.tablets.len() {
            let CursorImage::Named(icon) = self.tablets[index].image else {
                continue;
            };
            self.prepare_named_raster(icon, scale, &mut upload);
            self.select_named_frame(icon, scale, now);
        }
    }

    fn prepare_named_raster(
        &mut self,
        icon: CursorIcon,
        scale: OutputScale,
        upload: &mut impl FnMut(Size, &[u8]) -> Option<SurfaceBufferId>,
    ) {
        let key = (icon, scale);
        if self.named_rasters.contains_key(&key) {
            return;
        }
        let desired = scale.physical_length_round(self.logical_size).max(1);
        let frames = self
            .load_named_images(icon, desired)
            .into_iter()
            .flatten()
            .filter_map(|image| {
                let size = Size::new(image.width, image.height);
                let buffer_id = upload(size, &image.pixels_rgba)?;
                Some(CursorRasterFrame {
                    raster: CursorRaster {
                        buffer_id,
                        size,
                        hotspot: Point::new(
                            i32::try_from(image.xhot).unwrap_or(i32::MAX),
                            i32::try_from(image.yhot).unwrap_or(i32::MAX),
                        ),
                        sample_transform: SurfaceSampleTransform::IDENTITY,
                    },
                    delay_ms: image.delay,
                })
            })
            .collect::<Vec<_>>();
        if frames.is_empty() {
            tracing::warn!(
                theme = self.theme_name,
                shape = icon.name(),
                desired,
                "cursor theme image is unavailable; using vector fallback"
            );
        }
        let duration_ms = frames.iter().fold(0_u64, |duration, frame| {
            duration.saturating_add(u64::from(frame.delay_ms))
        });
        self.named_rasters.insert(
            key,
            (!frames.is_empty()).then_some(CursorRasterSequence {
                frames,
                duration_ms,
                current: 0,
            }),
        );
    }

    fn load_named_images(
        &self,
        icon: CursorIcon,
        desired: u32,
    ) -> Option<Vec<xcursor::parser::Image>> {
        let names = std::iter::once(icon.name()).chain(icon.alt_names().iter().copied());
        for name in names {
            let Some(path) = self.theme.load_icon(name) else {
                continue;
            };
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let Some(images) = xcursor::parser::parse_xcursor(&bytes) else {
                continue;
            };
            let Some(closest) = images
                .iter()
                .min_by_key(|image| image.size.abs_diff(desired))
            else {
                continue;
            };
            let dimensions = (closest.width, closest.height);
            let images = images
                .into_iter()
                .filter(|image| (image.width, image.height) == dimensions)
                .collect::<Vec<_>>();
            return Some(images);
        }
        None
    }

    pub(in crate::protocol) fn set_image(&mut self, image: CursorImage) -> bool {
        if self.image == image {
            return false;
        }
        if let CursorImage::Surface(surface) = &self.image {
            self.retired_surfaces.push(surface.clone());
        }
        self.image = image;
        true
    }

    pub(in crate::protocol) fn image_matches(&self, image: &CursorImage) -> bool {
        &self.image == image
    }

    /// Hide the software cursor after a keyboard press when configured.
    pub(crate) fn will_hide_for_keyboard_activity(&self) -> bool {
        self.hide_when_typing && !self.hidden_for_typing
    }

    pub(crate) fn note_keyboard_activity(&mut self) -> bool {
        if !self.hide_when_typing || self.hidden_for_typing {
            return false;
        }
        self.hidden_for_typing = true;
        true
    }

    /// Reveal the cursor again after pointer motion.
    pub(crate) fn note_pointer_activity(&mut self) -> bool {
        if !self.hidden_for_typing {
            return false;
        }
        self.hidden_for_typing = false;
        true
    }

    pub(in crate::protocol) fn note_tablet_activity(
        &mut self,
        tool: tensor_event::TabletToolId,
        location: LogicalPoint<f64>,
    ) -> bool {
        if let Some(tablet) = self.tablets.iter_mut().find(|tablet| tablet.tool == tool) {
            let changed = tablet.location != location;
            tablet.location = location;
            return changed;
        }
        if self.tablets.len() == MAX_TABLET_CURSORS {
            tracing::warn!(tool = tool.get(), "tablet cursor capacity exceeded");
            return false;
        }
        let index = self.tablets.partition_point(|tablet| tablet.tool < tool);
        self.tablets.insert(
            index,
            TabletCursor {
                tool,
                image: CursorImage::default_named(),
                location,
            },
        );
        true
    }

    pub(in crate::protocol) fn tablet_location(
        &self,
        tool: tensor_event::TabletToolId,
    ) -> Option<LogicalPoint<f64>> {
        self.tablets
            .iter()
            .find(|tablet| tablet.tool == tool)
            .map(|tablet| tablet.location)
    }

    pub(in crate::protocol) fn set_tablet_image(
        &mut self,
        tool: tensor_event::TabletToolId,
        image: CursorImage,
    ) -> bool {
        let Some(tablet) = self.tablets.iter_mut().find(|tablet| tablet.tool == tool) else {
            return false;
        };
        if tablet.image == image {
            return false;
        }
        if let CursorImage::Surface(surface) = &tablet.image {
            self.retired_surfaces.push(surface.clone());
        }
        tablet.image = image;
        true
    }

    pub(in crate::protocol) fn tablet_image_matches(
        &self,
        tool: tensor_event::TabletToolId,
        image: &CursorImage,
    ) -> bool {
        self.tablets
            .iter()
            .find(|tablet| tablet.tool == tool)
            .is_some_and(|tablet| &tablet.image == image)
    }

    pub(in crate::protocol) fn clear_tablet(&mut self, tool: tensor_event::TabletToolId) -> bool {
        let Some(index) = self.tablets.iter().position(|tablet| tablet.tool == tool) else {
            return false;
        };
        let tablet = self.tablets.remove(index);
        if let CursorImage::Surface(surface) = tablet.image {
            self.retired_surfaces.push(surface);
        }
        true
    }

    pub(in crate::protocol) fn drain_retired_surfaces(&mut self, mut visit: impl FnMut(WlSurface)) {
        for surface in self.retired_surfaces.drain(..) {
            visit(surface);
        }
    }

    /// Remove every reference to a cursor-role surface before its Wayland
    /// resource state disappears. A live tool keeps its compositor-provided
    /// default cursor; output membership is cleaned by the runtime boundary.
    pub(in crate::protocol) fn surface_destroyed(&mut self, surface: &WlSurface) -> bool {
        let mut changed = false;
        if matches!(&self.image, CursorImage::Surface(current) if current == surface) {
            self.image = CursorImage::default_named();
            changed = true;
        }
        for tablet in &mut self.tablets {
            if matches!(&tablet.image, CursorImage::Surface(current) if current == surface) {
                tablet.image = CursorImage::default_named();
                changed = true;
            }
        }
        self.retired_surfaces.retain(|retired| retired != surface);
        changed
    }

    pub(in crate::protocol) fn for_each_surface_position(
        &self,
        pointer: Option<LogicalPoint<f64>>,
        mut visit: impl FnMut(&WlSurface, LogicalPoint<f64>),
    ) {
        if let (CursorImage::Surface(surface), Some(location)) = (&self.image, pointer)
            && surface.is_alive()
        {
            visit(surface, location);
        }
        for tablet in &self.tablets {
            if let CursorImage::Surface(surface) = &tablet.image
                && surface.is_alive()
            {
                visit(surface, tablet.location);
            }
        }
    }

    /// Resolve a visible pointer source to a sampled cursor image or the
    /// vector fallback without giving the renderer a Wayland resource or a
    /// second coordinate system.
    pub(in crate::protocol) fn overlays_for_output(
        &mut self,
        pointer: Option<LogicalPoint<f64>>,
        output: LogicalRect<i32>,
        scale: OutputScale,
        viewport: Rect,
        mut resolve: impl FnMut(&WlSurface, OutputScale) -> Option<CursorRaster>,
    ) -> CursorOverlays {
        self.normalize_surface_liveness();
        let mut overlays = CursorOverlays::default();
        let output = CursorOutput {
            logical: output,
            scale,
            viewport,
        };
        if !self.hidden_for_typing
            && let Some(pointer) = pointer
            && let Some(overlay) = self.overlay(0, pointer, &self.image, output, &mut resolve)
        {
            assert!(overlays.push(overlay), "pointer cursor has a reserved slot");
        }
        for tablet in &self.tablets {
            if let Some(overlay) = self.overlay(
                tablet.tool.get(),
                tablet.location,
                &tablet.image,
                output,
                &mut resolve,
            ) {
                assert!(
                    overlays.push(overlay),
                    "tablet cursor capacity matches tools"
                );
            }
        }
        overlays
    }

    pub(in crate::protocol) fn overlay_for_source_at(
        &self,
        source: u64,
        location: LogicalPoint<f64>,
        output: LogicalRect<i32>,
        scale: OutputScale,
        viewport: Rect,
        mut resolve: impl FnMut(&WlSurface, OutputScale) -> Option<CursorRaster>,
    ) -> Option<CursorOverlay> {
        let image = if source == 0 {
            if self.hidden_for_typing {
                return None;
            }
            &self.image
        } else {
            &self
                .tablets
                .iter()
                .find(|tablet| tablet.tool.get() == source)?
                .image
        };
        self.overlay(
            source,
            location,
            image,
            CursorOutput {
                logical: output,
                scale,
                viewport,
            },
            &mut resolve,
        )
    }

    pub(in crate::protocol) fn named_cursor_intersects_output(
        &self,
        pointer: Option<LogicalPoint<f64>>,
        output: LogicalRect<i32>,
        scale: OutputScale,
        viewport: Rect,
    ) -> bool {
        let output = CursorOutput {
            logical: output,
            scale,
            viewport,
        };
        if !self.hidden_for_typing
            && let (Some(pointer), CursorImage::Named(_)) = (pointer, &self.image)
            && self
                .overlay(0, pointer, &self.image, output, &mut |_, _| None)
                .is_some()
        {
            return true;
        }
        self.tablets.iter().any(|tablet| {
            matches!(tablet.image, CursorImage::Named(_))
                && self
                    .overlay(
                        tablet.tool.get(),
                        tablet.location,
                        &tablet.image,
                        output,
                        &mut |_, _| None,
                    )
                    .is_some()
        })
    }

    fn overlay(
        &self,
        source: u64,
        location: LogicalPoint<f64>,
        image: &CursorImage,
        output: CursorOutput,
        resolve: &mut impl FnMut(&WlSurface, OutputScale) -> Option<CursorRaster>,
    ) -> Option<CursorOverlay> {
        if matches!(image, CursorImage::Hidden) {
            return None;
        }
        let local_x = location.x - f64::from(output.logical.loc.x);
        let local_y = location.y - f64::from(output.logical.loc.y);
        if output.logical.size.w <= 0 || output.logical.size.h <= 0 {
            return None;
        }
        let x = output.scale.physical_coordinate_round(local_x)?;
        let y = output.scale.physical_coordinate_round(local_y)?;
        let raster = match image {
            CursorImage::Hidden => return None,
            CursorImage::Named(icon) => self
                .named_rasters
                .get(&(*icon, output.scale))
                .and_then(Option::as_ref)
                .and_then(CursorRasterSequence::current),
            CursorImage::Surface(surface) => {
                let raster = resolve(surface, output.scale)?;
                return textured_overlay(source, x, y, raster, output.viewport);
            }
        };
        if let Some(raster) = raster {
            return textured_overlay(source, x, y, raster, output.viewport);
        }
        let fallback_size = output.scale.physical_length_round(self.logical_size).max(1);
        CursorOverlay::new(
            source,
            Rect::new(x, y, fallback_size, fallback_size),
            output.viewport,
        )
    }

    pub(in crate::protocol) fn uses_surface(&self, surface: &WlSurface) -> bool {
        matches!(&self.image, CursorImage::Surface(current) if current == surface)
            || self.tablets.iter().any(
                |tablet| matches!(&tablet.image, CursorImage::Surface(current) if current == surface),
            )
    }

    pub(in crate::protocol) fn source_position_for_surface(
        &self,
        surface: &WlSurface,
        pointer: Option<LogicalPoint<f64>>,
    ) -> Option<(u64, LogicalPoint<f64>)> {
        if matches!(&self.image, CursorImage::Surface(current) if current == surface) {
            return pointer.map(|location| (0, location));
        }
        self.tablets.iter().find_map(|tablet| {
            matches!(&tablet.image, CursorImage::Surface(current) if current == surface)
                .then_some((tablet.tool.get(), tablet.location))
        })
    }

    pub(in crate::protocol) fn pointer_uses_surface(&self, surface: &WlSurface) -> bool {
        matches!(&self.image, CursorImage::Surface(current) if current == surface)
    }

    pub(in crate::protocol) fn tablet_uses_surface(
        &self,
        tool: tensor_event::TabletToolId,
        surface: &WlSurface,
    ) -> bool {
        self.tablets.iter().any(|tablet| {
            tablet.tool == tool
                && matches!(&tablet.image, CursorImage::Surface(current) if current == surface)
        })
    }

    pub(in crate::protocol) fn surface_for_source(&self, source: u64) -> Option<WlSurface> {
        let image = if source == 0 {
            &self.image
        } else {
            &self
                .tablets
                .iter()
                .find(|tablet| tablet.tool.get() == source)?
                .image
        };
        match image {
            CursorImage::Surface(surface) if surface.is_alive() => Some(surface.clone()),
            _ => None,
        }
    }

    fn normalize_surface_liveness(&mut self) {
        if matches!(&self.image, CursorImage::Surface(surface) if !surface.is_alive()) {
            self.image = CursorImage::default_named();
        }
        for tablet in &mut self.tablets {
            if matches!(&tablet.image, CursorImage::Surface(surface) if !surface.is_alive()) {
                tablet.image = CursorImage::default_named();
            }
        }
    }

    #[cfg(test)]
    pub(in crate::protocol) fn image(&self) -> &CursorImage {
        &self.image
    }
}

fn textured_overlay(
    source: u64,
    x: i32,
    y: i32,
    raster: CursorRaster,
    viewport: Rect,
) -> Option<CursorOverlay> {
    CursorOverlay::new(
        source,
        Rect::new(
            x.saturating_sub(raster.hotspot.x),
            y.saturating_sub(raster.hotspot.y),
            raster.size.width,
            raster.size.height,
        ),
        viewport,
    )
    .map(|overlay| {
        overlay.with_texture(CursorTexture {
            buffer_id: raster.buffer_id,
            sample_transform: raster.sample_transform,
        })
    })
}

#[cfg(test)]
mod tests;
