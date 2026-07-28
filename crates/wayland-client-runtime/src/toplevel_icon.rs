//! xdg-toplevel-icon public types. Native path uses these types with shell SHM helpers.

use std::sync::Arc;

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
