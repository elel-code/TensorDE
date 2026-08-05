mod service;

pub(crate) use service::PowerServiceHandle;

use tensor_dbus::freedesktop::upower::{BatteryState, BatteryWarning, UPowerSnapshot};

use crate::{PanelAppletEmphasis, PanelAppletState};

/// Retained product-facing lifecycle for the UPower-backed panel state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum PowerServiceSnapshot {
    #[default]
    Pending,
    Ready(UPowerSnapshot),
    Unavailable,
    Failed,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PowerServiceStore {
    snapshot: PowerServiceSnapshot,
    revision: u64,
}

impl PowerServiceStore {
    pub(crate) fn publish(&mut self, snapshot: PowerServiceSnapshot) -> bool {
        if self.snapshot == snapshot {
            return false;
        }
        self.snapshot = snapshot;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(crate) fn read(&self) -> (u64, PowerServiceSnapshot) {
        (self.revision, self.snapshot.clone())
    }
}

impl PowerServiceSnapshot {
    pub fn panel_state(&self) -> PanelAppletState {
        match self {
            Self::Pending => PanelAppletState::pending(),
            Self::Ready(snapshot) => power_applet_state(snapshot),
            Self::Unavailable => PanelAppletState::unavailable(),
            Self::Failed => PanelAppletState::failed(),
        }
    }
}

/// Lowers validated UPower semantics into the bounded render-facing applet ABI.
pub fn power_applet_state(snapshot: &UPowerSnapshot) -> PanelAppletState {
    if !snapshot.battery_present() {
        return PanelAppletState::ready();
    }
    let Some(percentage) = snapshot.percentage() else {
        return PanelAppletState::failed();
    };
    let emphasis = match (snapshot.warning(), snapshot.state()) {
        (BatteryWarning::Critical | BatteryWarning::Action, _) | (_, BatteryState::Empty) => {
            PanelAppletEmphasis::Critical
        }
        (BatteryWarning::Low, _) => PanelAppletEmphasis::Attention,
        (_, BatteryState::Charging | BatteryState::PendingCharge) => PanelAppletEmphasis::Active,
        _ => PanelAppletEmphasis::Normal,
    };
    PanelAppletState::ready()
        .with_meter(percentage)
        .with_emphasis(emphasis)
}

#[cfg(test)]
mod tests {
    use tensor_dbus::{
        freedesktop::upower::{BatteryState, BatteryWarning, PowerSource, UPowerSnapshot},
        zvariant::OwnedObjectPath,
    };

    use super::*;
    use crate::PanelAppletAvailability;

    fn snapshot(
        source: PowerSource,
        present: bool,
        percentage: Option<u8>,
        state: BatteryState,
        warning: BatteryWarning,
    ) -> UPowerSnapshot {
        UPowerSnapshot::from_parts(
            OwnedObjectPath::try_from("/org/freedesktop/UPower/devices/DisplayDevice").unwrap(),
            source,
            present,
            percentage,
            state,
            warning,
        )
    }

    #[test]
    fn unavailable_and_failed_services_remain_distinct() {
        assert_eq!(
            PowerServiceSnapshot::Unavailable
                .panel_state()
                .availability(),
            PanelAppletAvailability::Unavailable
        );
        assert_eq!(
            PowerServiceSnapshot::Failed.panel_state().availability(),
            PanelAppletAvailability::Failed
        );
    }

    #[test]
    fn duplicate_service_snapshots_do_not_advance_the_revision() {
        let mut store = PowerServiceStore::default();
        assert!(!store.publish(PowerServiceSnapshot::Pending));
        assert!(store.publish(PowerServiceSnapshot::Unavailable));
        assert!(!store.publish(PowerServiceSnapshot::Unavailable));
        assert_eq!(store.read().0, 1);
    }

    #[test]
    fn ac_only_system_is_ready_without_a_stale_meter() {
        let state = power_applet_state(&snapshot(
            PowerSource::Ac,
            false,
            Some(82),
            BatteryState::Discharging,
            BatteryWarning::Low,
        ));
        assert_eq!(state.availability(), PanelAppletAvailability::Ready);
        assert_eq!(state.meter(), None);
        assert_eq!(state.emphasis(), PanelAppletEmphasis::Normal);
    }

    #[test]
    fn charging_and_warning_states_have_stable_precedence() {
        let charging = power_applet_state(&snapshot(
            PowerSource::Ac,
            true,
            Some(61),
            BatteryState::Charging,
            BatteryWarning::None,
        ));
        assert_eq!(charging.meter(), Some(61));
        assert_eq!(charging.emphasis(), PanelAppletEmphasis::Active);

        let low = power_applet_state(&snapshot(
            PowerSource::Battery,
            true,
            Some(14),
            BatteryState::Discharging,
            BatteryWarning::Low,
        ));
        assert_eq!(low.emphasis(), PanelAppletEmphasis::Attention);

        let critical = power_applet_state(&snapshot(
            PowerSource::Battery,
            true,
            Some(4),
            BatteryState::Discharging,
            BatteryWarning::Action,
        ));
        assert_eq!(critical.emphasis(), PanelAppletEmphasis::Critical);
    }
}
