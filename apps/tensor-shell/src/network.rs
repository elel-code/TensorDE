mod service;

pub use service::NetworkServiceError;
pub(crate) use service::NetworkServiceHandle;

use tensor_dbus::freedesktop::network_manager::{
    Connectivity, NetworkManagerSnapshot, NetworkState,
    wifi::{NetworkManagerDetailsSnapshot, WifiAccessPointSnapshot},
};

use crate::{PanelAppletEmphasis, PanelAppletState};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum NetworkServiceSnapshot {
    #[default]
    Pending,
    Ready(NetworkManagerDetailsSnapshot),
    Unavailable,
    Failed,
}

impl NetworkServiceSnapshot {
    pub fn panel_state(&self) -> PanelAppletState {
        match self {
            Self::Pending => PanelAppletState::pending(),
            Self::Ready(snapshot) => network_details_applet_state(snapshot),
            Self::Unavailable => PanelAppletState::unavailable(),
            Self::Failed => PanelAppletState::failed(),
        }
    }

    pub const fn supports_wireless_toggle(&self) -> bool {
        match self {
            Self::Ready(snapshot) => {
                snapshot.root().networking_enabled() && snapshot.root().wireless_hardware_enabled()
            }
            Self::Pending | Self::Unavailable | Self::Failed => false,
        }
    }

    pub fn details(&self) -> Option<&NetworkManagerDetailsSnapshot> {
        match self {
            Self::Ready(snapshot) => Some(snapshot),
            Self::Pending | Self::Unavailable | Self::Failed => None,
        }
    }

    pub fn active_access_point(&self) -> Option<&WifiAccessPointSnapshot> {
        self.details()?.wifi().active_access_point()
    }

    pub fn current_ssid(&self) -> Option<&str> {
        self.active_access_point().map(|point| point.ssid_display())
    }

