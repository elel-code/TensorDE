use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::mpsc::Receiver;

use tensor_files_core::ThumbnailRequestPriority;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkerRequestPriority {
    Deferred,
    Visible,
}

impl From<ThumbnailRequestPriority> for WorkerRequestPriority {
    fn from(priority: ThumbnailRequestPriority) -> Self {
        match priority {
            ThumbnailRequestPriority::Visible => Self::Visible,
            ThumbnailRequestPriority::Deferred => Self::Deferred,
        }
    }
}

pub(crate) trait PriorityWorkerRequest {
    type Key: Clone + Eq + Hash;

    fn key(&self) -> &Self::Key;
    fn priority(&self) -> WorkerRequestPriority;
}

pub(crate) struct PriorityWorkerQueue<R>
where
    R: PriorityWorkerRequest,
{
    visible: VecDeque<QueuedRequest<R>>,
    deferred: VecDeque<QueuedRequest<R>>,
    queued: HashMap<R::Key, QueuedState>,
    next_generation: u64,
    stale_deferred: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct QueuedState {
    priority: WorkerRequestPriority,
    generation: u64,
}

struct QueuedRequest<R> {
    request: R,
    generation: u64,
}

impl<R> Default for PriorityWorkerQueue<R>
where
    R: PriorityWorkerRequest,
{
    fn default() -> Self {
        Self {
            visible: VecDeque::new(),
            deferred: VecDeque::new(),
            queued: HashMap::new(),
            next_generation: 0,
            stale_deferred: 0,
        }
    }
}

impl<R> PriorityWorkerQueue<R>
where
    R: PriorityWorkerRequest,
{
    pub(crate) fn push(&mut self, request: R) {
        let key = request.key().clone();
        let priority = request.priority();
        match self.queued.get(&key).copied() {
            Some(QueuedState {
                priority: WorkerRequestPriority::Visible,
                ..
            }) => {}
            Some(QueuedState {
                priority: WorkerRequestPriority::Deferred,
                ..
            }) if priority == WorkerRequestPriority::Visible => {
                let generation = self.next_generation();
                self.queued.insert(
                    key,
                    QueuedState {
                        priority,
                        generation,
                    },
                );
                self.stale_deferred += 1;
                self.visible.push_back(QueuedRequest {
                    request,
                    generation,
                });
                self.compact_deferred_if_sparse();
            }
            Some(QueuedState {
                priority: WorkerRequestPriority::Deferred,
                ..
            }) => {}
            None => {
                let generation = self.next_generation();
                self.queued.insert(
                    key,
                    QueuedState {
                        priority,
                        generation,
                    },
                );
                match priority {
                    WorkerRequestPriority::Visible => self.visible.push_back(QueuedRequest {
                        request,
                        generation,
                    }),
                    WorkerRequestPriority::Deferred => self.deferred.push_back(QueuedRequest {
                        request,
                        generation,
                    }),
                }
            }
        }
    }

    pub(crate) fn next_request(&mut self, request_rx: &Receiver<R>) -> Option<R> {
        loop {
            while let Ok(request) = request_rx.try_recv() {
                self.push(request);
            }

            if let Some(request) = self.pop_ready() {
                return Some(request);
            }

            match request_rx.recv() {
                Ok(request) => self.push(request),
                Err(_) => return None,
            }
        }
    }

    pub(crate) fn pop_ready(&mut self) -> Option<R> {
        loop {
            let (queued, from_deferred) = if let Some(queued) = self.visible.pop_front() {
                (queued, false)
            } else {
                (self.deferred.pop_front()?, true)
            };
            let key = queued.request.key();
            let Some(state) = self.queued.get(key).copied() else {
                if from_deferred {
                    self.stale_deferred = self.stale_deferred.saturating_sub(1);
                }
                continue;
            };
            if state.generation != queued.generation || state.priority != queued.request.priority()
            {
                if from_deferred {
                    self.stale_deferred = self.stale_deferred.saturating_sub(1);
                }
                continue;
            }
            self.queued.remove(key);
            return Some(queued.request);
        }
    }

    fn compact_deferred_if_sparse(&mut self) {
        if self.stale_deferred == 0
            || self.stale_deferred.saturating_mul(2) <= self.deferred.len().max(1)
        {
            return;
        }
        let queued = &self.queued;
        self.deferred.retain(|entry| {
            queued
                .get(entry.request.key())
                .is_some_and(|state| state.generation == entry.generation)
        });
        self.stale_deferred = 0;
    }

    fn next_generation(&mut self) -> u64 {
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        generation
    }
}
