#[cfg(feature = "xwayland")]
use smithay::wayland::xwayland_keyboard_grab::XWaylandKeyboardGrabState;
use smithay::{
    utils::{ClockSource, Monotonic},
    wayland::{
        cursor_shape::CursorShapeManagerState,
        input_method::InputMethodManagerState,
        pointer_constraints::PointerConstraintsState,
        pointer_gestures::PointerGesturesState,
        selection::{
            ext_data_control::DataControlState as ExtDataControlState,
            primary_selection::PrimarySelectionState,
            wlr_data_control::DataControlState as WlrDataControlState,
        },
        session_lock::SessionLockManagerState,
        shell::wlr_layer::WlrLayerShellState,
        tablet_manager::TabletManagerState,
        text_input::TextInputManagerState,
        virtual_keyboard::VirtualKeyboardManagerState,
        xdg_activation::XdgActivationState,
        xdg_foreign::XdgForeignState,
    },
};
use wayland_server::{Client, DisplayHandle};

use super::extensions::{
    ext_workspace::ExtWorkspaceManagerState, gamma_control::GammaControlManagerState,
    output_management::OutputManagementState, security_context::SecurityContextManagerState,
    virtual_pointer::VirtualPointerManagerState,
};
use super::state::RuntimeState;

pub(in crate::protocol) mod background_effect;
pub(in crate::protocol) mod desktop_controls;
#[cfg(feature = "tty")]
pub(in crate::protocol) mod dmabuf;
pub(in crate::protocol) mod foreign_toplevel;
pub(in crate::protocol) mod fractional_scale;
pub(in crate::protocol) mod idle_inhibit;
pub(in crate::protocol) mod idle_notify;
pub(in crate::protocol) mod image_capture_source;
pub(in crate::protocol) mod image_copy_capture;
pub(in crate::protocol) mod output;
pub(in crate::protocol) mod presentation;
pub(in crate::protocol) mod relative_pointer;
pub(in crate::protocol) mod shm;
pub(in crate::protocol) mod shortcut_inhibit;
pub(in crate::protocol) mod single_pixel_buffer;
pub(in crate::protocol) mod surface_metadata;
pub(in crate::protocol) mod surface_timing;
#[cfg(feature = "tty")]
mod syncobj;
pub(in crate::protocol) mod viewporter;
pub(in crate::protocol) mod xdg_decoration;

use background_effect::BackgroundEffectProtocol;
use desktop_controls::DesktopControls;
#[cfg(feature = "tty")]
use dmabuf::DmabufProtocol;
use foreign_toplevel::ForeignToplevelListState;
use fractional_scale::FractionalScaleProtocol;
use idle_inhibit::IdleInhibitProtocol;
use idle_notify::IdleNotifyProtocol;
use image_capture_source::ImageCaptureSourceProtocol;
use image_copy_capture::ImageCopyCaptureProtocol;
use output::OutputProtocol;
use presentation::PresentationProtocol;
use relative_pointer::RelativePointerProtocol;
use shm::ShmProtocol;
use shortcut_inhibit::ShortcutInhibitProtocol;
use single_pixel_buffer::SinglePixelBufferProtocol;
use surface_metadata::SurfaceMetadataProtocol;
use surface_timing::{SurfaceBarrier, SurfaceTimingProtocol};
#[cfg(feature = "tty")]
pub(crate) use syncobj::DrmSyncPoint;
#[cfg(feature = "tty")]
use syncobj::DrmSyncobjProtocol;
#[cfg(feature = "tty")]
pub(super) use syncobj::{DrmSyncobjCachedState, DrmSyncobjHandler, DrmSyncobjState};
use viewporter::ViewporterProtocol;
use xdg_decoration::XdgDecorationProtocol;

pub(crate) struct ProtocolGlobals {
    shm: ShmProtocol,
    output: OutputProtocol,
    viewporter: ViewporterProtocol,
    fractional_scale: FractionalScaleProtocol,
    xdg_decoration: XdgDecorationProtocol,
    primary_selection: PrimarySelectionState,
    wlr_data_control: WlrDataControlState,
    ext_data_control: ExtDataControlState,
    pub(super) relative_pointer: RelativePointerProtocol,
    pointer_gestures: PointerGesturesState,
    pointer_constraints: PointerConstraintsState,
    presentation: PresentationProtocol,
    cursor_shape: CursorShapeManagerState,
    activation: XdgActivationState,
    pub(super) idle_notify: IdleNotifyProtocol,
    idle_inhibit: IdleInhibitProtocol,
    layer_shell: WlrLayerShellState,
    single_pixel_buffer: SinglePixelBufferProtocol,
    shortcut_inhibit: ShortcutInhibitProtocol,
    tablet: TabletManagerState,
    text_input: TextInputManagerState,
    input_method: InputMethodManagerState,
    virtual_keyboard: VirtualKeyboardManagerState,
    session_lock: SessionLockManagerState,
    security_context: SecurityContextManagerState,
    foreign_toplevel_list: ForeignToplevelListState,
    xdg_foreign: XdgForeignState,
    pub(super) desktop_controls: DesktopControls,
    pub(super) surface_metadata: SurfaceMetadataProtocol,
    background_effect: BackgroundEffectProtocol,
    pub(super) surface_timing: SurfaceTimingProtocol,
    #[cfg(feature = "xwayland")]
    xwayland_keyboard_grab: XWaylandKeyboardGrabState,
    virtual_pointer: VirtualPointerManagerState,
    gamma_control: GammaControlManagerState,
    ext_workspace: ExtWorkspaceManagerState,
    output_management: OutputManagementState,
    image_capture_source: ImageCaptureSourceProtocol,
    image_copy_capture: ImageCopyCaptureProtocol,
    #[cfg(feature = "tty")]
    dmabuf: DmabufProtocol,
    #[cfg(feature = "tty")]
    syncobj: DrmSyncobjProtocol,
}

