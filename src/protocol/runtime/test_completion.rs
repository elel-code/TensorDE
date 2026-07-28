use std::{
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::Duration,
};

use tensor_runtime::WorkerRx;

use super::{
    ProtocolError, RuntimeState, WaylandDisplayControlEvent, WaylandDisplayEvent,
    WaylandSocketControlEvent, drain_wayland_display_events, drain_wayland_socket_events,
};
#[cfg(feature = "xwayland")]
use super::{
    XWaylandPropertyControlEvent, XWaylandPropertyEvent, XWaylandStartupControlEvent,
    XWaylandStartupEvent, drain_xwayland_events,
};

pub(super) struct TestCompletionBridges {
    pub(super) clients: WorkerRx<std::os::unix::net::UnixStream>,
    pub(super) socket_control: WorkerRx<WaylandSocketControlEvent>,
    pub(super) display: WorkerRx<WaylandDisplayEvent>,
    pub(super) display_control: WorkerRx<WaylandDisplayControlEvent>,
    #[cfg(feature = "xwayland")]
    pub(super) xwayland: WorkerRx<XWaylandStartupEvent>,
    #[cfg(feature = "xwayland")]
    pub(super) xwayland_control: WorkerRx<XWaylandStartupControlEvent>,
    #[cfg(feature = "xwayland")]
    pub(super) xwayland_properties: WorkerRx<XWaylandPropertyEvent>,
    #[cfg(feature = "xwayland")]
    pub(super) xwayland_property_control: WorkerRx<XWaylandPropertyControlEvent>,
}

#[derive(Default)]
pub(super) struct TestCompletionLoop {
    notifications: Option<Receiver<()>>,
    bridges: Option<TestCompletionBridges>,
}

impl TestCompletionLoop {
    pub(super) fn install(&mut self, notifications: Receiver<()>, bridges: TestCompletionBridges) {
        assert!(
            self.bridges.is_none(),
            "test completion bridges were installed more than once"
        );
        self.notifications = Some(notifications);
        self.bridges = Some(bridges);
    }

    pub(super) fn dispatch(
        &mut self,
        timeout: Duration,
        state: &mut RuntimeState,
    ) -> Result<(), ProtocolError> {
        let notifications = self.notifications.as_ref().ok_or_else(|| {
            ProtocolError::TestCompletion("test completion bridges are not installed".to_owned())
        })?;
        match notifications.recv_timeout(timeout) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(ProtocolError::TestCompletion(
                    "test completion relay disconnected".to_owned(),
                ));
            }
        }

        let bridges = self.bridges.as_ref().expect("checked test bridges");
        drain_wayland_socket_events(&bridges.clients, &bridges.socket_control, state)
            .map_err(ProtocolError::TestCompletion)?;
        drain_wayland_display_events(&bridges.display, &bridges.display_control, state)
            .map_err(ProtocolError::TestCompletion)?;
        #[cfg(feature = "xwayland")]
        drain_xwayland_events(
            &bridges.xwayland,
            &bridges.xwayland_control,
            &bridges.xwayland_properties,
            &bridges.xwayland_property_control,
            state,
        )
        .map_err(ProtocolError::TestCompletion)?;
        Ok(())
    }
}
