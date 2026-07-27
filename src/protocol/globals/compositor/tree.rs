use std::{
    any::{Any, TypeId},
    fmt,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
};

use wayland_server::{DisplayHandle, Resource, backend::ClientId, protocol::wl_surface::WlSurface};

use super::{
    BufferAssignment, CompositorClientState, SUBSURFACE_ROLE, SurfaceAttributes, SurfaceData,
    cache::{CommitId, MultiCache, SurfaceDataMap},
    transaction::{Blocker, PendingTransaction, TransactionQueue},
    wire::SubsurfaceState,
};
use crate::protocol::state::RuntimeState;

type CommitHook = dyn Fn(&mut dyn Any, &DisplayHandle, &WlSurface) + Send + Sync;
type DestructionHook = dyn Fn(&mut dyn Any, &WlSurface) + Send + Sync;

static NEXT_HOOK_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::protocol) struct HookId(u64);

struct Hook<T: ?Sized> {
    id: HookId,
    callback: Arc<T>,
}

impl<T: ?Sized> Clone for Hook<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            callback: Arc::clone(&self.callback),
        }
    }
}

impl<T: ?Sized> Hook<T> {
    fn new(callback: Arc<T>) -> Self {
        Self {
            id: HookId(NEXT_HOOK_ID.fetch_add(1, Ordering::Relaxed)),
            callback,
        }
    }
}

pub(super) struct SurfaceUserData {
    pub(super) inner: Mutex<PrivateSurfaceData>,
    pub(super) client: ClientId,
    pub(super) client_state: Arc<CompositorClientState>,
    user_state_type: (TypeId, &'static str),
}

impl fmt::Debug for SurfaceUserData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SurfaceUserData")
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

impl SurfaceUserData {
    pub(super) fn new(client: ClientId, client_state: Arc<CompositorClientState>) -> Self {
        Self {
            inner: Mutex::new(PrivateSurfaceData::new()),
            client,
            client_state,
            user_state_type: (
                TypeId::of::<RuntimeState>(),
                std::any::type_name::<RuntimeState>(),
            ),
        }
    }
}

pub(super) struct PrivateSurfaceData {
    parent: Option<WlSurface>,
    children: Vec<WlSurface>,
    public: SurfaceData,
    pending_transaction: PendingTransaction,
    current_commit: CommitId,
    pre_commit_hooks: Vec<Hook<CommitHook>>,
    post_commit_hooks: Vec<Hook<CommitHook>>,
    destruction_hooks: Vec<Hook<DestructionHook>>,
}

impl fmt::Debug for PrivateSurfaceData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrivateSurfaceData")
            .field("parent", &self.parent)
            .field("children", &self.children.len())
            .field("public", &self.public)
            .field("current_commit", &self.current_commit)
            .field("pre_commit_hooks", &self.pre_commit_hooks.len())
            .field("post_commit_hooks", &self.post_commit_hooks.len())
            .field("destruction_hooks", &self.destruction_hooks.len())
            .finish()
    }
}

#[derive(Debug)]
pub(in crate::protocol) struct AlreadyHasRole;

impl fmt::Display for AlreadyHasRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("surface already has a permanent role")
    }
}

impl std::error::Error for AlreadyHasRole {}

#[derive(Clone, Copy)]
pub(super) enum Location {
    Before,
    After,
}

#[derive(Debug)]
pub(in crate::protocol) enum TraversalAction<T> {
    DoChildren(T),
    SkipChildren,
    #[allow(dead_code)]
    Break,
}

impl PrivateSurfaceData {
    fn new() -> Self {
        Self {
            parent: None,
            children: Vec::new(),
            public: SurfaceData {
                role: None,
                data_map: SurfaceDataMap::default(),
                cached_state: MultiCache::new(),
            },
            pending_transaction: PendingTransaction::default(),
            current_commit: CommitId(0),
            pre_commit_hooks: Vec::new(),
            post_commit_hooks: Vec::new(),
            destruction_hooks: Vec::new(),
        }
    }