impl ProtocolGlobals {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
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
            shm: ShmProtocol::new(display),
            output: OutputProtocol::new(display),
            viewporter: ViewporterProtocol::new(display),
            fractional_scale: FractionalScaleProtocol::new(display),
            xdg_decoration: XdgDecorationProtocol::new(display),
            primary_selection,
            wlr_data_control,
            ext_data_control,
            relative_pointer: RelativePointerProtocol::new(display),
            pointer_gestures: PointerGesturesState::new::<RuntimeState>(display),
            pointer_constraints: PointerConstraintsState::new::<RuntimeState>(display),
            presentation: PresentationProtocol::new(display, Monotonic::ID as u32),
            cursor_shape: CursorShapeManagerState::new::<RuntimeState>(display),
            activation: XdgActivationState::new::<RuntimeState>(display),
            idle_notify: IdleNotifyProtocol::new(display),
            idle_inhibit: IdleInhibitProtocol::new(display),
            layer_shell: WlrLayerShellState::new::<RuntimeState>(display),
            single_pixel_buffer: SinglePixelBufferProtocol::new(display),
            shortcut_inhibit: ShortcutInhibitProtocol::new(display),
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
            desktop_controls: DesktopControls::new(display),
            surface_metadata: SurfaceMetadataProtocol::new(display),
            background_effect: BackgroundEffectProtocol::new(display),
            surface_timing: SurfaceTimingProtocol::new(display),
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
            image_capture_source: ImageCaptureSourceProtocol::new(display, unrestricted),
            image_copy_capture: ImageCopyCaptureProtocol::new(display, unrestricted),
            #[cfg(feature = "tty")]
            dmabuf: DmabufProtocol::new(),
            #[cfg(feature = "tty")]
            syncobj: DrmSyncobjProtocol::new(),
        }
    }

    pub(crate) const fn xdg_output_enabled(&self) -> bool {
        self.output.xdg_output_enabled()
    }

    pub(crate) fn set_preferred_fractional_scale(
        &self,
        surface: &wayland_server::protocol::wl_surface::WlSurface,
        scale: tensor_util::OutputScale,
    ) {
        self.fractional_scale.set_preferred_scale(surface, scale);
    }

    pub(in crate::protocol) fn remove_surface(
        &mut self,
        surface: &wayland_server::protocol::wl_surface::WlSurface,
    ) -> Vec<SurfaceBarrier> {
        self.fractional_scale.remove_surface(surface);
        self.surface_metadata.remove_surface(surface);
        self.desktop_controls.remove_surface(surface);
        self.shortcut_inhibit.remove_surface(surface);
        if self.idle_inhibit.remove_surface(surface) {
            self.idle_notify.set_inhibited(false);
        }
        self.surface_timing.remove_surface(surface)
    }

    pub(crate) fn committed_background_has_area(
        &self,
        surface: &wayland_server::protocol::wl_surface::WlSurface,
    ) -> bool {
        self.surface_metadata.committed_background_has_area(surface)
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
    pub(crate) fn update_syncobj(
        &mut self,
        display: &DisplayHandle,
        device: Option<crate::backend::DrmDeviceFd>,
    ) {
        self.syncobj.update(display, device);
    }

    #[cfg(feature = "tty")]
    pub(super) fn drm_syncobj_state(&mut self) -> Option<&mut DrmSyncobjState> {
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

    pub(crate) fn layer_shell(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell
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

    pub(crate) fn xdg_toplevel_destroyed(
        &mut self,
        toplevel: &wayland_protocols::xdg::shell::server::xdg_toplevel::XdgToplevel,
    ) {
        self.xdg_decoration.toplevel_destroyed(toplevel);
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
            &self.shm,
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
            &self.idle_notify,
            &self.idle_inhibit,
            &self.layer_shell,
            &self.single_pixel_buffer,
            &self.shortcut_inhibit,
            &self.tablet,
            &self.text_input,
            &self.input_method,
            &self.virtual_keyboard,
            &self.session_lock,
            &self.security_context,
            &self.foreign_toplevel_list,
            &self.xdg_foreign,
            &self.desktop_controls,
            &self.surface_metadata,
            &self.background_effect,
            &self.surface_timing,
            &self.virtual_pointer,
            &self.gamma_control,
            &self.ext_workspace,
            &self.output_management,
            &self.image_capture_source,
            &self.image_copy_capture,
        );
        #[cfg(feature = "xwayland")]
        let _xwayland_global_owner = &self.xwayland_keyboard_grab;
        ProtocolCapabilities {
            shm: true,
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
            idle_notify: self.idle_notify.advertised(),
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
            commit_timing: self.surface_timing.commit_timing_advertised(),
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
    pub(crate) shm: bool,
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
    use wayland_server::Display;

    use super::*;
    use crate::layout::{LayoutEngine, LayoutKind};

    #[test]
    fn long_lived_globals_are_owned_as_one_capability_set() {
        let display = Display::<RuntimeState>::new().unwrap();
        let state = RuntimeState::with_appearance(
            display,
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
                shm: true,
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
