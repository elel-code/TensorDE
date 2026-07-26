use std::sync::{Arc, Mutex};

use smithay_client_toolkit::dispatch2::Dispatch2;
use wayland_client::globals::{BindError, GlobalList};
use wayland_client::protocol::wl_shm;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
use wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_manager_v1::{
    Event as ManagerEvent, XdgToplevelIconManagerV1,
};
use wayland_protocols::xdg::toplevel_icon::v1::client::xdg_toplevel_icon_v1::XdgToplevelIconV1;
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::Shm;

use crate::shm_format::copy_rgba_to_premultiplied_argb8888;

const MAX_SHM_ICON_EDGE: u32 = i32::MAX as u32 / 4;

/// One square RGBA pixel representation for a toplevel icon.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToplevelIconBuffer {
    rgba: Arc<[u8]>,
    width: u32,
    height: u32,
    scale: i32,
}

impl ToplevelIconBuffer {
    pub fn new(
        rgba: impl Into<Arc<[u8]>>,
        width: u32,
        height: u32,
        scale: i32,
    ) -> Result<Self, ToplevelIconError> {
        if width == 0 || height == 0 {
            return Err(ToplevelIconError::EmptyBuffer);
        }
        if width != height {
            return Err(ToplevelIconError::NonSquareBuffer);
        }
        // wl_shm width, height, and stride are signed 32-bit values. Each
        // ARGB8888 row consumes four bytes per pixel.
        if width > MAX_SHM_ICON_EDGE {
            return Err(ToplevelIconError::DimensionsTooLarge);
        }
        if scale < 1 {
            return Err(ToplevelIconError::InvalidScale);
        }
        if !width.is_multiple_of(scale as u32) {
            return Err(ToplevelIconError::IndivisibleScale);
        }
        let rgba = rgba.into();
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(ToplevelIconError::ByteLengthOverflow)?;
        if rgba.len() != expected {
            return Err(ToplevelIconError::ByteLengthMismatch);
        }
        Ok(Self {
            rgba,
            width,
            height,
            scale,
        })
    }

    pub fn rgba(&self) -> &Arc<[u8]> {
        &self.rgba
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn scale(&self) -> i32 {
        self.scale
    }

    pub const fn logical_size(&self) -> u32 {
        self.width / self.scale as u32
    }
}

/// A named and/or pixel-backed icon for an individual xdg-toplevel.
///
/// Providing both forms lets the compositor prefer its current XDG icon theme
/// while retaining pixel buffers as a fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToplevelIcon {
    name: Option<String>,
    buffers: Vec<ToplevelIconBuffer>,
}

impl ToplevelIcon {
    pub fn new(
        name: Option<String>,
        buffers: Vec<ToplevelIconBuffer>,
    ) -> Result<Self, ToplevelIconError> {
        if name.as_ref().is_some_and(String::is_empty) {
            return Err(ToplevelIconError::EmptyName);
        }
        if name.as_ref().is_some_and(|name| name.contains('\0')) {
            return Err(ToplevelIconError::NameContainsNul);
        }
        if name.is_none() && buffers.is_empty() {
            return Err(ToplevelIconError::EmptyIcon);
        }
        Ok(Self { name, buffers })
    }

    pub fn from_name(name: impl Into<String>) -> Result<Self, ToplevelIconError> {
        Self::new(Some(name.into()), Vec::new())
    }

