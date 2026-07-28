#[cfg(feature = "xwayland")]
use std::ffi::OsString;

#[cfg(feature = "xwayland")]
use tensor_runtime::{OpaqueFdCompletion, OpaqueFdCompletionRuntime, WorkerRx, WorkerTx};
#[cfg(feature = "xwayland")]
use tracing::info;

#[cfg(feature = "xwayland")]
use super::{ProtocolError, WaylandRuntime};
#[cfg(feature = "xwayland")]
use crate::protocol::state::RuntimeState;
#[cfg(feature = "xwayland")]
use crate::protocol::xwayland::{X11PropertyResult, XWayland};

#[cfg(feature = "xwayland")]
pub(crate) const MAX_PENDING_XWAYLAND_STARTUP_EVENTS: usize = 1;
#[cfg(feature = "xwayland")]
pub(crate) const MAX_PENDING_XWAYLAND_STARTUP_CONTROL_EVENTS: usize = 1;
#[cfg(feature = "xwayland")]
pub(crate) const MAX_PENDING_XWAYLAND_PROPERTY_RESULTS: usize = 64;
#[cfg(feature = "xwayland")]
pub(crate) const MAX_PENDING_XWAYLAND_PROPERTY_CONTROL_EVENTS: usize = 1;

#[cfg(feature = "xwayland")]
pub(crate) type XWaylandStartupEvent = OpaqueFdCompletion;
#[cfg(feature = "xwayland")]
pub(crate) type XWaylandStartupControlEvent = String;
#[cfg(feature = "xwayland")]
pub(crate) type XWaylandPropertyEvent = X11PropertyResult;
#[cfg(feature = "xwayland")]
pub(crate) type XWaylandPropertyControlEvent = String;

#[cfg(feature = "xwayland")]
pub(super) struct XWaylandCompletionChannels {
    events: WorkerTx<XWaylandStartupEvent>,
    control: WorkerTx<XWaylandStartupControlEvent>,
    property_events: WorkerTx<XWaylandPropertyEvent>,
    property_control: WorkerTx<XWaylandPropertyControlEvent>,
}

#[cfg(feature = "xwayland")]
pub(crate) fn drain_xwayland_events(
    events: &WorkerRx<XWaylandStartupEvent>,
    control: &WorkerRx<XWaylandStartupControlEvent>,
    property_events: &WorkerRx<XWaylandPropertyEvent>,
    property_control: &WorkerRx<XWaylandPropertyControlEvent>,
    state: &mut RuntimeState,
) -> Result<(), String> {
    while let Some(completion) = events.try_recv() {
        if state.xwm.is_some() {
            match state.drain_xwm_events() {
                Ok(()) => {
                    if let Err(error) = completion.rearm() {
                        state.stop_xwayland();
                        return Err(format!("failed to rearm X11 socket completion: {error:?}"));
                    }
                }
                Err(error) => {
                    let _ = completion.finish();
                    state.stop_xwayland();
                    return Err(format!("X11 window manager completion failed: {error}"));
                }
            }
            continue;
        }
        match state.complete_xwayland_startup() {
            Ok(Some(display_number)) => {
                if let Err(error) = completion.finish() {
                    state.stop_xwayland();
                    return Err(format!(
                        "failed to finish XWayland startup completion: {error:?}"
                    ));
                }
                info!(display_number, "XWayland rootless XWM is ready");
            }
            Ok(None) => {
                if let Err(error) = completion.rearm() {
                    state.stop_xwayland();
                    return Err(format!(
                        "failed to rearm XWayland startup completion: {error:?}"
                    ));
                }
            }
            Err(error) => {
                let _ = completion.finish();
                state.stop_xwayland();
                return Err(format!("failed to attach rootless XWayland XWM: {error}"));
            }
        }
    }
    if let Some(error) = control.try_recv() {
        state.stop_xwayland();
        return Err(format!(
            "XWayland startup completion runtime failed: {error}"
        ));
    }
    while let Some(result) = property_events.try_recv() {
        if let Err(error) = state.apply_x11_property_result(result) {
            state.stop_xwayland();
            return Err(format!("X11 property completion failed: {error}"));
        }
    }
    if let Some(error) = property_control.try_recv() {
        state.stop_xwayland();
        return Err(format!("X11 property completion runtime failed: {error}"));
    }
    Ok(())
}

#[cfg(feature = "xwayland")]
impl WaylandRuntime {
    pub(crate) fn install_xwayland_completion_channels(
        &mut self,
        events: WorkerTx<XWaylandStartupEvent>,
        control: WorkerTx<XWaylandStartupControlEvent>,
        property_events: WorkerTx<XWaylandPropertyEvent>,
        property_control: WorkerTx<XWaylandPropertyControlEvent>,
    ) -> Result<(), ProtocolError> {
        if self.xwayland_completion_channels.is_some() {
            return Err(ProtocolError::XWaylandCompletionChannelsAlreadyInstalled);
        }
        self.xwayland_completion_channels = Some(XWaylandCompletionChannels {
            events,
            control,
            property_events,
            property_control,
        });
        Ok(())
    }

    pub(crate) fn take_xwayland_completion_runtime(&mut self) -> Option<OpaqueFdCompletionRuntime> {
        self.xwayland_completion_runtime.take()
    }

    pub(super) fn start_xwayland(&mut self) -> Result<(), ProtocolError> {
        if self.state.has_xwayland_process() {
            return Err(ProtocolError::XWaylandAlreadyStarted);
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
            channels.property_events.clone(),
            channels.property_control.clone(),
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
                assert!(matches!(
                    runtime.start_xwayland(),
                    Err(super::ProtocolError::XWaylandAlreadyStarted)
                ));
                return;
            }
        }
        panic!("XWayland did not complete its displayfd startup handshake");
    }
}
