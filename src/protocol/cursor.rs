use std::{
    collections::HashMap,
    io,
    os::fd::OwnedFd,
    time::{Duration, Instant},
};

use cursor_icon::CursorIcon;
use rustix::time::{
    Itimerspec, TimerfdClockId, TimerfdFlags, TimerfdTimerFlags, Timespec, timerfd_create,
    timerfd_settime,
};
use tensor_protocol::SurfaceSampleTransform;
use tensor_util::{LogicalPoint, LogicalRect, OutputScale, Point, Rect, Size};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use crate::{
    ecs::SurfaceBufferId,
    render::{CursorOverlay, CursorOverlays, CursorTexture},
};

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

fn create_animation_timer() -> Option<OwnedFd> {
    match timerfd_create(
        TimerfdClockId::Monotonic,
        TimerfdFlags::CLOEXEC | TimerfdFlags::NONBLOCK,
    ) {
        Ok(timer) => Some(timer),
        Err(error) => {
            tracing::warn!(%error, "cursor animation timerfd is unavailable");
            None
        }
    }
}

fn duration_timespec(duration: Duration) -> Timespec {
    Timespec {
        tv_sec: i64::try_from(duration.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: i64::from(duration.subsec_nanos()),
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
            animation_timer: create_animation_timer(),
            animation_deadline: None,
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

    fn select_named_frame(&mut self, icon: CursorIcon, scale: OutputScale, now: Instant) {
        let Some(Some(sequence)) = self.named_rasters.get_mut(&(icon, scale)) else {
            return;
        };
        let Some((current, remaining)) =
            sequence.frame_at(now.duration_since(self.animation_epoch))
        else {
            return;
        };
        sequence.current = current;
        self.arm_animation_timer(now, remaining);
    }

    pub(crate) fn duplicate_animation_timer_fd(&self) -> io::Result<Option<OwnedFd>> {
        self.animation_timer
            .as_ref()
            .map(|timer| rustix::io::fcntl_dupfd_cloexec(timer, 0).map_err(io::Error::from))
            .transpose()
    }

    pub(crate) fn complete_animation_timer(&mut self) -> io::Result<bool> {
        let Some(timer) = &self.animation_timer else {
            return Ok(false);
        };
        let mut expirations = [0_u8; 8];
        let read = rustix::io::read(timer, &mut expirations).map_err(io::Error::from)?;
        if read != expirations.len() {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        self.animation_deadline = None;
        Ok(true)
    }

    pub(crate) fn animation_timer_failed(&mut self) {
        self.animation_timer = None;
        self.animation_deadline = None;
    }

    fn arm_animation_timer(&mut self, now: Instant, delay: Duration) {
        let Some(timer) = &self.animation_timer else {
            return;
        };
        let deadline = now.checked_add(delay).unwrap_or(now);
        if self
            .animation_deadline
            .is_some_and(|current| current <= deadline)
        {
            return;
        }
        let delay = deadline
            .saturating_duration_since(Instant::now())
            .max(Duration::from_nanos(1));
        let value = duration_timespec(delay);
        if let Err(error) = timerfd_settime(
            timer,
            TimerfdTimerFlags::empty(),
            &Itimerspec {
                it_interval: Timespec::default(),
                it_value: value,
            },
        ) {
            tracing::warn!(%error, "cursor animation timerfd could not be armed");
            self.animation_timer = None;
            self.animation_deadline = None;
            return;
        }
        self.animation_deadline = Some(deadline);
    }

    fn disarm_animation_timer(&mut self) {
        let Some(timer) = &self.animation_timer else {
            return;
        };
        if let Err(error) = timerfd_settime(
            timer,
            TimerfdTimerFlags::empty(),
            &Itimerspec {
                it_interval: Timespec::default(),
                it_value: Timespec::default(),
            },
        ) {
            tracing::warn!(%error, "cursor animation timerfd could not be disarmed");
            self.animation_timer = None;
        }
        self.animation_deadline = None;
    }

    pub(in crate::protocol) fn set_image(&mut self, image: CursorImage) -> bool {
        if self.image == image {
            return false;
        }
        self.image = image;
        true
    }

    /// Hide the software cursor after a keyboard press when configured.
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
        tablet.image = image;
        true
    }

    pub(in crate::protocol) fn clear_tablet(&mut self, tool: tensor_event::TabletToolId) -> bool {
        let Some(index) = self.tablets.iter().position(|tablet| tablet.tool == tool) else {
            return false;
        };
        self.tablets.remove(index);
        true
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
        if output.logical.size.w <= 0
            || output.logical.size.h <= 0
            || local_x < 0.0
            || local_y < 0.0
            || local_x >= f64::from(output.logical.size.w)
            || local_y >= f64::from(output.logical.size.h)
        {
            return None;
        }
        let x = output.scale.physical_coordinate_round(local_x)?;
        let y = output.scale.physical_coordinate_round(local_y)?;
        let raster = match image {
            CursorImage::Hidden => None,
            CursorImage::Named(icon) => self
                .named_rasters
                .get(&(*icon, output.scale))
                .and_then(Option::as_ref)
                .and_then(CursorRasterSequence::current),
            CursorImage::Surface(surface) => resolve(surface, output.scale),
        };
        if let Some(raster) = raster {
            return CursorOverlay::new(
                source,
                Rect::new(
                    x.saturating_sub(raster.hotspot.x),
                    y.saturating_sub(raster.hotspot.y),
                    raster.size.width,
                    raster.size.height,
                ),
                output.viewport,
            )
            .map(|overlay| {
                overlay.with_texture(CursorTexture {
                    buffer_id: raster.buffer_id,
                    sample_transform: raster.sample_transform,
                })
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> LogicalRect<i32> {
        LogicalRect::new((10, 20).into(), (100, 80).into())
    }

    #[test]
    fn visible_cursor_uses_output_local_physical_coordinates() {
        let mut cursor = CursorState::default();
        let overlays = cursor.overlays_for_output(
            Some((20.4, 30.4).into()),
            output(),
            OutputScale::from_f64(1.25).unwrap(),
            Rect::new(0, 0, 125, 100),
            |_, _| None,
        );
        let overlay = overlays.as_slice()[0];

        assert_eq!(overlay.destination, Rect::new(13, 13, 30, 30));
        assert_eq!(overlay.clip, overlay.destination);
    }

    #[test]
    fn hidden_and_off_output_cursors_do_not_create_an_overlay() {
        let mut cursor = CursorState::default();
        assert!(!cursor.set_image(CursorImage::default_named()));
        assert!(cursor.set_image(CursorImage::Hidden));
        assert_eq!(
            cursor
                .overlays_for_output(
                    Some((20.0, 30.0).into()),
                    output(),
                    OutputScale::ONE,
                    Rect::new(0, 0, 100, 80),
                    |_, _| None,
                )
                .as_slice(),
            []
        );
        cursor.set_image(CursorImage::default_named());
        assert_eq!(
            cursor
                .overlays_for_output(
                    Some((110.0, 30.0).into()),
                    output(),
                    OutputScale::ONE,
                    Rect::new(0, 0, 100, 80),
                    |_, _| None,
                )
                .as_slice(),
            []
        );
    }

    #[test]
    fn pointer_and_tablet_cursors_remain_independent() {
        let mut cursor = CursorState::default();
        assert!(
            cursor.note_tablet_activity(tensor_event::TabletToolId::new(1), (40.0, 50.0).into())
        );
        let overlays = cursor.overlays_for_output(
            Some((20.0, 30.0).into()),
            output(),
            OutputScale::ONE,
            Rect::new(0, 0, 100, 80),
            |_, _| None,
        );
        assert_eq!(overlays.as_slice().len(), 2);
        assert_eq!(
            overlays.as_slice()[0].destination,
            Rect::new(10, 10, 24, 24)
        );
        assert_eq!(
            overlays.as_slice()[1].destination,
            Rect::new(30, 30, 24, 24)
        );
        assert!(cursor.clear_tablet(tensor_event::TabletToolId::new(1)));
        assert_eq!(
            cursor
                .overlays_for_output(
                    Some((20.0, 30.0).into()),
                    output(),
                    OutputScale::ONE,
                    Rect::new(0, 0, 100, 80),
                    |_, _| None,
                )
                .as_slice()
                .len(),
            1
        );
    }

    #[test]
    fn named_raster_uses_physical_hotspot_and_sample_transform() {
        let mut cursor = CursorState::default();
        let scale = OutputScale::from_f64(1.25).unwrap();
        let transform = SurfaceSampleTransform::new((0.25, 0.5), (0.5, 0.0), (0.0, 0.25));
        cursor.named_rasters.insert(
            (CursorIcon::Default, scale),
            Some(CursorRasterSequence {
                frames: vec![CursorRasterFrame {
                    raster: CursorRaster {
                        buffer_id: SurfaceBufferId::new(7),
                        size: Size::new(32, 40),
                        hotspot: Point::new(3, 4),
                        sample_transform: transform,
                    },
                    delay_ms: 0,
                }],
                duration_ms: 0,
                current: 0,
            }),
        );

        let overlays = cursor.overlays_for_output(
            Some((20.0, 30.0).into()),
            output(),
            scale,
            Rect::new(0, 0, 125, 100),
            |_, _| None,
        );
        let overlay = overlays.as_slice()[0];

        assert_eq!(overlay.destination, Rect::new(10, 9, 32, 40));
        assert_eq!(
            overlay.texture,
            Some(CursorTexture {
                buffer_id: SurfaceBufferId::new(7),
                sample_transform: transform,
            })
        );
    }

    #[test]
    fn animated_raster_selects_frames_and_exact_next_deadlines() {
        let frame = |buffer_id, delay_ms| CursorRasterFrame {
            raster: CursorRaster {
                buffer_id: SurfaceBufferId::new(buffer_id),
                size: Size::new(24, 24),
                hotspot: Point::new(1, 2),
                sample_transform: SurfaceSampleTransform::IDENTITY,
            },
            delay_ms,
        };
        let sequence = CursorRasterSequence {
            frames: vec![frame(1, 10), frame(2, 20), frame(3, 30)],
            duration_ms: 60,
            current: 0,
        };

        assert_eq!(
            sequence.frame_at(Duration::from_millis(0)),
            Some((0, Duration::from_millis(10)))
        );
        assert_eq!(
            sequence.frame_at(Duration::from_millis(10)),
            Some((1, Duration::from_millis(20)))
        );
        assert_eq!(
            sequence.frame_at(Duration::from_millis(59)),
            Some((2, Duration::from_millis(1)))
        );
        assert_eq!(
            sequence.frame_at(Duration::from_millis(60)),
            Some((0, Duration::from_millis(10)))
        );
    }

    #[test]
    fn theme_change_releases_every_uploaded_animation_frame() {
        let raster = |buffer_id| CursorRasterFrame {
            raster: CursorRaster {
                buffer_id: SurfaceBufferId::new(buffer_id),
                size: Size::new(24, 24),
                hotspot: Point::new(0, 0),
                sample_transform: SurfaceSampleTransform::IDENTITY,
            },
            delay_ms: 10,
        };
        let mut cursor = CursorState::default();
        cursor.named_rasters.insert(
            (CursorIcon::Default, OutputScale::ONE),
            Some(CursorRasterSequence {
                frames: vec![raster(3), raster(4)],
                duration_ms: 20,
                current: 0,
            }),
        );

        let released = cursor.configure("another-theme".to_owned(), 24, false);

        assert_eq!(released, [SurfaceBufferId::new(3), SurfaceBufferId::new(4)]);
        assert!(cursor.named_rasters.is_empty());
    }
}