    pub fn from_rgba(
        rgba: impl Into<Arc<[u8]>>,
        width: u32,
        height: u32,
        scale: i32,
    ) -> Result<Self, ToplevelIconError> {
        Self::new(
            None,
            vec![ToplevelIconBuffer::new(rgba, width, height, scale)?],
        )
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn buffers(&self) -> &[ToplevelIconBuffer] {
        &self.buffers
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ToplevelIconError {
    #[error("a toplevel icon must have a name or at least one pixel buffer")]
    EmptyIcon,
    #[error("toplevel icon names must not be empty")]
    EmptyName,
    #[error("toplevel icon names must not contain NUL bytes")]
    NameContainsNul,
    #[error("toplevel icon buffer dimensions must be non-zero")]
    EmptyBuffer,
    #[error("toplevel icon buffers must be square")]
    NonSquareBuffer,
    #[error("toplevel icon buffer dimensions exceed Wayland SHM limits")]
    DimensionsTooLarge,
    #[error("toplevel icon buffer scale must be at least one")]
    InvalidScale,
    #[error("toplevel icon buffer dimensions must be divisible by its scale")]
    IndivisibleScale,
    #[error("toplevel icon RGBA byte length overflow")]
    ByteLengthOverflow,
    #[error("toplevel icon RGBA byte length does not match its dimensions")]
    ByteLengthMismatch,
}

#[derive(Debug, Default)]
struct ManagerMetadata {
    pending_sizes: Vec<u32>,
    preferred_sizes: Vec<u32>,
    preferences_ready: bool,
}

impl ManagerMetadata {
    fn push_size(&mut self, size: i32) {
        if size > 0 {
            self.pending_sizes.push(size as u32);
        }
    }

    fn finish(&mut self) {
        self.pending_sizes.sort_unstable();
        self.pending_sizes.dedup();
        self.preferred_sizes = std::mem::take(&mut self.pending_sizes);
        self.preferences_ready = true;
    }

    fn preferred_sizes(&self) -> &[u32] {
        if self.preferences_ready {
            &self.preferred_sizes
        } else {
            &[]
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ManagerData(Arc<Mutex<ManagerMetadata>>);

impl<D> Dispatch2<XdgToplevelIconManagerV1, D> for ManagerData {
    fn event(
        &self,
        _: &mut D,
        _: &XdgToplevelIconManagerV1,
        event: ManagerEvent,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        let mut metadata = self.0.lock().expect("toplevel icon metadata poisoned");
        match event {
            ManagerEvent::IconSize { size } => metadata.push_size(size),
            ManagerEvent::Done => metadata.finish(),
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct IconData;

impl<D> Dispatch2<XdgToplevelIconV1, D> for IconData {
    fn event(
        &self,
        _: &mut D,
        _: &XdgToplevelIconV1,
        _: <XdgToplevelIconV1 as Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<D>,
    ) {
        unreachable!("xdg_toplevel_icon_v1 has no events");
    }
}

#[derive(Debug)]
pub(crate) struct ToplevelIconManager {
    manager: XdgToplevelIconManagerV1,
    metadata: Arc<Mutex<ManagerMetadata>>,
}

impl ToplevelIconManager {
    pub(crate) fn bind<D>(
        globals: &GlobalList,
        queue_handle: &QueueHandle<D>,
    ) -> Result<Self, BindError>
    where
        D: Dispatch<XdgToplevelIconManagerV1, ManagerData> + 'static,
    {
        let metadata = Arc::new(Mutex::new(ManagerMetadata::default()));
        let manager = globals.bind(queue_handle, 1..=1, ManagerData(Arc::clone(&metadata)))?;
        Ok(Self { manager, metadata })
    }

    pub(crate) fn preferred_sizes(&self) -> Vec<u32> {
        let metadata = self
            .metadata
            .lock()
            .expect("toplevel icon metadata poisoned");
        metadata.preferred_sizes().to_vec()
    }

    pub(crate) fn set_icon<D>(
        &self,
        queue_handle: &QueueHandle<D>,
        shm: &Shm,
        toplevel: &XdgToplevel,
        icon: Option<ToplevelIcon>,
    ) -> Result<Option<AppliedToplevelIcon>, String>
    where
        D: Dispatch<XdgToplevelIconV1, IconData> + 'static,
    {
        let Some(icon) = icon else {
            self.manager.set_icon(toplevel, None);
            return Ok(None);
        };

        let total_bytes = icon
            .buffers
            .iter()
            .try_fold(0usize, |total, buffer| total.checked_add(buffer.rgba.len()));
        let mut buffers = Vec::with_capacity(icon.buffers.len());
        if let Some(total_bytes) = total_bytes.filter(|total| *total > 0) {
            let mut pool = SlotPool::new(total_bytes, shm).map_err(|error| error.to_string())?;
            for source in &icon.buffers {
                let stride = (source.width as i32)
                    .checked_mul(4)
                    .ok_or_else(|| "toplevel icon stride overflow".to_string())?;
                let (buffer, canvas) = pool
                    .create_buffer(
                        source.width as i32,
                        source.height as i32,
                        stride,
                        wl_shm::Format::Argb8888,
                    )
                    .map_err(|error| error.to_string())?;
                copy_rgba_to_premultiplied_argb8888(&source.rgba, canvas);
                buffers.push((buffer, source.scale));
            }
        } else if total_bytes.is_none() {
            return Err("toplevel icon SHM allocation size overflow".to_string());
        }

        let protocol_icon = self.manager.create_icon(queue_handle, IconData);
        if let Some(name) = icon.name {
            protocol_icon.set_name(name);
        }
        for (buffer, scale) in &buffers {
            protocol_icon.add_buffer(buffer.wl_buffer(), *scale);
        }
        self.manager.set_icon(toplevel, Some(&protocol_icon));
        protocol_icon.destroy();

        Ok(Some(AppliedToplevelIcon {
            _buffers: buffers.into_iter().map(|(buffer, _)| buffer).collect(),
        }))
    }
}

impl Drop for ToplevelIconManager {
    fn drop(&mut self) {
        if self.manager.is_alive() {
            self.manager.destroy();
        }
    }
}

#[derive(Debug)]
pub(crate) struct AppliedToplevelIcon {
    // Retaining these is conservative beyond xdg_toplevel_icon_v1.destroy and
    // also accommodates compositors that consume the icon asynchronously.
    _buffers: Vec<Buffer>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_buffers_enforce_protocol_shape_scale_and_length() {
        assert!(ToplevelIconBuffer::new(vec![0; 16], 2, 2, 1).is_ok());
        assert_eq!(
            ToplevelIconBuffer::new(vec![0; 24], 3, 2, 1),
            Err(ToplevelIconError::NonSquareBuffer)
        );
        assert_eq!(
            ToplevelIconBuffer::new(vec![0; 36], 3, 3, 2),
            Err(ToplevelIconError::IndivisibleScale)
        );
        assert_eq!(
            ToplevelIconBuffer::new(vec![0; 15], 2, 2, 1),
            Err(ToplevelIconError::ByteLengthMismatch)
        );
        assert_eq!(
            ToplevelIconBuffer::new(Vec::new(), MAX_SHM_ICON_EDGE + 1, MAX_SHM_ICON_EDGE + 1, 1),
            Err(ToplevelIconError::DimensionsTooLarge)
        );
    }

    #[test]
    fn icon_supports_theme_name_with_pixel_fallbacks() {
        let buffer = ToplevelIconBuffer::new(vec![255; 16], 2, 2, 1).unwrap();
        let icon = ToplevelIcon::new(Some("fika".to_string()), vec![buffer]).unwrap();
        assert_eq!(icon.name(), Some("fika"));
        assert_eq!(icon.buffers().len(), 1);
        assert_eq!(icon.buffers()[0].logical_size(), 2);
    }

    #[test]
    fn empty_and_wire_invalid_names_are_rejected() {
        assert_eq!(
            ToplevelIcon::new(None, Vec::new()),
            Err(ToplevelIconError::EmptyIcon)
        );
        assert_eq!(
            ToplevelIcon::from_name(""),
            Err(ToplevelIconError::EmptyName)
        );
        assert_eq!(
            ToplevelIcon::from_name("bad\0name"),
            Err(ToplevelIconError::NameContainsNul)
        );
    }

    #[test]
    fn preferred_sizes_are_sorted_and_deduplicated_on_done() {
        let mut metadata = ManagerMetadata::default();
        for size in [64, 32, 0, -1, 64, 128] {
            metadata.push_size(size);
        }

        assert!(metadata.preferred_sizes().is_empty());
        metadata.finish();

        assert_eq!(metadata.preferred_sizes(), [32, 64, 128]);
        assert!(metadata.preferences_ready);
    }
}
