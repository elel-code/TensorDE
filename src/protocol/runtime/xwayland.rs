#[cfg(feature = "xwayland")]
use std::{ffi::OsString, process::Stdio};

#[cfg(feature = "xwayland")]
use smithay::xwayland::{X11Wm, XWayland, XWaylandClientData, XWaylandEvent};
#[cfg(feature = "xwayland")]
use tracing::{info, warn};

#[cfg(feature = "xwayland")]
use super::{ProtocolError, WaylandRuntime};

#[cfg(feature = "xwayland")]
impl WaylandRuntime {
    pub(super) fn start_xwayland(&mut self) -> Result<(), ProtocolError> {
        if self.xwayland_client.is_some() {
            return Ok(());
        }

        let display = self
            .display
            .as_ref()
            .ok_or(ProtocolError::DisplayConsumed)?;
        let display_handle = display.handle();
        let loop_handle = self.event_loop.handle();
        let (xwayland, client) = XWayland::spawn(
            &display_handle,
            None,
            std::iter::empty::<(&str, &str)>(),
            std::iter::empty::<&str>(),
            true,
            Stdio::null(),
            Stdio::null(),
            |_| {},
        )
        .map_err(ProtocolError::XWayland)?;
        let display_number = xwayland.display_number();
        let xwm_client = client.clone();
        self.event_loop
            .handle()
            .insert_source(xwayland, move |event, _, state| match event {
                XWaylandEvent::Ready {
                    x11_socket,
                    display_number,
                } => {
                    let Some(client_data) = xwm_client.get_data::<XWaylandClientData>() else {
                        warn!(
                            display_number,
                            "XWayland client lost its Smithay client data"
                        );
                        return;
                    };

                    // wl_output and fractional-scale state drive XWayland buffers. This must
                    // remain one so X11 cannot acquire a second coordinate system.
                    client_data.compositor_state.set_client_scale(1.0);
                    match X11Wm::start_wm(
                        loop_handle.clone(),
                        &display_handle,
                        x11_socket,
                        xwm_client.clone(),
                    ) {
                        Ok(xwm) => {
                            state.install_xwm(xwm);
                            info!(display_number, "XWayland rootless XWM is ready");
                        }
                        Err(error) => {
                            warn!(%error, display_number, "failed to attach rootless XWayland XWM");
                        }
                    }
                }
                XWaylandEvent::Error => warn!("XWayland exited before becoming ready"),
            })
            .map_err(|error| ProtocolError::XWaylandSource(error.to_string()))?;
        self.xwayland_display = Some(OsString::from(format!(":{display_number}")));
        self.xwayland_client = Some(client);
        Ok(())
    }
}

#[cfg(not(feature = "xwayland"))]
impl super::WaylandRuntime {
    pub(super) fn start_xwayland(&mut self) -> Result<(), super::ProtocolError> {
        Err(super::ProtocolError::XWaylandDisabled)
    }
}
