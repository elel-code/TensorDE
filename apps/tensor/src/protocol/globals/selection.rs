//! Tensor-owned clipboard, primary-selection, and data-control authority.
//!
//! All four wire protocols share one compositor-thread state machine. Sources
//! and devices are indexed by stable object identities; focus changes touch
//! only the old and new clients, while privileged monitors are visited only
//! when a selection actually changes.

mod core;
mod data_control;
mod device;
mod dnd;
mod offer;
mod primary;

#[cfg(test)]
mod tests;

use std::{
    collections::HashMap,
    os::fd::{AsFd, OwnedFd},
};

use wayland_protocols::{
    ext::data_control::v1::server::{
        ext_data_control_device_v1::ExtDataControlDeviceV1,
        ext_data_control_source_v1::ExtDataControlSourceV1,
    },
    wp::primary_selection::zv1::server::{
        zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
        zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1,
    },
};
use wayland_protocols_wlr::data_control::v1::server::{
    zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
    zwlr_data_control_source_v1::ZwlrDataControlSourceV1,
};
use wayland_server::{
    DisplayHandle, Resource, Weak,
    backend::{ClientId, GlobalId, ObjectId},
    protocol::{wl_data_device::WlDataDevice, wl_data_source::WlDataSource},
};

use self::dnd::{ActiveDnd, DndGrabKind};

