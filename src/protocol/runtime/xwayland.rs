#[cfg(feature = "xwayland")]
use std::ffi::OsString;

#[cfg(feature = "xwayland")]
use tensor_runtime::{OpaqueFdCompletion, OpaqueFdCompletionRuntime, WorkerRx, WorkerTx};
#[cfg(feature = "xwayland")]
use tracing::{info, warn};

#[cfg(feature = "xwayland")]
use super::{ProtocolError, WaylandRuntime};
#[cfg(feature = "xwayland")]
use crate::protocol::state::RuntimeState;
#[cfg(feature = "xwayland")]
use crate::protocol::xwayland::XWayland;

#[cfg(feature = "xwayland")]
pub(crate) const MAX_PENDING_XWAYLAND_STARTUP_EVENTS: usize = 1;
#[cfg(feature = "xwayland")]
pub(crate) const MAX_PENDING_XWAYLAND_STARTUP_CONTROL_EVENTS: usize = 1;

#[cfg(feature = "xwayland")]
pub(crate) type XWaylandStartupEvent = OpaqueFdCompletion;
#[cfg(feature = "xwayland")]
pub(crate) type XWaylandStartupControlEvent = String;

#[cfg(feature = "xwayland")]
pub(super) struct XWaylandCompletionChannels {
    events: WorkerTx<XWaylandStartupEvent>,
    control: WorkerTx<XWaylandStartupControlEvent>,
}

#[cfg(feature = "xwayland")]
pub(crate) fn drain_xwayland_startup_events(
    events: &WorkerRx<XWaylandStartupEvent>,
    control: &WorkerRx<XWaylandStartupControlEvent>,
    state: &mut RuntimeState,
) {
    while let Some(completion) = events.try_recv() {
        if state.xwm.is_some() {
            match state.drain_xwm_events() {
                Ok(()) => {
                    if let Err(error) = completion.rearm() {
                        warn!(?error, "failed to rearm the X11 socket completion");
                    }
                }
                Err(error) => {
                    let _ = completion.finish();
                    state.xwm = None;
                    warn!(%error, "X11 window manager completion failed");
                }
            }
            continue;
        }
        match state.complete_xwayland_startup() {
            Ok(Some(display_number)) => {
                if let Err(error) = completion.finish() {
                    warn!(
                        ?error,
                        "failed to finish the XWayland startup completion service"
                    );
                }
                info!(display_number, "XWayland rootless XWM is ready");
            }
            Ok(None) => {
                if let Err(error) = completion.rearm() {
                    warn!(?error, "failed to rearm the XWayland startup completion");
                }
            }
            Err(error) => {
                let _ = completion.finish();
                warn!(%error, "failed to attach rootless XWayland XWM");
            }
        }
    }
    while let Some(error) = control.try_recv() {
        warn!(%error, "XWayland startup completion runtime failed");
    }
}

#[cfg(feature = "xwayland")]
impl WaylandRuntime {
    pub(crate) fn install_xwayland_completion_channels(
        &mut self,
        events: WorkerTx<XWaylandStartupEvent>,
        control: WorkerTx<XWaylandStartupControlEvent>,
    ) {
        if self.xwayland_completion_channels.is_none() {
            self.xwayland_completion_channels =
                Some(XWaylandCompletionChannels { events, control });
        }
    }

    pub(crate) fn take_xwayland_completion_runtime(&mut self) -> Option<OpaqueFdCompletionRuntime> {
        self.xwayland_completion_runtime.take()
    }

    pub(super) fn start_xwayland(&mut self) -> Result<(), ProtocolError> {
        if self.state.has_xwayland_process() {
            return Ok(());
        }

        let channels = self
            .xwayland_completion_channels
            .as_ref()
            .ok_or(ProtocolError::XWaylandCompletionRuntimeMissing)?;
        let display_handle = self.state.display_handle.clone();
        let (xwayland, client) =
            XWayland::spawn(&display_handle).map_err(ProtocolError::XWayland)?;
        let display_number = xwayland.display_number();
        let completion_runtime = OpaqueFdCompletionRuntime::start(
            "tensor-xwayland-startup-completions",
            xwayland.completion_fd(),
            channels.events.clone(),
            channels.control.clone(),
        )
        .map_err(|error| ProtocolError::XWaylandCompletion(error.to_string()))?;
        self.state.install_xwayland_process(
            xwayland,
            client,
            channels.events.clone(),
            channels.control.clone(),
        );
        self.xwayland_completion_runtime = Some(completion_runtime);
        self.xwayland_display = Some(OsString::from(format!(":{display_number}")));
        Ok(())
    }
}

#[cfg(not(feature = "xwayland"))]
impl super::WaylandRuntime {
    pub(super) fn start_xwayland(&mut self) -> Result<(), super::ProtocolError> {
        Err(super::ProtocolError::XWaylandDisabled)
    }
}

#[cfg(all(test, feature = "xwayland"))]
mod tests {
    use std::time::Duration;

    use crate::{
        layout::{LayoutEngine, LayoutKind},
        scene::SceneAppearance,
    };

    use super::WaylandRuntime;

    #[test]
    #[ignore = "requires an installed Xwayland executable"]
    fn startup_displayfd_completion_installs_the_xwm() {
        let mut runtime = WaylandRuntime::with_appearance(
            LayoutEngine::new(LayoutKind::Scrolling1D),
            SceneAppearance::default(),
        )
        .unwrap();
        let _relay = runtime.prepare_for_test(true).unwrap();

        for _ in 0..400 {
            runtime
                .event_loop
                .dispatch(Duration::from_millis(5), &mut runtime.state)
                .unwrap();
            if runtime.state.xwm.is_some() {
                return;
            }
        }
        panic!("XWayland did not complete its displayfd startup handshake");
    }
}
