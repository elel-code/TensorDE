use calloop::LoopHandle;
#[cfg(feature = "xwayland")]
use smithay::wayland::xwayland_keyboard_grab::XWaylandKeyboardGrabState;
use smithay::{
    utils::{ClockSource, Monotonic},
    wayland::{
        alpha_modifier::AlphaModifierState,
        background_effect::BackgroundEffectState,
        commit_timing::CommitTimingManagerState,
        content_type::ContentTypeState,
        cursor_shape::CursorShapeManagerState,
        fifo::FifoManagerState,
        foreign_toplevel_list::ForeignToplevelListState,
        fractional_scale::FractionalScaleManagerState,
        idle_inhibit::IdleInhibitManagerState,
        idle_notify::IdleNotifierState,
        image_capture_source::{
            ImageCaptureSourceState, OutputCaptureSourceState, ToplevelCaptureSourceState,
        },
        image_copy_capture::ImageCopyCaptureState,
        input_method::InputMethodManagerState,
        keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitState,
        pointer_constraints::PointerConstraintsState,
        pointer_gestures::PointerGesturesState,
        pointer_warp::PointerWarpManager,
        presentation::PresentationState,
        relative_pointer::RelativePointerManagerState,
        selection::{
            ext_data_control::DataControlState as ExtDataControlState,
            primary_selection::PrimarySelectionState,
            wlr_data_control::DataControlState as WlrDataControlState,
        },
        session_lock::SessionLockManagerState,
        shell::{wlr_layer::WlrLayerShellState, xdg::decoration::XdgDecorationState},
        single_pixel_buffer::SinglePixelBufferState,
        tablet_manager::TabletManagerState,
        text_input::TextInputManagerState,
        viewporter::ViewporterState,
        virtual_keyboard::VirtualKeyboardManagerState,
        xdg_activation::XdgActivationState,
        xdg_foreign::XdgForeignState,
        xdg_system_bell::XdgSystemBellState,
        xdg_toplevel_icon::XdgToplevelIconManager,
        xdg_toplevel_tag::XdgToplevelTagManager,
    },
};
use wayland_server::{Client, DisplayHandle};

