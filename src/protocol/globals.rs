use smithay::{
    reexports::{calloop::LoopHandle, wayland_server::DisplayHandle},
    utils::{ClockSource, Monotonic},
    wayland::{
        cursor_shape::CursorShapeManagerState,
        fractional_scale::FractionalScaleManagerState,
        idle_notify::IdleNotifierState,
        pointer_gestures::PointerGesturesState,
        presentation::PresentationState,
        relative_pointer::RelativePointerManagerState,
        selection::primary_selection::PrimarySelectionState,
        shell::{wlr_layer::WlrLayerShellState, xdg::decoration::XdgDecorationState},
        viewporter::ViewporterState,
        xdg_activation::XdgActivationState,
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
    presentation: PresentationState,
    cursor_shape: CursorShapeManagerState,
    activation: XdgActivationState,
    idle_notifier: IdleNotifierState<RuntimeState>,
    layer_shell: WlrLayerShellState,
    #[cfg(feature = "tty")]
    dmabuf: DmabufProtocol,
    #[cfg(feature = "tty")]
    syncobj: DrmSyncobjProtocol,
}

impl ProtocolGlobals {
    pub(crate) fn new(
        display: &DisplayHandle,
        loop_handle: &LoopHandle<'static, RuntimeState>,
    ) -> Self {
        Self {
            viewporter: ViewporterState::new::<RuntimeState>(display),
            fractional_scale: FractionalScaleManagerState::new::<RuntimeState>(display),
            xdg_decoration: XdgDecorationState::new::<RuntimeState>(display),
            primary_selection: PrimarySelectionState::new::<RuntimeState>(display),
            relative_pointer: RelativePointerManagerState::new::<RuntimeState>(display),
            pointer_gestures: PointerGesturesState::new::<RuntimeState>(display),
            presentation: PresentationState::new::<RuntimeState>(display, Monotonic::ID as u32),
            cursor_shape: CursorShapeManagerState::new::<RuntimeState>(display),
            activation: XdgActivationState::new::<RuntimeState>(display),
            idle_notifier: IdleNotifierState::new(display, loop_handle.clone()),
            layer_shell: WlrLayerShellState::new::<RuntimeState>(display),
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

    pub(crate) fn activation(&mut self) -> &mut XdgActivationState {
        &mut self.activation
    }

    pub(crate) fn idle_notifier(&mut self) -> &mut IdleNotifierState<RuntimeState> {
        &mut self.idle_notifier
    }

    pub(crate) fn layer_shell(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell
    }

    pub(crate) fn capabilities(&self) -> ProtocolCapabilities {
        let _global_owners = (
            &self.viewporter,
            &self.fractional_scale,
            &self.xdg_decoration,
            &self.primary_selection,
            &self.relative_pointer,
            &self.pointer_gestures,
            &self.presentation,
            &self.cursor_shape,
            &self.activation,
            &self.idle_notifier,
            &self.layer_shell,
        );
        ProtocolCapabilities {
            viewporter: true,
            fractional_scale: true,
            xdg_decoration: true,
            primary_selection: true,
            relative_pointer: true,
            pointer_gestures: true,
            presentation_time: true,
            cursor_shape: true,
            xdg_activation: true,
            idle_notify: true,
            layer_shell: true,
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
    pub(crate) presentation_time: bool,
    pub(crate) cursor_shape: bool,
    pub(crate) xdg_activation: bool,
    pub(crate) idle_notify: bool,
    pub(crate) layer_shell: bool,
    #[cfg(feature = "tty")]
    pub(crate) linux_dmabuf: bool,
    #[cfg(feature = "tty")]
    pub(crate) linux_drm_syncobj: bool,
    #[cfg(feature = "tty")]
    pub(crate) linux_drm_syncobj_active: bool,
}

#[cfg(test)]
mod tests {
    use smithay::reexports::{calloop::EventLoop, wayland_server::Display};

    use super::*;
    use crate::layout::{LayoutEngine, LayoutKind};

    #[test]
    fn long_lived_globals_are_owned_as_one_capability_set() {
        let event_loop = EventLoop::<RuntimeState>::try_new().unwrap();
        let display = Display::<RuntimeState>::new().unwrap();
        let state = RuntimeState::with_appearance(
            display.handle(),
            event_loop.handle(),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            crate::scene::SceneAppearance::default(),
        );

        assert_eq!(
            state.protocol_globals.capabilities(),
            ProtocolCapabilities {
                viewporter: true,
                fractional_scale: true,
                xdg_decoration: true,
                primary_selection: true,
                relative_pointer: true,
                pointer_gestures: true,
                presentation_time: true,
                cursor_shape: true,
                xdg_activation: true,
                idle_notify: true,
                layer_shell: true,
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
