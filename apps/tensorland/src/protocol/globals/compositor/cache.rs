use std::{
    any::Any,
    collections::VecDeque,
    sync::{Mutex, MutexGuard},
};

use appendlist::AppendList;
use wayland_server::DisplayHandle;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct CommitId(pub(super) u64);

pub(in crate::protocol) trait Cacheable: Default + Send + 'static {
    fn commit(&mut self, display: &DisplayHandle) -> Self;
    fn merge_into(self, current: &mut Self, display: &DisplayHandle);
}

#[derive(Debug)]
pub(in crate::protocol) struct CachedState<T> {
    pending: T,
    cache: VecDeque<(CommitId, T)>,
    current: T,
}

impl<T: Default> Default for CachedState<T> {
    fn default() -> Self {
        Self {
            pending: T::default(),
            cache: VecDeque::new(),
            current: T::default(),
        }
    }
}

impl<T> CachedState<T> {
    pub(in crate::protocol) fn current(&mut self) -> &mut T {
        &mut self.current
    }

    pub(in crate::protocol) fn pending(&mut self) -> &mut T {
        &mut self.pending
    }
}

trait Cache: Any + Send {
    fn as_any(&self) -> &dyn Any;
    fn commit(&self, commit: Option<CommitId>, display: &DisplayHandle);
    fn apply(&self, commit: CommitId, display: &DisplayHandle);
}

impl<T: Cacheable> Cache for Mutex<CachedState<T>> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn commit(&self, commit: Option<CommitId>, display: &DisplayHandle) {
        let mut cached = self.lock().unwrap();
        let update = cached.pending.commit(display);
        if let Some(commit) = commit {
            if let Some((last, state)) = cached.cache.back_mut()
                && *last == commit
            {
                update.merge_into(state, display);
            } else {
                cached.cache.push_back((commit, update));
            }
        } else {
            let queued = std::mem::take(&mut cached.cache);
            for (_, state) in queued {
                state.merge_into(&mut cached.current, display);
            }
            update.merge_into(&mut cached.current, display);
        }
    }

    fn apply(&self, commit: CommitId, display: &DisplayHandle) {
        let mut cached = self.lock().unwrap();
        while cached
            .cache
            .front()
            .is_some_and(|(queued, _)| *queued <= commit)
        {
            let (_, state) = cached.cache.pop_front().unwrap();
            state.merge_into(&mut cached.current, display);
        }
    }
}

pub(in crate::protocol) struct MultiCache {
    caches: AppendList<Box<dyn Cache>>,
}

impl std::fmt::Debug for MultiCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("MultiCache").finish_non_exhaustive()
    }
}

impl MultiCache {
    pub(super) fn new() -> Self {
        Self {
            caches: AppendList::new(),
        }
    }

    fn find_or_insert<T: Cacheable>(&self) -> &Mutex<CachedState<T>> {
        for cache in &self.caches {
            if let Some(cache) = cache.as_any().downcast_ref::<Mutex<CachedState<T>>>() {
                return cache;
            }
        }
        self.caches
            .push(Box::new(Mutex::new(CachedState::<T>::default())));
        self.caches[self.caches.len() - 1]
            .as_any()
            .downcast_ref()
            .unwrap()
    }

    pub(in crate::protocol) fn get<T: Cacheable>(&self) -> MutexGuard<'_, CachedState<T>> {
        self.find_or_insert::<T>().lock().unwrap()
    }

    pub(in crate::protocol) fn has<T: Cacheable>(&self) -> bool {
        self.caches
            .iter()
            .any(|cache| cache.as_any().is::<Mutex<CachedState<T>>>())
    }

    pub(super) fn commit(&self, commit: Option<CommitId>, display: &DisplayHandle) {
        for cache in &self.caches {
            cache.commit(commit, display);
        }
    }

    pub(super) fn apply(&self, commit: CommitId, display: &DisplayHandle) {
        for cache in &self.caches {
            cache.apply(commit, display);
        }
    }
}

pub(in crate::protocol) struct SurfaceDataMap {
    entries: AppendList<Box<dyn Any + Send>>,
}

impl Default for SurfaceDataMap {
    fn default() -> Self {
        Self {
            entries: AppendList::new(),
        }
    }
}

impl std::fmt::Debug for SurfaceDataMap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SurfaceDataMap")
            .finish_non_exhaustive()
    }
}

impl SurfaceDataMap {
    pub(in crate::protocol) fn get<T: Send + 'static>(&self) -> Option<&T> {
        self.entries
            .iter()
            .find_map(|entry| entry.downcast_ref::<T>())
    }

    pub(in crate::protocol) fn insert_if_missing_threadsafe<T, F>(&self, create: F) -> bool
    where
        T: Send + 'static,
        F: FnOnce() -> T,
    {
        self.insert_if_missing(create)
    }

    pub(in crate::protocol) fn insert_if_missing<T, F>(&self, create: F) -> bool
    where
        T: Send + 'static,
        F: FnOnce() -> T,
    {
        if self.get::<T>().is_some() {
            return false;
        }
        self.entries.push(Box::new(create()));
        true
    }

    pub(in crate::protocol) fn get_or_insert<T, F>(&self, create: F) -> &T
    where
        T: Send + 'static,
        F: FnOnce() -> T,
    {
        if self.get::<T>().is_none() {
            self.entries.push(Box::new(create()));
        }
        self.get::<T>().unwrap()
    }
}