    pub(super) fn init(surface: &WlSurface) {
        let mut data = Self::lock(surface);
        debug_assert!(data.children.is_empty());
        data.children.push(surface.clone());
    }

    pub(super) fn cleanup(
        state: &mut RuntimeState,
        surface_data: &SurfaceUserData,
        surface: &WlSurface,
    ) {
        let old_parent = surface_data.inner.lock().unwrap().parent.take();
        if let Some(parent) = old_parent
            && let Some(parent_data) = parent.data::<SurfaceUserData>()
        {
            parent_data
                .inner
                .lock()
                .unwrap()
                .children
                .retain(|child| child.id() != surface.id());
        }

        let mut data = surface_data.inner.lock().unwrap();
        for child in data.children.drain(..) {
            let Some(child_data) = child.data::<SurfaceUserData>() else {
                continue;
            };
            if std::ptr::eq(&child_data.inner, &surface_data.inner) {
                continue;
            }
            child_data.inner.lock().unwrap().parent = None;
        }
        let mut attributes = data.public.cached_state.get::<SurfaceAttributes>();
        if let Some(BufferAssignment::NewBuffer(buffer)) = attributes.current().buffer.take() {
            buffer.release();
        }
        if let Some(BufferAssignment::NewBuffer(buffer)) = attributes.pending().buffer.take() {
            buffer.release();
        }
        drop(attributes);
        let hooks = data.destruction_hooks.clone();
        drop(data);
        for hook in hooks {
            (hook.callback)(state, surface);
        }
    }