const MAX_MIME_TYPES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct SourceToken(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SelectionTarget {
    Clipboard,
    Primary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceKind {
    Core,
    Primary,
    WlrDataControl,
    ExtDataControl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceUse {
    Selection(SelectionTarget),
    Dnd,
}

enum SourceResource {
    Core(Weak<WlDataSource>),
    Primary(Weak<ZwpPrimarySelectionSourceV1>),
    WlrDataControl(Weak<ZwlrDataControlSourceV1>),
    ExtDataControl(Weak<ExtDataControlSourceV1>),
}

impl SourceResource {
    fn cancel(&self) {
        match self {
            Self::Core(source) => source.upgrade().ok().map(|source| source.cancelled()),
            Self::Primary(source) => source.upgrade().ok().map(|source| source.cancelled()),
            Self::WlrDataControl(source) => source.upgrade().ok().map(|source| source.cancelled()),
            Self::ExtDataControl(source) => source.upgrade().ok().map(|source| source.cancelled()),
        };
    }

    fn send(&self, mime_type: String, fd: OwnedFd) {
        match self {
            Self::Core(source) => source
                .upgrade()
                .ok()
                .map(|source| source.send(mime_type, fd.as_fd())),
            Self::Primary(source) => source
                .upgrade()
                .ok()
                .map(|source| source.send(mime_type, fd.as_fd())),
            Self::WlrDataControl(source) => source
                .upgrade()
                .ok()
                .map(|source| source.send(mime_type, fd.as_fd())),
            Self::ExtDataControl(source) => source
                .upgrade()
                .ok()
                .map(|source| source.send(mime_type, fd.as_fd())),
        };
    }
}

struct Source {
    client: ClientId,
    kind: SourceKind,
    resource: SourceResource,
    mime_types: Vec<String>,
    use_: Option<SourceUse>,
    core_actions_set: bool,
    core_actions: wayland_server::protocol::wl_data_device_manager::DndAction,
}

impl Source {
    fn frozen(&self) -> bool {
        self.use_.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OfferMimeError {
    Frozen,
    UnknownSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SetSelectionError {
    NotFocused,
    UnknownSource,
    WrongSource,
    UsedSource,
    DndActions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SetActionsError {
    InvalidMask,
    InvalidSource,
}

pub(crate) struct SelectionProtocol {
    _core_global: GlobalId,
    _primary_global: GlobalId,
    _wlr_data_control_global: GlobalId,
    _ext_data_control_global: GlobalId,
    display: DisplayHandle,
    next_source: u64,
    sources: HashMap<SourceToken, Source>,
    clipboard: Option<SourceToken>,
    primary: Option<SourceToken>,
    focused_client: Option<ClientId>,
    core_devices: HashMap<ClientId, HashMap<ObjectId, Weak<WlDataDevice>>>,
    primary_devices: HashMap<ClientId, HashMap<ObjectId, Weak<ZwpPrimarySelectionDeviceV1>>>,
    wlr_data_control_devices: HashMap<ObjectId, Weak<ZwlrDataControlDeviceV1>>,
    ext_data_control_devices: HashMap<ObjectId, Weak<ExtDataControlDeviceV1>>,
    active_dnd: Option<ActiveDnd>,
}

impl SelectionProtocol {
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        Self {
            _core_global: core::create_global(display),
            _primary_global: primary::create_global(display),
            _wlr_data_control_global: data_control::create_wlr_global(display),
            _ext_data_control_global: data_control::create_ext_global(display),
            display: display.clone(),
            next_source: 1,
            sources: HashMap::new(),
            clipboard: None,
            primary: None,
            focused_client: None,
            core_devices: HashMap::new(),
            primary_devices: HashMap::new(),
            wlr_data_control_devices: HashMap::new(),
            ext_data_control_devices: HashMap::new(),
            active_dnd: None,
        }
    }

    pub(crate) fn set_focus(&mut self, focused: Option<ClientId>) {
        if self.focused_client == focused {
            return;
        }
        if let Some(previous) = self.focused_client.as_ref() {
            self.clear_client_devices(previous);
        }
        self.focused_client = focused;
        if let Some(client) = self.focused_client.as_ref() {
            self.send_client_devices(client);
        }
    }

    pub(super) fn allocate_source(&mut self) -> SourceToken {
        loop {
            let token = SourceToken(self.next_source);
            self.next_source = self.next_source.wrapping_add(1).max(1);
            if !self.sources.contains_key(&token) {
                return token;
            }
        }
    }

    pub(super) fn register_core_source(
        &mut self,
        token: SourceToken,
        client: ClientId,
        source: &WlDataSource,
    ) {
        self.register_source(
            token,
            client,
            SourceKind::Core,
            SourceResource::Core(source.downgrade()),
        );
    }

    pub(super) fn register_primary_source(
        &mut self,
        token: SourceToken,
        client: ClientId,
        source: &ZwpPrimarySelectionSourceV1,
    ) {
        self.register_source(
            token,
            client,
            SourceKind::Primary,
            SourceResource::Primary(source.downgrade()),
        );
    }

    pub(super) fn register_wlr_source(
        &mut self,
        token: SourceToken,
        client: ClientId,
        source: &ZwlrDataControlSourceV1,
    ) {
        self.register_source(
            token,
            client,
            SourceKind::WlrDataControl,
            SourceResource::WlrDataControl(source.downgrade()),
        );
    }

    pub(super) fn register_ext_source(
        &mut self,
        token: SourceToken,
        client: ClientId,
        source: &ExtDataControlSourceV1,
    ) {
        self.register_source(
            token,
            client,
            SourceKind::ExtDataControl,
            SourceResource::ExtDataControl(source.downgrade()),
        );
    }

    fn register_source(
        &mut self,
        token: SourceToken,
        client: ClientId,
        kind: SourceKind,
        resource: SourceResource,
    ) {
        self.sources.insert(
            token,
            Source {
                client,
                kind,
                resource,
                mime_types: Vec::new(),
                use_: None,
                core_actions_set: false,
                core_actions: wayland_server::protocol::wl_data_device_manager::DndAction::empty(),
            },
        );
    }

    pub(super) fn offer_mime(
        &mut self,
        token: SourceToken,
        mime_type: String,
    ) -> Result<(), OfferMimeError> {
        let source = self
            .sources
            .get_mut(&token)
            .ok_or(OfferMimeError::UnknownSource)?;
        if source.frozen() {
            return Err(OfferMimeError::Frozen);
        }
        if source.mime_types.len() < MAX_MIME_TYPES
            && !source.mime_types.iter().any(|known| known == &mime_type)
        {
            source.mime_types.push(mime_type);
        }
        Ok(())
    }

    pub(super) fn set_core_actions(
        &mut self,
        token: SourceToken,
        actions: wayland_server::WEnum<wayland_server::protocol::wl_data_device_manager::DndAction>,
    ) -> Result<(), SetActionsError> {
        let source = self
            .sources
            .get_mut(&token)
            .ok_or(SetActionsError::InvalidSource)?;
        if source.kind != SourceKind::Core || source.frozen() || source.core_actions_set {
            return Err(SetActionsError::InvalidSource);
        }
        let actions = actions
            .into_result()
            .map_err(|_| SetActionsError::InvalidMask)?;
        source.core_actions = actions;
        source.core_actions_set = true;
        Ok(())
    }

    pub(super) fn set_core_selection(
        &mut self,
        client: &ClientId,
        source: Option<SourceToken>,
    ) -> Result<(), SetSelectionError> {
        self.set_focused_selection(client, source, SourceKind::Core, SelectionTarget::Clipboard)
    }

    pub(super) fn set_primary_selection(
        &mut self,
        client: &ClientId,
        source: Option<SourceToken>,
    ) -> Result<(), SetSelectionError> {
        self.set_focused_selection(
            client,
            source,
            SourceKind::Primary,
            SelectionTarget::Primary,
        )
    }

    pub(super) fn set_wlr_selection(
        &mut self,
        client: &ClientId,
        source: Option<SourceToken>,
        target: SelectionTarget,
    ) -> Result<(), SetSelectionError> {
        self.set_control_selection(client, source, SourceKind::WlrDataControl, target)
    }

    pub(super) fn set_ext_selection(
        &mut self,
        client: &ClientId,
        source: Option<SourceToken>,
        target: SelectionTarget,
    ) -> Result<(), SetSelectionError> {
        self.set_control_selection(client, source, SourceKind::ExtDataControl, target)
    }

    fn set_focused_selection(
        &mut self,
        client: &ClientId,
        source: Option<SourceToken>,
        kind: SourceKind,
        target: SelectionTarget,
    ) -> Result<(), SetSelectionError> {
        if self.focused_client.as_ref() != Some(client) {
            return Err(SetSelectionError::NotFocused);
        }
        if let Some(token) = source {
            let record = self
                .sources
                .get_mut(&token)
                .ok_or(SetSelectionError::UnknownSource)?;
            if &record.client != client || record.kind != kind {
                return Err(SetSelectionError::WrongSource);
            }
            if record.use_.is_some() {
                return Err(SetSelectionError::UsedSource);
            }
            if kind == SourceKind::Core && record.core_actions_set {
                return Err(SetSelectionError::DndActions);
            }
            record.use_ = Some(SourceUse::Selection(target));
        }
        self.replace_selection(target, source);
        Ok(())
    }

    fn set_control_selection(
        &mut self,
        client: &ClientId,
        source: Option<SourceToken>,
        kind: SourceKind,
        target: SelectionTarget,
    ) -> Result<(), SetSelectionError> {
        if let Some(token) = source {
            let record = self
                .sources
                .get_mut(&token)
                .ok_or(SetSelectionError::UnknownSource)?;
            if &record.client != client || record.kind != kind {
                return Err(SetSelectionError::WrongSource);
            }
            if record.use_.is_some() {
                return Err(SetSelectionError::UsedSource);
            }
            record.use_ = Some(SourceUse::Selection(target));
        }
        self.replace_selection(target, source);
        Ok(())
    }

    fn replace_selection(&mut self, target: SelectionTarget, source: Option<SourceToken>) {
        let slot = match target {
            SelectionTarget::Clipboard => &mut self.clipboard,
            SelectionTarget::Primary => &mut self.primary,
        };
        if *slot == source {
            return;
        }
        let old = std::mem::replace(slot, source);
        if let Some(old) = old.and_then(|token| self.sources.get(&token)) {
            old.resource.cancel();
        }
        self.broadcast_selection(target);
    }

    pub(super) fn receive(
        &self,
        client: &ClientId,
        source: SourceToken,
        target: SelectionTarget,
        focused_only: bool,
        mime_type: String,
        fd: OwnedFd,
    ) {
        if self.selection(target) != Some(source)
            || (focused_only && self.focused_client.as_ref() != Some(client))
        {
            return;
        }
        let Some(source) = self.sources.get(&source) else {
            return;
        };
        if source.mime_types.iter().any(|known| known == &mime_type) {
            source.resource.send(mime_type, fd);
        }
    }

    fn source_destroyed(&mut self, token: SourceToken) -> Option<DndGrabKind> {
        self.sources.remove(&token)?;
        let dnd = self
            .active_dnd
            .as_ref()
            .filter(|dnd| dnd.source_token == Some(token))
            .map(|dnd| dnd.kind);
        let clipboard = self.clipboard == Some(token);
        let primary = self.primary == Some(token);
        if clipboard {
            self.clipboard = None;
            self.broadcast_selection(SelectionTarget::Clipboard);
        }
        if primary {
            self.primary = None;
            self.broadcast_selection(SelectionTarget::Primary);
        }
        dnd
    }

    #[cfg(test)]
    pub(super) fn counts(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.sources.len(),
            self.core_devices.values().map(HashMap::len).sum(),
            self.primary_devices.values().map(HashMap::len).sum(),
            self.wlr_data_control_devices.len(),
            self.ext_data_control_devices.len(),
        )
    }
}
