use std::{
    collections::HashSet,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use wayland_server::{DisplayHandle, Resource, Weak, protocol::wl_surface::WlSurface};

use super::{cache::CommitId, tree::PrivateSurfaceData};
use crate::protocol::state::RuntimeState;

pub(in crate::protocol) trait Blocker {
    fn state(&self) -> BlockerState;
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::protocol) enum BlockerState {
    Pending,
    Released,
    Cancelled,
}

#[derive(Clone, Debug)]
pub(in crate::protocol) struct Barrier(Arc<AtomicBool>);

impl PartialEq for Barrier {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Barrier {}

impl Barrier {
    pub(in crate::protocol) fn new(signaled: bool) -> Self {
        Self(Arc::new(AtomicBool::new(signaled)))
    }

    pub(in crate::protocol) fn is_signaled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(in crate::protocol) fn signal(&self) {
        self.0.store(true, Ordering::Release);
    }
}

impl Blocker for Barrier {
    fn state(&self) -> BlockerState {
        if self.is_signaled() {
            BlockerState::Released
        } else {
            BlockerState::Pending
        }
    }
}

#[derive(Default)]
struct TransactionState {
    surfaces: Vec<(Weak<WlSurface>, CommitId)>,
    blockers: Vec<Box<dyn Blocker + Send>>,
}

impl TransactionState {
    fn insert(&mut self, surface: WlSurface, commit: CommitId) {
        if let Some((_, known)) = self
            .surfaces
            .iter_mut()
            .find(|(known, _)| *known == surface)
        {
            *known = (*known).max(commit);
        } else {
            self.surfaces.push((surface.downgrade(), commit));
        }
    }
}

enum TransactionInner {
    Data(TransactionState),
    Fused(Arc<Mutex<TransactionInner>>),
}

pub(super) struct PendingTransaction {
    inner: Arc<Mutex<TransactionInner>>,
}

impl Default for PendingTransaction {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TransactionInner::Data(
                TransactionState::default(),
            ))),
        }
    }
}

impl PendingTransaction {
    fn with_state<T>(&self, apply: impl FnOnce(&mut TransactionState) -> T) -> T {
        let mut current = Arc::clone(&self.inner);
        loop {
            let next = {
                let mut guard = current.lock().unwrap();
                match &mut *guard {
                    TransactionInner::Data(state) => return apply(state),
                    TransactionInner::Fused(next) => Arc::clone(next),
                }
            };
            current = next;
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.with_state(|state| state.surfaces.is_empty() && state.blockers.is_empty())
    }

    pub(super) fn insert(&self, surface: WlSurface, commit: CommitId) {
        self.with_state(|state| state.insert(surface, commit));
    }

    pub(super) fn add_blocker(&self, blocker: impl Blocker + Send + 'static) {
        self.with_state(|state| state.blockers.push(Box::new(blocker)));
    }

    fn is_same_as(&self, other: &Self) -> bool {
        self.with_state(|left| left as *const _) == other.with_state(|right| right as *const _)
    }

    pub(super) fn merge_into(&self, target: &Self) {
        if self.is_same_as(target) {
            return;
        }
        let mut current = Arc::clone(&self.inner);
        let state = loop {
            let next = {
                let mut guard = current.lock().unwrap();
                match &mut *guard {
                    TransactionInner::Data(state) => {
                        let state = std::mem::take(state);
                        *guard = TransactionInner::Fused(Arc::clone(&target.inner));
                        break state;
                    }
                    TransactionInner::Fused(next) => Arc::clone(next),
                }
            };
            current = next;
        };
        target.with_state(|target| {
            for (surface, commit) in state.surfaces {
                if let Ok(surface) = surface.upgrade() {
                    target.insert(surface, commit);
                }
            }
            target.blockers.extend(state.blockers);
        });
    }

    pub(super) fn finalize(mut self) -> Transaction {
        loop {
            let inner = Arc::try_unwrap(self.inner)
                .unwrap_or_else(|_| panic!("pending transaction still has live aliases"))
                .into_inner()
                .unwrap();
            match inner {
                TransactionInner::Data(state) => {
                    return Transaction {
                        surfaces: state.surfaces,
                        blockers: state.blockers,
                    };
                }
                TransactionInner::Fused(next) => self.inner = next,
            }
        }
    }
}

pub(super) struct Transaction {
    surfaces: Vec<(Weak<WlSurface>, CommitId)>,
    blockers: Vec<Box<dyn Blocker + Send>>,
}

impl fmt::Debug for Transaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Transaction")
            .field("surfaces", &self.surfaces.len())
            .field("blockers", &self.blockers.len())
            .finish()
    }
}

impl Transaction {
    fn state(&self) -> BlockerState {
        if !self.surfaces.iter().any(|(surface, _)| surface.is_alive()) {
            return BlockerState::Cancelled;
        }
        self.blockers
            .iter()
            .fold(BlockerState::Released, |state, blocker| {
                match (state, blocker.state()) {
                    (BlockerState::Cancelled, _) | (_, BlockerState::Cancelled) => {
                        BlockerState::Cancelled
                    }
                    (BlockerState::Pending, _) | (_, BlockerState::Pending) => {
                        BlockerState::Pending
                    }
                    (BlockerState::Released, BlockerState::Released) => BlockerState::Released,
                }
            })
    }

    pub(super) fn apply(self, display: &DisplayHandle, state: &mut RuntimeState) {
        for (surface, commit) in self.surfaces {
            let Ok(surface) = surface.upgrade() else {
                continue;
            };
            PrivateSurfaceData::with_states(&surface, |states| {
                states.cached_state.apply(commit, display);
            });
            PrivateSurfaceData::invoke_post_commit_hooks(state, display, &surface);
            state.surface_commit_applied(&surface);
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct TransactionQueue {
    transactions: Vec<Transaction>,
    seen_surfaces: HashSet<u32>,
}

impl TransactionQueue {
    pub(super) fn push(&mut self, transaction: Transaction) {
        self.transactions.push(transaction);
    }

    pub(super) fn take_ready(&mut self) -> Vec<Transaction> {
        let mut ready = Vec::new();
        self.seen_surfaces.clear();
        let mut index = 0;
        while index < self.transactions.len() {
            let mut blocked = match self.transactions[index].state() {
                BlockerState::Cancelled => {
                    self.transactions.remove(index);
                    continue;
                }
                BlockerState::Pending => true,
                BlockerState::Released => false,
            };
            if !blocked {
                blocked = self.transactions[index]
                    .surfaces
                    .iter()
                    .any(|(surface, _)| {
                        surface.is_alive()
                            && self.seen_surfaces.contains(&surface.id().protocol_id())
                    });
            }
            if blocked {
                for (surface, _) in &self.transactions[index].surfaces {
                    if surface.is_alive() {
                        self.seen_surfaces.insert(surface.id().protocol_id());
                    }
                }
                index += 1;
            } else {
                ready.push(self.transactions.remove(index));
            }
        }
        ready
    }
}