    fn lock(surface: &WlSurface) -> MutexGuard<'_, Self> {
        surface
            .data::<SurfaceUserData>()
            .expect("wl_surface was not created by Tensor")
            .inner
            .lock()
            .unwrap()
    }

    pub(super) fn with_states<T>(surface: &WlSurface, apply: impl FnOnce(&SurfaceData) -> T) -> T {
        let data = Self::lock(surface);
        apply(&data.public)
    }

    fn set_role(surface: &WlSurface, role: &'static str) -> Result<(), AlreadyHasRole> {
        let mut data = Self::lock(surface);
        if data.public.role.is_some() && data.public.role != Some(role) {
            return Err(AlreadyHasRole);
        }
        data.public.role = Some(role);
        Ok(())
    }

    fn get_role(surface: &WlSurface) -> Option<&'static str> {
        Self::lock(surface).public.role
    }

    fn add_blocker(surface: &WlSurface, blocker: impl Blocker + Send + 'static) {
        Self::lock(surface).pending_transaction.add_blocker(blocker);
    }

    fn add_pre_commit_hook(surface: &WlSurface, callback: Arc<CommitHook>) -> HookId {
        let hook = Hook::new(callback);
        let id = hook.id;
        Self::lock(surface).pre_commit_hooks.push(hook);
        id
    }

    fn add_post_commit_hook(surface: &WlSurface, callback: Arc<CommitHook>) -> HookId {
        let hook = Hook::new(callback);
        let id = hook.id;
        Self::lock(surface).post_commit_hooks.push(hook);
        id
    }

    fn add_destruction_hook(surface: &WlSurface, callback: Arc<DestructionHook>) -> HookId {
        let hook = Hook::new(callback);
        let id = hook.id;
        Self::lock(surface).destruction_hooks.push(hook);
        id
    }

    pub(super) fn invoke_pre_commit_hooks(
        state: &mut RuntimeState,
        display: &DisplayHandle,
        surface: &WlSurface,
    ) {
        let hooks = Self::lock(surface).pre_commit_hooks.clone();
        for hook in hooks {
            (hook.callback)(state, display, surface);
        }
    }

    pub(super) fn invoke_post_commit_hooks(
        state: &mut RuntimeState,
        display: &DisplayHandle,
        surface: &WlSurface,
    ) {
        let hooks = Self::lock(surface).post_commit_hooks.clone();
        for hook in hooks {
            (hook.callback)(state, display, surface);
        }
    }

    fn direct_sync(surface: &WlSurface) -> bool {
        Self::with_states(surface, |states| {
            states
                .data_map
                .get::<SubsurfaceState>()
                .is_some_and(|state| state.sync.load(Ordering::Acquire))
        })
    }

    fn effectively_sync(surface: &WlSurface) -> bool {
        let mut current = surface.clone();
        for _ in 0..256 {
            if Self::direct_sync(&current) {
                return true;
            }
            let Some(parent) = Self::get_parent(&current) else {
                return false;
            };
            current = parent;
        }
        true
    }

    fn commit_sync_tree(
        surface: &WlSurface,
        parent_transaction: &PendingTransaction,
        display: &DisplayHandle,
    ) {
        let mut data = Self::lock(surface);
        for child in &data.children {
            if child.id() != surface.id() {
                Self::commit_sync_tree(child, &data.pending_transaction, display);
            }
        }
        let commit = data.current_commit;
        data.public.cached_state.commit(Some(commit), display);
        data.pending_transaction.insert(surface.clone(), commit);
        let transaction = std::mem::take(&mut data.pending_transaction);
        transaction.merge_into(parent_transaction);
        data.current_commit.0 = data.current_commit.0.wrapping_add(1);
    }

    pub(super) fn commit(surface: &WlSurface, display: &DisplayHandle, state: &mut RuntimeState) {
        let synchronized = Self::effectively_sync(surface);
        let surface_data = surface
            .data::<SurfaceUserData>()
            .expect("wl_surface was not created by Tensor");
        let client_state = Arc::clone(&surface_data.client_state);
        let mut data = surface_data.inner.lock().unwrap();

        for child in &data.children {
            if child.id() == surface.id() {
                continue;
            }
            let child_data = Self::lock(child);
            let child_sync = child_data
                .public
                .data_map
                .get::<SubsurfaceState>()
                .is_some_and(|state| state.sync.load(Ordering::Acquire));
            if !synchronized && child_sync {
                drop(child_data);
                Self::commit_sync_tree(child, &data.pending_transaction, display);
            } else if synchronized || child_sync {
                let transaction = {
                    let mut child_data = child_data;
                    child_data.current_commit.0 = child_data.current_commit.0.wrapping_add(1);
                    std::mem::take(&mut child_data.pending_transaction)
                };
                transaction.merge_into(&data.pending_transaction);
            }
        }

        if !synchronized && data.pending_transaction.is_empty() {
            data.public.cached_state.commit(None, display);
            drop(data);
            Self::invoke_post_commit_hooks(state, display, surface);
            state.surface_commit_applied(surface);
            return;
        }

        let commit = data.current_commit;
        data.public.cached_state.commit(Some(commit), display);
        data.pending_transaction.insert(surface.clone(), commit);
        data.current_commit.0 = data.current_commit.0.wrapping_add(1);
        if synchronized {
            return;
        }

        let transaction = std::mem::take(&mut data.pending_transaction).finalize();
        drop(data);
        let ready = {
            let mut queue = client_state.queue.lock().unwrap();
            let queue = queue.get_or_insert_with(TransactionQueue::default);
            queue.push(transaction);
            queue.take_ready()
        };
        for transaction in ready {
            transaction.apply(display, state);
        }
    }

    fn is_ancestor(ancestor: &WlSurface, surface: &WlSurface) -> bool {
        let mut current = surface.clone();
        for _ in 0..256 {
            let Some(parent) = Self::get_parent(&current) else {
                return false;
            };
            if parent == *ancestor {
                return true;
            }
            current = parent;
        }
        true
    }

    pub(super) fn set_parent(child: &WlSurface, parent: &WlSurface) -> Result<(), AlreadyHasRole> {
        if child == parent || Self::is_ancestor(child, parent) {
            return Err(AlreadyHasRole);
        }
        {
            let mut child_data = Self::lock(child);
            if child_data.public.role.is_some() && child_data.public.role != Some(SUBSURFACE_ROLE) {
                return Err(AlreadyHasRole);
            }
            if child_data.parent.is_some() {
                return Err(AlreadyHasRole);
            }
            child_data.public.role = Some(SUBSURFACE_ROLE);
            child_data.parent = Some(parent.clone());
        }
        Self::lock(parent).children.push(child.clone());
        Ok(())
    }

    pub(super) fn unset_parent(child: &WlSurface) {
        let parent = Self::lock(child).parent.take();
        if let Some(parent) = parent {
            Self::lock(&parent)
                .children
                .retain(|candidate| candidate.id() != child.id());
        }
    }

    fn get_parent(surface: &WlSurface) -> Option<WlSurface> {
        Self::lock(surface).parent.clone()
    }

    pub(super) fn reorder(
        surface: &WlSurface,
        location: Location,
        relative_to: &WlSurface,
    ) -> Result<(), ()> {
        let parent = Self::get_parent(surface).ok_or(())?;
        let mut data = Self::lock(&parent);
        let current = data
            .children
            .iter()
            .position(|candidate| candidate.id() == surface.id())
            .ok_or(())?;
        let mut relative = data
            .children
            .iter()
            .position(|candidate| candidate.id() == relative_to.id())
            .ok_or(())?;
        let surface = data.children.remove(current);
        if current < relative {
            relative -= 1;
        }
        let target = match location {
            Location::Before => relative,
            Location::After => relative + 1,
        };
        data.children.insert(target, surface);
        Ok(())
    }

    fn map<F1, F2, F3, T>(
        surface: &WlSurface,
        initial: &T,
        filter: &mut F1,
        processor: &mut F2,
        post_filter: &mut F3,
        reverse: bool,
    ) -> bool
    where
        F1: FnMut(&WlSurface, &SurfaceData, &T) -> TraversalAction<T>,
        F2: FnMut(&WlSurface, &SurfaceData, &T),
        F3: FnMut(&WlSurface, &SurfaceData, &T) -> bool,
    {
        let data = Self::lock(surface);
        match filter(surface, &data.public, initial) {
            TraversalAction::DoChildren(child_state) => {
                if reverse {
                    for child in data.children.iter().rev() {
                        if child.id() == surface.id() {
                            processor(surface, &data.public, initial);
                        } else if !Self::map(
                            child,
                            &child_state,
                            filter,
                            processor,
                            post_filter,
                            true,
                        ) {
                            return false;
                        }
                    }
                } else {
                    for child in &data.children {
                        if child.id() == surface.id() {
                            processor(surface, &data.public, initial);
                        } else if !Self::map(
                            child,
                            &child_state,
                            filter,
                            processor,
                            post_filter,
                            false,
                        ) {
                            return false;
                        }
                    }
                }
                post_filter(surface, &data.public, initial)
            }
            TraversalAction::SkipChildren => {
                processor(surface, &data.public, initial);
                true
            }
            TraversalAction::Break => false,
        }
    }
}

