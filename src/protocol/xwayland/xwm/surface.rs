use std::sync::{Arc, Mutex};

use tensor_util::{LogicalRect, LogicalSize};
use wayland_server::protocol::wl_surface::WlSurface;
use x11rb::{
    CURRENT_TIME,
    connection::Connection,
    protocol::xproto::{AtomEnum, ConfigureWindowAux, ConnectionExt as _, InputFocus, PropMode},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use super::{
    super::{X11AtomList, X11PropertyTarget, X11SizeHints},
    PropertyQuery,
};

#[derive(Debug)]
struct SurfaceState {
    geometry: LogicalRect<i32>,
    wl_surface: Option<WlSurface>,
    transient_for: Option<u32>,
    min_size: Option<LogicalSize<i32>>,
    max_size: Option<LogicalSize<i32>>,
    override_redirect: bool,
    mapped: bool,
    activated: bool,
    net_state: X11AtomList,
    map_request_in_flight: bool,
    initial_query_pending: bool,
    map_result_ready: bool,
    pending_properties: u8,
    dirty_properties: u8,
    alive: bool,
}

impl SurfaceState {
    fn new(geometry: LogicalRect<i32>, override_redirect: bool) -> Self {
        Self {
            geometry,
            wl_surface: None,
            transient_for: None,
            min_size: None,
            max_size: None,
            override_redirect,
            mapped: false,
            activated: false,
            net_state: X11AtomList::default(),
            map_request_in_flight: false,
            initial_query_pending: false,
            map_result_ready: false,
            pending_properties: 0,
            dirty_properties: 0,
            alive: true,
        }
    }

    fn mark_mapped(&mut self, mapped: bool) {
        self.mapped = mapped;
        self.map_request_in_flight = false;
        self.initial_query_pending = false;
        self.map_result_ready = false;
    }

    fn mark_destroyed(&mut self) {
        self.alive = false;
        self.mapped = false;
        self.wl_surface = None;
        self.map_request_in_flight = false;
        self.initial_query_pending = false;
        self.map_result_ready = false;
        self.pending_properties = 0;
        self.dirty_properties = 0;
    }

    fn begin_map_request(&mut self) -> bool {
        if self.map_request_in_flight {
            return false;
        }
        self.map_request_in_flight = true;
        self.initial_query_pending = true;
        true
    }

    fn cancel_map_request(&mut self) {
        self.map_request_in_flight = false;
        self.initial_query_pending = false;
        self.map_result_ready = false;
    }

    fn schedule_property_query(&mut self, query: PropertyQuery) -> bool {
        let bit = query.bit();
        if self.pending_properties & bit != 0 {
            self.dirty_properties |= bit;
            return false;
        }
        self.pending_properties |= bit;
        true
    }

    fn cancel_property_query(&mut self, query: PropertyQuery) {
        let bit = query.bit();
        self.pending_properties &= !bit;
        self.dirty_properties &= !bit;
    }

    /// `None` is an unsolicited completion; `Some(true)` requests one catch-up query.
    fn complete_property_query(&mut self, query: PropertyQuery) -> Option<bool> {
        let bit = query.bit();
        if self.pending_properties & bit == 0 {
            return None;
        }
        self.pending_properties &= !bit;
        let resubmit = self.dirty_properties & bit != 0;
        self.dirty_properties &= !bit;
        if resubmit {
            self.pending_properties |= bit;
        }
        Some(resubmit)
    }

    fn take_completed_map_request(&mut self) -> bool {
        if !self.map_result_ready || self.pending_properties != 0 {
            return false;
        }
        self.map_result_ready = false;
        true
    }

    fn apply_initial_properties(
        &mut self,
        transient_for: Option<u32>,
        size_hints: X11SizeHints,
        net_state: X11AtomList,
        net_wm_state_focused: u32,
    ) -> bool {
        if !self.initial_query_pending {
            return false;
        }
        self.initial_query_pending = false;
        self.transient_for = transient_for;
        self.min_size = size_hints.min_size;
        self.max_size = size_hints.max_size;
        self.activated = net_state.contains(net_wm_state_focused);
        self.net_state = net_state;
        self.map_result_ready = true;
        self.take_completed_map_request()
    }
}

#[derive(Clone)]
pub(crate) struct X11Surface {
    connection: Arc<RustConnection>,
    window: u32,
    generation: u64,
    state: Arc<Mutex<SurfaceState>>,
    net_wm_state: u32,
    net_wm_state_focused: u32,
}

impl PartialEq for X11Surface {
    fn eq(&self, other: &Self) -> bool {
        self.window == other.window
            && self.generation == other.generation
            && Arc::ptr_eq(&self.connection, &other.connection)
    }
}

impl Eq for X11Surface {}

impl std::fmt::Debug for X11Surface {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("X11Surface")
            .field("window", &self.window)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl X11Surface {
    pub(super) fn new(
        connection: Arc<RustConnection>,
        window: u32,
        generation: u64,
        override_redirect: bool,
        geometry: LogicalRect<i32>,
        net_wm_state: u32,
        net_wm_state_focused: u32,
    ) -> Self {
        Self {
            connection,
            window,
            generation,
            state: Arc::new(Mutex::new(SurfaceState::new(geometry, override_redirect))),
            net_wm_state,
            net_wm_state_focused,
        }
    }

    pub(crate) const fn window_id(&self) -> u32 {
        self.window
    }

    pub(super) const fn property_target(&self) -> X11PropertyTarget {
        X11PropertyTarget {
            window: self.window,
            generation: self.generation,
        }
    }

    pub(crate) fn alive(&self) -> bool {
        self.state.lock().unwrap().alive
    }

    pub(crate) fn is_override_redirect(&self) -> bool {
        self.state.lock().unwrap().override_redirect
    }

    pub(crate) fn geometry(&self) -> LogicalRect<i32> {
        LogicalRect::from_size(self.state.lock().unwrap().geometry.size)
    }

    pub(crate) fn bbox(&self) -> LogicalRect<i32> {
        self.geometry()
    }

    pub(crate) fn last_configure(&self) -> LogicalRect<i32> {
        self.state.lock().unwrap().geometry
    }

    pub(crate) fn update_geometry(&self, geometry: LogicalRect<i32>) {
        self.state.lock().unwrap().geometry = geometry;
    }

    pub(crate) fn wl_surface(&self) -> Option<WlSurface> {
        self.state.lock().unwrap().wl_surface.clone()
    }

    pub(crate) fn set_wl_surface(&self, surface: Option<WlSurface>) {
        self.state.lock().unwrap().wl_surface = surface;
    }

    pub(crate) fn is_transient_for(&self) -> Option<u32> {
        self.state.lock().unwrap().transient_for
    }

    pub(crate) fn min_size(&self) -> Option<LogicalSize<i32>> {
        self.state.lock().unwrap().min_size
    }

    pub(crate) fn max_size(&self) -> Option<LogicalSize<i32>> {
        self.state.lock().unwrap().max_size
    }

    pub(crate) fn is_activated(&self) -> bool {
        self.state.lock().unwrap().activated
    }

    pub(crate) fn set_activated(&self, activated: bool) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = self.state.lock().unwrap();
        if state.activated == activated {
            return Ok(());
        }
        if !state
            .net_state
            .set_member(self.net_wm_state_focused, activated)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "_NET_WM_STATE exceeded Tensor's fixed atom capacity",
            )
            .into());
        }
        state.activated = activated;
        self.connection.change_property32(
            PropMode::REPLACE,
            self.window,
            self.net_wm_state,
            AtomEnum::ATOM,
            state.net_state.as_slice(),
        )?;
        drop(state);
        if activated {
            self.connection
                .set_input_focus(InputFocus::PARENT, self.window, CURRENT_TIME)?;
        }
        self.connection.flush()?;
        Ok(())
    }

    pub(crate) fn set_mapped(&self, mapped: bool) -> Result<(), Box<dyn std::error::Error>> {
        if mapped {
            self.connection.map_window(self.window)?;
        } else {
            self.connection.unmap_window(self.window)?;
        }
        self.mark_mapped(mapped);
        self.connection.flush()?;
        Ok(())
    }

    pub(crate) fn configure(
        &self,
        geometry: Option<LogicalRect<i32>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let geometry = geometry.unwrap_or_else(|| self.last_configure());
        let width = u32::try_from(geometry.size.w.max(1)).unwrap_or(u32::MAX);
        let height = u32::try_from(geometry.size.h.max(1)).unwrap_or(u32::MAX);
        self.connection.configure_window(
            self.window,
            &ConfigureWindowAux::new()
                .x(geometry.loc.x)
                .y(geometry.loc.y)
                .width(width)
                .height(height)
                .border_width(0),
        )?;
        self.update_geometry(geometry);
        self.connection.flush()?;
        Ok(())
    }

    pub(super) fn mark_mapped(&self, mapped: bool) {
        self.state.lock().unwrap().mark_mapped(mapped);
    }

    pub(super) fn mark_destroyed(&self) {
        self.state.lock().unwrap().mark_destroyed();
    }

    pub(super) fn begin_map_request(&self) -> bool {
        self.state.lock().unwrap().begin_map_request()
    }

    pub(crate) fn cancel_map_request(&self) {
        self.state.lock().unwrap().cancel_map_request();
    }

    pub(super) fn schedule_property_query(&self, query: PropertyQuery) -> bool {
        self.state.lock().unwrap().schedule_property_query(query)
    }

    pub(super) fn cancel_property_query(&self, query: PropertyQuery) {
        self.state.lock().unwrap().cancel_property_query(query);
    }

    pub(super) fn complete_property_query(&self, query: PropertyQuery) -> Option<bool> {
        self.state.lock().unwrap().complete_property_query(query)
    }

    pub(super) fn take_completed_map_request(&self) -> bool {
        self.state.lock().unwrap().take_completed_map_request()
    }

    pub(super) fn apply_initial_properties(
        &self,
        transient_for: Option<u32>,
        size_hints: X11SizeHints,
        net_state: X11AtomList,
    ) -> bool {
        self.state.lock().unwrap().apply_initial_properties(
            transient_for,
            size_hints,
            net_state,
            self.net_wm_state_focused,
        )
    }

    pub(super) fn apply_transient_for(&self, transient_for: Option<u32>) {
        self.state.lock().unwrap().transient_for = transient_for;
    }

    pub(super) fn apply_size_hints(&self, size_hints: X11SizeHints) {
        let mut state = self.state.lock().unwrap();
        state.min_size = size_hints.min_size;
        state.max_size = size_hints.max_size;
    }

    pub(super) fn apply_net_state(&self, net_state: X11AtomList) {
        let mut state = self.state.lock().unwrap();
        state.activated = net_state.contains(self.net_wm_state_focused);
        state.net_state = net_state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> SurfaceState {
        SurfaceState::new(LogicalRect::from_size((100, 80).into()), false)
    }

    #[test]
    fn duplicate_map_requests_share_one_initial_query() {
        let mut state = state();
        assert!(state.begin_map_request());
        assert!(!state.begin_map_request());
        state.cancel_map_request();
        assert!(state.begin_map_request());
    }

    #[test]
    fn dirty_property_coalesces_to_one_catch_up_query() {
        let mut state = state();
        assert!(state.schedule_property_query(PropertyQuery::NormalHints));
        assert!(!state.schedule_property_query(PropertyQuery::NormalHints));
        assert!(!state.schedule_property_query(PropertyQuery::NormalHints));
        assert_eq!(
            state.complete_property_query(PropertyQuery::NormalHints),
            Some(true)
        );
        assert_eq!(
            state.complete_property_query(PropertyQuery::NormalHints),
            Some(false)
        );
        assert_eq!(
            state.complete_property_query(PropertyQuery::NormalHints),
            None
        );
    }

    #[test]
    fn map_waits_for_every_catch_up_property_completion() {
        let mut state = state();
        assert!(state.begin_map_request());
        assert!(state.schedule_property_query(PropertyQuery::TransientFor));
        assert!(state.schedule_property_query(PropertyQuery::NetState));
        assert!(!state.apply_initial_properties(
            Some(9),
            X11SizeHints::default(),
            X11AtomList::default(),
            17,
        ));
        assert!(!state.take_completed_map_request());
        assert_eq!(
            state.complete_property_query(PropertyQuery::TransientFor),
            Some(false)
        );
        assert!(!state.take_completed_map_request());
        assert_eq!(
            state.complete_property_query(PropertyQuery::NetState),
            Some(false)
        );
        assert!(state.take_completed_map_request());
        assert!(!state.take_completed_map_request());
    }
}
