use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::mpsc::Receiver;

use fika_core::ThumbnailRequestPriority;

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
    visible: VecDeque<R>,
    deferred: VecDeque<R>,
    queued: HashMap<R::Key, WorkerRequestPriority>,
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
        }
    }
}

impl<R> PriorityWorkerQueue<R>
where
    R: PriorityWorkerRequest,
{
    pub(crate) fn push(&mut self, request: R) {
        let key = request.key().clone();
        match self.queued.get(&key).copied() {
            Some(WorkerRequestPriority::Visible) => {}
            Some(WorkerRequestPriority::Deferred)
                if request.priority() == WorkerRequestPriority::Visible =>
            {
                self.deferred.retain(|queued| queued.key() != &key);
                self.queued.insert(key, WorkerRequestPriority::Visible);
                self.visible.push_back(request);
            }
            Some(WorkerRequestPriority::Deferred) => {}
            None => {
                let priority = request.priority();
                self.queued.insert(key, priority);
                match priority {
                    WorkerRequestPriority::Visible => self.visible.push_back(request),
                    WorkerRequestPriority::Deferred => self.deferred.push_back(request),
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
        let request = self
            .visible
            .pop_front()
            .or_else(|| self.deferred.pop_front())?;
        self.queued.remove(request.key());
        Some(request)
    }
}
