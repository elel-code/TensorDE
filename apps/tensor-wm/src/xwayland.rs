use std::{env, ffi::OsStr};

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XWaylandConfig {
    enabled: bool,
}

impl XWaylandConfig {
    pub const fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub const fn enabled(self) -> bool {
        self.enabled
    }

    pub fn from_environment(value: Option<&str>) -> Result<Self, XWaylandConfigError> {
        let value = value.ok_or(XWaylandConfigError::NonUnicode)?;
        match value {
            "1" | "true" | "on" => Ok(Self::new(true)),
            "0" | "false" | "off" => Ok(Self::new(false)),
            _ => Err(XWaylandConfigError::InvalidValue(value.to_owned())),
        }
    }
}

impl Default for XWaylandConfig {
    fn default() -> Self {
        Self::new(true)
    }
}

pub fn reject_x11_session() -> Result<(), XWaylandError> {
    if is_x11_only(
        env::var_os("DISPLAY").as_deref(),
        env::var_os("WAYLAND_DISPLAY").as_deref(),
    ) {
        return Err(XWaylandError::X11SessionDetected);
    }
    Ok(())
}

fn is_x11_only(display: Option<&OsStr>, wayland_display: Option<&OsStr>) -> bool {
    display.is_some() && wayland_display.is_none()
}

#[derive(Debug, Error)]
pub enum XWaylandConfigError {
    #[error("TENSOR_XWAYLAND is not valid Unicode")]
    NonUnicode,
    #[error("invalid XWayland value '{0}'; expected true, false, on, or off")]
    InvalidValue(String),
}

#[derive(Debug, Error)]
pub enum XWaylandError {
    #[error("Tensor only starts as a Wayland session; refusing an inherited X11 session")]
    X11SessionDetected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_environment_switches() {
        assert!(
            XWaylandConfig::from_environment(Some("true"))
                .unwrap()
                .enabled()
        );
        assert!(
            !XWaylandConfig::from_environment(Some("off"))
                .unwrap()
                .enabled()
        );
    }

    #[test]
    fn distinguishes_xwayland_from_an_x11_session() {
        assert!(is_x11_only(Some(OsStr::new(":0")), None));
        assert!(!is_x11_only(
            Some(OsStr::new(":0")),
            Some(OsStr::new("wayland-1"))
        ));
        assert!(!is_x11_only(None, None));
    }
}