pub(in crate::protocol) fn with_states<T>(
    surface: &WlSurface,
    apply: impl FnOnce(&SurfaceData) -> T,
) -> T {
    PrivateSurfaceData::with_states(surface, apply)
}

pub(in crate::protocol) fn get_parent(surface: &WlSurface) -> Option<WlSurface> {
    PrivateSurfaceData::get_parent(surface)
}

pub(in crate::protocol) fn get_role(surface: &WlSurface) -> Option<&'static str> {
    PrivateSurfaceData::get_role(surface)
}

pub(in crate::protocol) fn give_role(
    surface: &WlSurface,
    role: &'static str,
) -> Result<(), AlreadyHasRole> {
    PrivateSurfaceData::set_role(surface, role)
}

pub(in crate::protocol) fn is_sync_subsurface(surface: &WlSurface) -> bool {
    PrivateSurfaceData::effectively_sync(surface)
}

pub(in crate::protocol) fn add_blocker(
    surface: &WlSurface,
    blocker: impl Blocker + Send + 'static,
) {
    PrivateSurfaceData::add_blocker(surface, blocker);
}

fn checked_state_type<D: 'static>(surface: &WlSurface) {
    let data = surface
        .data::<SurfaceUserData>()
        .expect("wl_surface was not created by Tensor");
    assert_eq!(
        data.user_state_type.0,
        TypeId::of::<D>(),
        "surface hook state mismatch: {} != {}",
        data.user_state_type.1,
        std::any::type_name::<D>()
    );
}

