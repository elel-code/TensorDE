use smithay::{
    reexports::wayland_server::DisplayHandle,
    wayland::{
        fractional_scale::FractionalScaleManagerState, pointer_gestures::PointerGesturesState,
        relative_pointer::RelativePointerManagerState,
        selection::primary_selection::PrimarySelectionState,
        shell::xdg::decoration::XdgDecorationState, viewporter::ViewporterState,
    },
};

use super::state::RuntimeState;

pub(crate) struct ProtocolGlobals {
    viewporter: ViewporterState,
    fractional_scale: FractionalScaleManagerState,
    xdg_decoration: XdgDecorationState,
    primary_selection: PrimarySelectionState,
    relative_pointer: RelativePointerManagerState,
    pointer_gestures: PointerGesturesState,
}

impl ProtocolGlobals {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            viewporter: ViewporterState::new::<RuntimeState>(display),
            fractional_scale: FractionalScaleManagerState::new::<RuntimeState>(display),
            xdg_decoration: XdgDecorationState::new::<RuntimeState>(display),
            primary_selection: PrimarySelectionState::new::<RuntimeState>(display),
            relative_pointer: RelativePointerManagerState::new::<RuntimeState>(display),
            pointer_gestures: PointerGesturesState::new::<RuntimeState>(display),
        }
    }

    pub(crate) fn primary_selection(&mut self) -> &mut PrimarySelectionState {
        &mut self.primary_selection
    }

    pub(crate) fn capabilities(&self) -> ProtocolCapabilities {
        let _global_owners = (
            &self.viewporter,
            &self.fractional_scale,
            &self.xdg_decoration,
            &self.primary_selection,
            &self.relative_pointer,
            &self.pointer_gestures,
        );
        ProtocolCapabilities {
            viewporter: true,
            fractional_scale: true,
            xdg_decoration: true,
            primary_selection: true,
            relative_pointer: true,
            pointer_gestures: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProtocolCapabilities {
    pub(crate) viewporter: bool,
    pub(crate) fractional_scale: bool,
    pub(crate) xdg_decoration: bool,
    pub(crate) primary_selection: bool,
    pub(crate) relative_pointer: bool,
    pub(crate) pointer_gestures: bool,
}

#[cfg(test)]
mod tests {
    use smithay::reexports::wayland_server::Display;

    use super::*;
    use crate::layout::{LayoutEngine, LayoutKind};

    #[test]
    fn long_lived_globals_are_owned_as_one_capability_set() {
        let display = Display::<RuntimeState>::new().unwrap();
        let state = RuntimeState::new(display.handle(), LayoutEngine::new(LayoutKind::Scrolling1D));

        assert_eq!(
            state.protocol_globals.capabilities(),
            ProtocolCapabilities {
                viewporter: true,
                fractional_scale: true,
                xdg_decoration: true,
                primary_selection: true,
                relative_pointer: true,
                pointer_gestures: true,
            }
        );
    }
}