use super::extensions::{
    ext_workspace::ExtWorkspaceManagerState, gamma_control::GammaControlManagerState,
    output_management::OutputManagementState, security_context::SecurityContextManagerState,
    virtual_pointer::VirtualPointerManagerState,
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
    wlr_data_control: WlrDataControlState,
    ext_data_control: ExtDataControlState,
    relative_pointer: RelativePointerManagerState,
    pointer_gestures: PointerGesturesState,
    pointer_constraints: PointerConstraintsState,
    presentation: PresentationState,
    cursor_shape: CursorShapeManagerState,
    activation: XdgActivationState,
    idle_notifier: IdleNotifierState<RuntimeState>,
    idle_inhibit: IdleInhibitManagerState,
    layer_shell: WlrLayerShellState,
    single_pixel_buffer: SinglePixelBufferState,
    keyboard_shortcuts_inhibit: KeyboardShortcutsInhibitState,
    tablet: TabletManagerState,
    text_input: TextInputManagerState,
    input_method: InputMethodManagerState,
    virtual_keyboard: VirtualKeyboardManagerState,
    session_lock: SessionLockManagerState,
    security_context: SecurityContextManagerState,
    foreign_toplevel_list: ForeignToplevelListState,
    xdg_foreign: XdgForeignState,
    system_bell: XdgSystemBellState,
    pointer_warp: PointerWarpManager,
    content_type: ContentTypeState,
    alpha_modifier: AlphaModifierState,
    background_effect: BackgroundEffectState,
    toplevel_icon: XdgToplevelIconManager,
    toplevel_tag: XdgToplevelTagManager,
    fifo: FifoManagerState,
    commit_timing: CommitTimingManagerState,
    #[cfg(feature = "xwayland")]
    xwayland_keyboard_grab: XWaylandKeyboardGrabState,
    virtual_pointer: VirtualPointerManagerState,
    gamma_control: GammaControlManagerState,
    ext_workspace: ExtWorkspaceManagerState,
    output_management: OutputManagementState,
    /// Opaque capture sources (shared by output/toplevel managers).
    image_capture_source: ImageCaptureSourceState,
    output_capture_source: OutputCaptureSourceState,
    toplevel_capture_source: ToplevelCaptureSourceState,
    image_copy_capture: ImageCopyCaptureState,
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
        let _ = loop_handle;
        let primary_selection = PrimarySelectionState::new::<RuntimeState>(display);
        let unrestricted = |client: &Client| {
            client
                .get_data::<crate::protocol::state::WaylandClientState>()
                .is_none_or(|data| data.security_context.is_none())
        };
        let wlr_data_control = WlrDataControlState::new::<RuntimeState, _>(
            display,
            Some(&primary_selection),
            unrestricted,
        );
        let ext_data_control = ExtDataControlState::new::<RuntimeState, _>(
            display,
            Some(&primary_selection),
            unrestricted,
        );
        Self {
            viewporter: ViewporterState::new::<RuntimeState>(display),
            fractional_scale: FractionalScaleManagerState::new::<RuntimeState>(display),
            xdg_decoration: XdgDecorationState::new::<RuntimeState>(display),
            primary_selection,
            wlr_data_control,
            ext_data_control,
            relative_pointer: RelativePointerManagerState::new::<RuntimeState>(display),
            pointer_gestures: PointerGesturesState::new::<RuntimeState>(display),
            pointer_constraints: PointerConstraintsState::new::<RuntimeState>(display),
            presentation: PresentationState::new::<RuntimeState>(display, Monotonic::ID as u32),
            cursor_shape: CursorShapeManagerState::new::<RuntimeState>(display),
            activation: XdgActivationState::new::<RuntimeState>(display),
            idle_notifier: IdleNotifierState::new(display, loop_handle.clone()),
            idle_inhibit: IdleInhibitManagerState::new::<RuntimeState>(display),
            layer_shell: WlrLayerShellState::new::<RuntimeState>(display),
            single_pixel_buffer: SinglePixelBufferState::new::<RuntimeState>(display),
            keyboard_shortcuts_inhibit: KeyboardShortcutsInhibitState::new::<RuntimeState>(display),
            tablet: TabletManagerState::new::<RuntimeState>(display),
            text_input: TextInputManagerState::new::<RuntimeState>(display),
            // Privileged input-method / virtual-keyboard: unrestricted clients only.
            input_method: InputMethodManagerState::new::<RuntimeState, _>(display, |_| true),
            virtual_keyboard: VirtualKeyboardManagerState::new::<RuntimeState, _>(display, |_| {
                true
            }),
            session_lock: SessionLockManagerState::new::<RuntimeState, _>(display, |_| true),
            // Sandboxed clients must not re-bind security-context.
            security_context: SecurityContextManagerState::new::<RuntimeState, _>(
                display,
                |client| {
                    client
                        .get_data::<crate::protocol::state::WaylandClientState>()
                        .is_some_and(|data| data.security_context.is_none())
                },
            ),
            foreign_toplevel_list: ForeignToplevelListState::new::<RuntimeState>(display),
            xdg_foreign: XdgForeignState::new::<RuntimeState>(display),
            system_bell: XdgSystemBellState::new::<RuntimeState>(display),
            pointer_warp: PointerWarpManager::new::<RuntimeState>(display),
            content_type: ContentTypeState::new::<RuntimeState>(display),
            alpha_modifier: AlphaModifierState::new::<RuntimeState>(display),
            background_effect: BackgroundEffectState::new::<RuntimeState>(display),
            toplevel_icon: XdgToplevelIconManager::new::<RuntimeState>(display),
            toplevel_tag: XdgToplevelTagManager::new::<RuntimeState>(display),
            fifo: FifoManagerState::new::<RuntimeState>(display),
            commit_timing: CommitTimingManagerState::new::<RuntimeState>(display),
            #[cfg(feature = "xwayland")]
            xwayland_keyboard_grab: XWaylandKeyboardGrabState::new::<RuntimeState>(display),
            virtual_pointer: VirtualPointerManagerState::new::<RuntimeState, _>(
                display,
                unrestricted,
            ),
            gamma_control: GammaControlManagerState::new::<RuntimeState, _>(display, unrestricted),
            ext_workspace: ExtWorkspaceManagerState::new::<RuntimeState, _>(display, unrestricted),
            // Community stopgap until a staging/stable output-management lands.
            output_management: OutputManagementState::new::<RuntimeState, _>(display, unrestricted),
            // ext-image-capture-source + ext-image-copy-capture (prefer over wlr-screencopy).
            image_capture_source: ImageCaptureSourceState::new(),
            output_capture_source: OutputCaptureSourceState::new::<RuntimeState>(display),
            toplevel_capture_source: ToplevelCaptureSourceState::new::<RuntimeState>(display),
            image_copy_capture: ImageCopyCaptureState::new_with_filter::<RuntimeState, _>(
                display,
                unrestricted,
            ),
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
        main_device: u64,
        formats: impl IntoIterator<Item = tensor_host::DrmFormat>,
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

    pub(crate) fn wlr_data_control(&mut self) -> &mut WlrDataControlState {
        &mut self.wlr_data_control
    }

    pub(crate) fn ext_data_control(&mut self) -> &mut ExtDataControlState {
        &mut self.ext_data_control
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

    pub(crate) fn keyboard_shortcuts_inhibit(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.keyboard_shortcuts_inhibit
    }

    pub(crate) fn foreign_toplevel_list(&mut self) -> &mut ForeignToplevelListState {
        &mut self.foreign_toplevel_list
    }

    pub(crate) fn session_lock(&mut self) -> &mut SessionLockManagerState {
        &mut self.session_lock
    }

    pub(crate) fn xdg_foreign(&mut self) -> &mut XdgForeignState {
        &mut self.xdg_foreign
    }

    pub(crate) fn virtual_pointer(&mut self) -> &mut VirtualPointerManagerState {
        &mut self.virtual_pointer
    }

    pub(crate) fn gamma_control(&mut self) -> &mut GammaControlManagerState {
        &mut self.gamma_control
    }

    pub(crate) fn ext_workspace(&mut self) -> &mut ExtWorkspaceManagerState {
        &mut self.ext_workspace
    }

    pub(crate) fn output_management(&mut self) -> &mut OutputManagementState {
        &mut self.output_management
    }

    pub(crate) fn output_capture_source(&mut self) -> &mut OutputCaptureSourceState {
        &mut self.output_capture_source
    }

    pub(crate) fn toplevel_capture_source(&mut self) -> &mut ToplevelCaptureSourceState {
        &mut self.toplevel_capture_source
    }

    pub(crate) fn image_copy_capture(&mut self) -> &mut ImageCopyCaptureState {
        &mut self.image_copy_capture
    }

    pub(crate) fn capabilities(&self) -> ProtocolCapabilities {
        // Link the tier catalog into non-test builds (docs / AGENTS contract).
        let _tier_index = (
            crate::protocol::PROTOCOL_CATALOG.len(),
            crate::protocol::PROTOCOL_CATALOG
                .iter()
                .filter(|entry| entry.tier.preferred_for_new_work())
                .count(),
            crate::protocol::ProtocolTier::Core.as_str(),
            crate::protocol::ProtocolTier::Proprietary.as_str(),
        );
        let _global_owners = (
            &self.viewporter,
            &self.fractional_scale,
            &self.xdg_decoration,
            &self.primary_selection,
            &self.wlr_data_control,
            &self.ext_data_control,
            &self.relative_pointer,
            &self.pointer_gestures,
            &self.pointer_constraints,
            &self.presentation,
            &self.cursor_shape,
            &self.activation,
            &self.idle_notifier,
            &self.idle_inhibit,
            &self.layer_shell,
            &self.single_pixel_buffer,
            &self.keyboard_shortcuts_inhibit,
            &self.tablet,
            &self.text_input,
            &self.input_method,
            &self.virtual_keyboard,
            &self.session_lock,
            &self.security_context,
            &self.foreign_toplevel_list,
            &self.xdg_foreign,
            &self.system_bell,
            &self.pointer_warp,
            &self.content_type,
            &self.alpha_modifier,
            &self.background_effect,
            &self.toplevel_icon,
            &self.toplevel_tag,
            &self.fifo,
            &self.commit_timing,
            &self.virtual_pointer,
            &self.gamma_control,
            &self.ext_workspace,
            &self.output_management,
            &self.image_capture_source,
            &self.output_capture_source,
            &self.toplevel_capture_source,
            &self.image_copy_capture,
        );
        #[cfg(feature = "xwayland")]
        let _xwayland_global_owner = &self.xwayland_keyboard_grab;
        ProtocolCapabilities {
            viewporter: true,
            fractional_scale: true,
            xdg_decoration: true,
            primary_selection: true,
            wlr_data_control: true,
            ext_data_control: true,
            relative_pointer: true,
            pointer_gestures: true,
            pointer_constraints: true,
            presentation_time: true,
            cursor_shape: true,
            xdg_activation: true,
            idle_notify: true,
            idle_inhibit: true,
            layer_shell: true,
            single_pixel_buffer: true,
            keyboard_shortcuts_inhibit: true,
            tablet: true,
            text_input: true,
            input_method: true,
            virtual_keyboard: true,
            session_lock: true,
            security_context: true,
            foreign_toplevel_list: true,
            xdg_foreign: true,
            system_bell: true,
            pointer_warp: true,
            content_type: true,
            alpha_modifier: true,
            background_effect: true,
            toplevel_icon: true,
            toplevel_tag: true,
            fifo: true,
            commit_timing: true,
            xwayland_keyboard_grab: cfg!(feature = "xwayland"),
            virtual_pointer: true,
            gamma_control: true,
            ext_workspace: true,
            output_management: true,
            image_capture_source: true,
            image_copy_capture: true,
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
    pub(crate) wlr_data_control: bool,
    pub(crate) ext_data_control: bool,
    pub(crate) relative_pointer: bool,
    pub(crate) pointer_gestures: bool,
    pub(crate) pointer_constraints: bool,
    pub(crate) presentation_time: bool,
    pub(crate) cursor_shape: bool,
    pub(crate) xdg_activation: bool,
    pub(crate) idle_notify: bool,
    pub(crate) idle_inhibit: bool,
    pub(crate) layer_shell: bool,
    pub(crate) single_pixel_buffer: bool,
    pub(crate) keyboard_shortcuts_inhibit: bool,
    pub(crate) tablet: bool,
    pub(crate) text_input: bool,
    pub(crate) input_method: bool,
    pub(crate) virtual_keyboard: bool,
    pub(crate) session_lock: bool,
    pub(crate) security_context: bool,
    pub(crate) foreign_toplevel_list: bool,
    pub(crate) xdg_foreign: bool,
    pub(crate) system_bell: bool,
    pub(crate) pointer_warp: bool,
    pub(crate) content_type: bool,
    pub(crate) alpha_modifier: bool,
    pub(crate) background_effect: bool,
    pub(crate) toplevel_icon: bool,
    pub(crate) toplevel_tag: bool,
    pub(crate) fifo: bool,
    pub(crate) commit_timing: bool,
    pub(crate) xwayland_keyboard_grab: bool,
    pub(crate) virtual_pointer: bool,
    pub(crate) gamma_control: bool,
    pub(crate) ext_workspace: bool,
    pub(crate) output_management: bool,
    pub(crate) image_capture_source: bool,
    pub(crate) image_copy_capture: bool,
    #[cfg(feature = "tty")]
    pub(crate) linux_dmabuf: bool,
    #[cfg(feature = "tty")]
    pub(crate) linux_drm_syncobj: bool,
    #[cfg(feature = "tty")]
    pub(crate) linux_drm_syncobj_active: bool,
}

#[cfg(test)]
mod tests {
    use calloop::EventLoop;
    use wayland_server::Display;

    use super::*;
    use crate::layout::{LayoutEngine, LayoutKind};

    #[test]
    fn long_lived_globals_are_owned_as_one_capability_set() {
        let event_loop = EventLoop::<RuntimeState>::try_new().unwrap();
        let display = Display::<RuntimeState>::new().unwrap();
        let state = RuntimeState::with_appearance(
            display,
            event_loop.handle(),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            crate::scene::SceneAppearance::default(),
        );

        // Tier catalog stays aligned with advertised desktop surface (docs contract).
        assert!(
            tensor_protocol::catalog_entry("ext-image-copy-capture")
                .is_some_and(|e| e.prefer_over_community)
        );
        assert!(
            tensor_protocol::catalog_entry("wlr-layer-shell")
                .is_some_and(|e| e.tier == crate::protocol::ProtocolTier::Community)
        );
        assert!(crate::protocol::PROTOCOL_CATALOG.iter().any(|e| {
            e.name == "ext-background-effect" && e.tier == crate::protocol::ProtocolTier::StagingExt
        }));

        assert_eq!(
            state.protocol_globals.capabilities(),
            ProtocolCapabilities {
                viewporter: true,
                fractional_scale: true,
                xdg_decoration: true,
                primary_selection: true,
                wlr_data_control: true,
                ext_data_control: true,
                relative_pointer: true,
                pointer_gestures: true,
                pointer_constraints: true,
                presentation_time: true,
                cursor_shape: true,
                xdg_activation: true,
                idle_notify: true,
                idle_inhibit: true,
                layer_shell: true,
                single_pixel_buffer: true,
                keyboard_shortcuts_inhibit: true,
                tablet: true,
                text_input: true,
                input_method: true,
                virtual_keyboard: true,
                session_lock: true,
                security_context: true,
                foreign_toplevel_list: true,
                xdg_foreign: true,
                system_bell: true,
                pointer_warp: true,
                content_type: true,
                alpha_modifier: true,
                background_effect: true,
                toplevel_icon: true,
                toplevel_tag: true,
                fifo: true,
                commit_timing: true,
                xwayland_keyboard_grab: cfg!(feature = "xwayland"),
                virtual_pointer: true,
                gamma_control: true,
                ext_workspace: true,
                output_management: true,
                image_capture_source: true,
                image_copy_capture: true,
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