pub(in crate::protocol) fn add_pre_commit_hook<D: 'static, F>(
    surface: &WlSurface,
    hook: F,
) -> HookId
where
    F: Fn(&mut D, &DisplayHandle, &WlSurface) + Send + Sync + 'static,
{
    checked_state_type::<D>(surface);
    PrivateSurfaceData::add_pre_commit_hook(
        surface,
        Arc::new(move |state, display, surface| {
            hook(state.downcast_mut::<D>().unwrap(), display, surface);
        }),
    )
}

pub(in crate::protocol) fn add_post_commit_hook<D: 'static, F>(
    surface: &WlSurface,
    hook: F,
) -> HookId
where
    F: Fn(&mut D, &DisplayHandle, &WlSurface) + Send + Sync + 'static,
{
    checked_state_type::<D>(surface);
    PrivateSurfaceData::add_post_commit_hook(
        surface,
        Arc::new(move |state, display, surface| {
            hook(state.downcast_mut::<D>().unwrap(), display, surface);
        }),
    )
}

pub(in crate::protocol) fn add_destruction_hook<D: 'static, F>(
    surface: &WlSurface,
    hook: F,
) -> HookId
where
    F: Fn(&mut D, &WlSurface) + Send + Sync + 'static,
{
    checked_state_type::<D>(surface);
    PrivateSurfaceData::add_destruction_hook(
        surface,
        Arc::new(move |state, surface| hook(state.downcast_mut::<D>().unwrap(), surface)),
    )
}

pub(in crate::protocol) fn remove_pre_commit_hook(surface: &WlSurface, hook: &HookId) {
    PrivateSurfaceData::lock(surface)
        .pre_commit_hooks
        .retain(|candidate| candidate.id != *hook);
}

pub(in crate::protocol) fn remove_post_commit_hook(surface: &WlSurface, hook: &HookId) {
    PrivateSurfaceData::lock(surface)
        .post_commit_hooks
        .retain(|candidate| candidate.id != *hook);
}

pub(in crate::protocol) fn remove_destruction_hook(surface: &WlSurface, hook: &HookId) {
    PrivateSurfaceData::lock(surface)
        .destruction_hooks
        .retain(|candidate| candidate.id != *hook);
}

pub(in crate::protocol) fn with_surface_tree_upward<F1, F2, F3, T>(
    surface: &WlSurface,
    initial: T,
    mut filter: F1,
    mut processor: F2,
    mut post_filter: F3,
) where
    F1: FnMut(&WlSurface, &SurfaceData, &T) -> TraversalAction<T>,
    F2: FnMut(&WlSurface, &SurfaceData, &T),
    F3: FnMut(&WlSurface, &SurfaceData, &T) -> bool,
{
    PrivateSurfaceData::map(
        surface,
        &initial,
        &mut filter,
        &mut processor,
        &mut post_filter,
        false,
    );
}

pub(in crate::protocol) fn with_surface_tree_downward<F1, F2, F3, T>(
    surface: &WlSurface,
    initial: T,
    mut filter: F1,
    mut processor: F2,
    mut post_filter: F3,
) where
    F1: FnMut(&WlSurface, &SurfaceData, &T) -> TraversalAction<T>,
    F2: FnMut(&WlSurface, &SurfaceData, &T),
    F3: FnMut(&WlSurface, &SurfaceData, &T) -> bool,
{
    PrivateSurfaceData::map(
        surface,
        &initial,
        &mut filter,
        &mut processor,
        &mut post_filter,
        true,
    );
}
