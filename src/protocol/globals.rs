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

#[cfg(feature = "tty")]
mod dmabuf;
#[cfg(feature = "tty")]
mod syncobj;

#[cfg(feature = "tty")]
use dmabuf::DmabufProtocol;
#[cfg(feature = "tty")]
use syncobj::DrmSyncobjProtocol;

pub(crate) struct ProtocolGlobals {
    viewporter: ViewporterState,
    fractional_scale: FractionalScaleManagerState,
    xdg_decoration: XdgDecorationState,
    primary_selection: PrimarySelectionState,
    relative_pointer: RelativePointerManagerState,
    pointer_gestures: PointerGesturesState,
    #[cfg(feature = "tty")]
    dmabuf: DmabufProtocol,
    #[cfg(feature = "tty")]
    syncobj: DrmSyncobjProtocol,
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
            #[cfg(feature = "tty")]
            dmabuf: DmabufProtocol::new(),
            #[cfg(feature = "tty")]
            syncobj: DrmSyncobjProtocol::new(),
        }
    }

    #[cfg(feature = "tty")]
    pub(crate) fn install_dmabuf(
        &mut self,
        display: &DisplayHandle,
        main_device: libc::dev_t,
        formats: impl IntoIterator<Item = smithay::backend::allocator::Format>,
    ) -> Result<bool, String> {
        self.dmabuf.install(display, main_device, formats)
    }

    #[cfg(feature = "tty")]
    pub(crate) fn dmabuf_state(&mut self) -> &mut smithay::wayland::dmabuf::DmabufState {
        &mut self.dmabuf.state
    }

    #[cfg(feature = "tty")]
    pub(crate) fn update_syncobj(
        &mut self,
        display: &DisplayHandle,
        device: Option<smithay::backend::drm::DrmDeviceFd>,
    ) {
        self.syncobj.update(display, device);
    }

    #[cfg(feature = "tty")]
    pub(crate) fn drm_syncobj_state(
        &mut self,
    ) -> Option<&mut smithay::wayland::drm_syncobj::DrmSyncobjState> {
        self.syncobj.state.as_mut()
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
            #[cfg(feature = "tty")]
            linux_dmabuf: self.dmabuf.advertised(),
            #[cfg(feature = "tty")]
            linux_drm_syncobj: self.syncobj.advertised(),
            #[cfg(feature = "tty")]
            linux_drm_syncobj_active: self.syncobj.active(),
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
    #[cfg(feature = "tty")]
    pub(crate) linux_dmabuf: bool,
    #[cfg(feature = "tty")]
    pub(crate) linux_drm_syncobj: bool,
    #[cfg(feature = "tty")]
    pub(crate) linux_drm_syncobj_active: bool,
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
                #[cfg(feature = "tty")]
                linux_dmabuf: false,
                #[cfg(feature = "tty")]
                linux_drm_syncobj: false,
                #[cfg(feature = "tty")]
                linux_drm_syncobj_active: false,
            }
        );
    }
}
