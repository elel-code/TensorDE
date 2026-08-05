use super::PanelWidgetKind;

const MAX_BADGE: u16 = 999;

/// Whether the product service behind an applet can currently provide state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PanelAppletAvailability {
    #[default]
    Pending,
    Ready,
    Unavailable,
    Failed,
}

/// Product-neutral visual importance after a service snapshot is validated.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PanelAppletEmphasis {
    #[default]
    Normal,
    Active,
    Attention,
    Critical,
}

/// Bounded render-facing state for one built-in panel applet.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PanelAppletState {
    availability: PanelAppletAvailability,
    emphasis: PanelAppletEmphasis,
    badge: Option<u16>,
    meter: Option<u8>,
}

impl PanelAppletState {
    pub const fn pending() -> Self {
        Self {
            availability: PanelAppletAvailability::Pending,
            emphasis: PanelAppletEmphasis::Normal,
            badge: None,
            meter: None,
        }
    }

    pub const fn ready() -> Self {
        Self {
            availability: PanelAppletAvailability::Ready,
            ..Self::pending()
        }
    }

    pub const fn unavailable() -> Self {
        Self {
            availability: PanelAppletAvailability::Unavailable,
            ..Self::pending()
        }
    }

    pub const fn failed() -> Self {
        Self {
            availability: PanelAppletAvailability::Failed,
            ..Self::pending()
        }
    }

    pub const fn with_emphasis(mut self, emphasis: PanelAppletEmphasis) -> Self {
        self.emphasis = emphasis;
        self
    }

    pub fn with_badge(mut self, count: usize) -> Self {
        self.badge =
            (count != 0).then_some(u16::try_from(count).unwrap_or(u16::MAX).min(MAX_BADGE));
        self
    }

    pub const fn with_meter(mut self, percent: u8) -> Self {
        self.meter = Some(if percent > 100 { 100 } else { percent });
        self
    }

    pub const fn availability(self) -> PanelAppletAvailability {
        self.availability
    }

    pub const fn emphasis(self) -> PanelAppletEmphasis {
        self.emphasis
    }

    pub const fn badge(self) -> Option<u16> {
        self.badge
    }

    pub const fn meter(self) -> Option<u8> {
        self.meter
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanelAppletUpdate {
    pub widget: PanelWidgetKind,
    pub state: PanelAppletState,
}

impl PanelAppletUpdate {
    pub const fn new(widget: PanelWidgetKind, state: PanelAppletState) -> Self {
        Self { widget, state }
    }
}

/// Fixed-size retained applet view model. D-Bus completion order never reaches
/// rendering directly: product services publish complete snapshots here first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PanelAppletStore {
    states: [PanelAppletState; PanelWidgetKind::ALL.len()],
    revision: u64,
    dirty: u16,
}

impl Default for PanelAppletStore {
    fn default() -> Self {
        let mut states = [PanelAppletState::pending(); PanelWidgetKind::ALL.len()];
        for widget in [
            PanelWidgetKind::Launcher,
            PanelWidgetKind::Clock,
            PanelWidgetKind::Notifications,
            PanelWidgetKind::ControlCenter,
        ] {
            states[widget.index()] = PanelAppletState::ready();
        }
        Self {
            states,
            revision: 0,
            dirty: 0,
        }
    }
}

impl PanelAppletStore {
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub const fn state(&self, widget: PanelWidgetKind) -> PanelAppletState {
        self.states[widget.index()]
    }

    pub fn apply(&mut self, update: PanelAppletUpdate) -> bool {
        let index = update.widget.index();
        if self.states[index] == update.state {
            return false;
        }
        self.states[index] = update.state;
        self.revision = self.revision.wrapping_add(1);
        self.dirty |= 1 << index;
        true
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_snapshot_does_not_advance_or_dirty_the_store() {
        let mut store = PanelAppletStore::default();
        assert!(!store.apply(PanelAppletUpdate::new(
            PanelWidgetKind::Clock,
            PanelAppletState::ready(),
        )));
        assert_eq!(store.revision(), 0);
        assert!(!store.take_dirty());
    }

    #[test]
    fn updates_are_bounded_and_coalesce_into_one_dirty_batch() {
        let mut store = PanelAppletStore::default();
        assert!(
            store.apply(PanelAppletUpdate::new(
                PanelWidgetKind::Notifications,
                PanelAppletState::ready()
                    .with_badge(usize::MAX)
                    .with_emphasis(PanelAppletEmphasis::Attention),
            ))
        );
        assert!(store.apply(PanelAppletUpdate::new(
            PanelWidgetKind::SystemStatus,
            PanelAppletState::ready().with_meter(255),
        )));
        assert_eq!(store.revision(), 2);
        assert_eq!(
            store.state(PanelWidgetKind::Notifications).badge(),
            Some(MAX_BADGE)
        );
        assert_eq!(
            store.state(PanelWidgetKind::SystemStatus).meter(),
            Some(100)
        );
        assert!(store.take_dirty());
        assert!(!store.take_dirty());
    }

    #[test]
    fn service_backed_widgets_start_pending_without_hiding_entry_points() {
        let store = PanelAppletStore::default();
        assert_eq!(
            store.state(PanelWidgetKind::SystemStatus).availability(),
            PanelAppletAvailability::Pending
        );
        assert_eq!(
            store.state(PanelWidgetKind::Launcher).availability(),
            PanelAppletAvailability::Ready
        );
    }
}