    pub fn signal_strength(&self) -> Option<u8> {
        self.active_access_point()
            .map(WifiAccessPointSnapshot::signal_strength)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NetworkActionState {
    #[default]
    Idle,
    Pending(bool),
    Succeeded(bool),
    Failed(bool),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct NetworkServiceStore {
    snapshot: NetworkServiceSnapshot,
    action: NetworkActionState,
    revision: u64,
}

impl NetworkServiceStore {
    pub(crate) fn publish_snapshot(&mut self, snapshot: NetworkServiceSnapshot) -> bool {
        if self.snapshot == snapshot {
            return false;
        }
        self.snapshot = snapshot;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(crate) fn publish_action(&mut self, action: NetworkActionState) -> bool {
        if self.action == action {
            return false;
        }
        self.action = action;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(crate) fn publish_external_snapshot(&mut self, snapshot: NetworkServiceSnapshot) -> bool {
        let changed = self.publish_snapshot(snapshot);
        if changed {
            self.publish_action(NetworkActionState::Idle);
        }
        changed
    }

    pub(crate) fn begin_toggle(&mut self) -> Result<bool, NetworkServiceError> {
        if matches!(self.action, NetworkActionState::Pending(_)) {
            return Err(NetworkServiceError::Busy);
        }
        let NetworkServiceSnapshot::Ready(snapshot) = &self.snapshot else {
            return Err(NetworkServiceError::Unavailable);
        };
        if !snapshot.root().networking_enabled() || !snapshot.root().wireless_hardware_enabled() {
            return Err(NetworkServiceError::WirelessUnavailable);
        }
        let target = !snapshot.root().wireless_enabled();
        self.publish_action(NetworkActionState::Pending(target));
        Ok(target)
    }

    pub(crate) fn read(&self) -> (u64, NetworkServiceSnapshot, NetworkActionState) {
        (self.revision, self.snapshot.clone(), self.action)
    }
}

pub fn network_applet_state(snapshot: &NetworkManagerSnapshot) -> PanelAppletState {
    let emphasis = match (snapshot.state(), snapshot.connectivity()) {
        (_, Connectivity::Portal | Connectivity::Limited)
        | (
            NetworkState::ConnectedLocal | NetworkState::ConnectedSite,
            Connectivity::Unknown | Connectivity::None | Connectivity::Full,
        ) => PanelAppletEmphasis::Attention,
        (
            NetworkState::Connecting | NetworkState::Disconnecting | NetworkState::ConnectedGlobal,
            _,
        ) => PanelAppletEmphasis::Active,
        (NetworkState::Unknown | NetworkState::Disabled | NetworkState::Disconnected, _) => {
            PanelAppletEmphasis::Normal
        }
    };
    PanelAppletState::ready().with_emphasis(emphasis)
}

fn network_details_applet_state(snapshot: &NetworkManagerDetailsSnapshot) -> PanelAppletState {
    lower_network_metrics(
        snapshot.root(),
        snapshot.wifi().access_points().len(),
        snapshot
            .wifi()
            .active_access_point()
            .map(WifiAccessPointSnapshot::signal_strength),
    )
}

fn lower_network_metrics(
    snapshot: &NetworkManagerSnapshot,
    access_point_count: usize,
    signal_strength: Option<u8>,
) -> PanelAppletState {
    let mut state = network_applet_state(snapshot).with_badge(access_point_count);
    if let Some(signal_strength) = signal_strength {
        state = state.with_meter(signal_strength);
    }
    state
}

#[cfg(test)]
mod tests {
    use tensor_dbus::freedesktop::network_manager::PrimaryConnectionKind;

    use super::*;
    use crate::PanelAppletAvailability;

    fn root_snapshot(
        wireless: bool,
        state: NetworkState,
        connectivity: Connectivity,
    ) -> NetworkManagerSnapshot {
        NetworkManagerSnapshot::from_parts(
            true,
            wireless,
            true,
            state,
            connectivity,
            PrimaryConnectionKind::Wifi,
        )
    }

    fn snapshot(
        wireless: bool,
        state: NetworkState,
        connectivity: Connectivity,
    ) -> NetworkManagerDetailsSnapshot {
        NetworkManagerDetailsSnapshot::from_parts(
            root_snapshot(wireless, state, connectivity),
            Default::default(),
        )
    }

    #[test]
    fn lifecycle_and_connectivity_lower_to_stable_applet_states() {
        assert_eq!(
            NetworkServiceSnapshot::Unavailable
                .panel_state()
                .availability(),
            PanelAppletAvailability::Unavailable
        );
        assert_eq!(
            network_applet_state(&root_snapshot(
                true,
                NetworkState::ConnectedGlobal,
                Connectivity::Full,
            ))
            .emphasis(),
            PanelAppletEmphasis::Active
        );
        assert_eq!(
            network_applet_state(&root_snapshot(
                true,
                NetworkState::ConnectedSite,
                Connectivity::Portal,
            ))
            .emphasis(),
            PanelAppletEmphasis::Attention
        );
    }

    #[test]
    fn toggle_reservation_is_bounded_by_readiness_hardware_and_in_flight_state() {
        let mut store = NetworkServiceStore::default();
        assert!(matches!(
            store.begin_toggle(),
            Err(NetworkServiceError::Unavailable)
        ));
        store.publish_snapshot(NetworkServiceSnapshot::Ready(snapshot(
            true,
            NetworkState::ConnectedGlobal,
            Connectivity::Full,
        )));
        assert!(!store.begin_toggle().unwrap());
        assert_eq!(store.action, NetworkActionState::Pending(false));
        assert!(matches!(
            store.begin_toggle(),
            Err(NetworkServiceError::Busy)
        ));
    }

    #[test]
    fn duplicate_snapshot_and_action_do_not_advance_revision() {
        let mut store = NetworkServiceStore::default();
        assert!(!store.publish_snapshot(NetworkServiceSnapshot::Pending));
        assert!(!store.publish_action(NetworkActionState::Idle));
        assert_eq!(store.revision, 0);
    }

    #[test]
    fn wifi_details_lower_to_bounded_badge_and_signal_meter() {
        let state = lower_network_metrics(
            &root_snapshot(true, NetworkState::ConnectedGlobal, Connectivity::Full),
            256,
            Some(117),
        );
        assert_eq!(state.badge(), Some(256));
        assert_eq!(state.meter(), Some(100));
        assert_eq!(state.emphasis(), PanelAppletEmphasis::Active);
    }

    #[test]
    fn external_snapshot_change_clears_stale_action_feedback() {
        let mut store = NetworkServiceStore::default();
        let connected = NetworkServiceSnapshot::Ready(snapshot(
            true,
            NetworkState::ConnectedGlobal,
            Connectivity::Full,
        ));
        store.publish_snapshot(connected.clone());
        store.publish_action(NetworkActionState::Succeeded(true));

        let disconnected = NetworkServiceSnapshot::Ready(snapshot(
            false,
            NetworkState::Disconnected,
            Connectivity::None,
        ));
        assert!(store.publish_external_snapshot(disconnected.clone()));
        assert_eq!(store.action, NetworkActionState::Idle);

        store.publish_action(NetworkActionState::Failed(false));
        assert!(!store.publish_external_snapshot(disconnected));
        assert_eq!(store.action, NetworkActionState::Failed(false));
    }
}
