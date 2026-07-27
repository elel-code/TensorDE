//! Fixed-capacity completion queue for tasks on one Compio runtime thread.

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use futures_util::task::AtomicWaker;

/// A fixed-capacity, single-thread queue that wakes one local consumer.
///
/// Capacity is allocated once during construction. Successful sends and
/// receives do not grow the backing storage, lock a mutex, or cross threads.
pub struct LocalCompletionQueue<T> {
    inner: Rc<QueueInner<T>>,
}

struct QueueInner<T> {
    pending: RefCell<VecDeque<T>>,
    capacity: usize,
    overflowed: Cell<bool>,
    receiver: AtomicWaker,
}

impl<T> Clone for LocalCompletionQueue<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T> LocalCompletionQueue<T> {
    pub fn bounded(capacity: usize) -> Self {
        assert!(
            capacity > 0,
            "local completion queue capacity must be nonzero"
        );
        Self {
            inner: Rc::new(QueueInner {
                pending: RefCell::new(VecDeque::with_capacity(capacity)),
                capacity,
                overflowed: Cell::new(false),
                receiver: AtomicWaker::new(),
            }),
        }
    }

    /// Publish one completion without allocating or blocking.
    pub fn try_send(&self, value: T) -> Result<(), T> {
        let mut pending = self.inner.pending.borrow_mut();
        if pending.len() == self.inner.capacity {
            self.inner.overflowed.set(true);
            drop(pending);
            self.inner.receiver.wake();
            return Err(value);
        }
        pending.push_back(value);
        drop(pending);
        self.inner.receiver.wake();
        Ok(())
    }

    pub fn try_recv(&self) -> Result<Option<T>, LocalQueueOverflow> {
        if self.inner.overflowed.replace(false) {
            return Err(LocalQueueOverflow);
        }
        Ok(self.inner.pending.borrow_mut().pop_front())
    }

    pub fn recv(&self) -> LocalQueueRecv<'_, T> {
        LocalQueueRecv { queue: self }
    }

    #[cfg(test)]
    fn backing_capacity(&self) -> usize {
        self.inner.pending.borrow().capacity()
    }
}

pub struct LocalQueueRecv<'a, T> {
    queue: &'a LocalCompletionQueue<T>,
}

impl<T> Future for LocalQueueRecv<'_, T> {
    type Output = Result<T, LocalQueueOverflow>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.queue.try_recv() {
            Ok(Some(value)) => return Poll::Ready(Ok(value)),
            Err(error) => return Poll::Ready(Err(error)),
            Ok(None) => {}
        }

        self.queue.inner.receiver.register(cx.waker());
        match self.queue.try_recv() {
            Ok(Some(value)) => Poll::Ready(Ok(value)),
            Err(error) => Poll::Ready(Err(error)),
            Ok(None) => Poll::Pending,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("local completion queue exceeded its fixed capacity")]
pub struct LocalQueueOverflow;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_and_receive_reuse_fixed_storage() {
        let runtime = crate::io_uring_runtime(1).unwrap();
        runtime.block_on(async {
            let queue = LocalCompletionQueue::bounded(2);
            let capacity = queue.backing_capacity();

            queue.try_send(1).unwrap();
            assert_eq!(queue.recv().await.unwrap(), 1);
            queue.try_send(2).unwrap();
            assert_eq!(queue.recv().await.unwrap(), 2);
            assert_eq!(queue.backing_capacity(), capacity);
        });
    }

    #[test]
    fn overflow_is_reported_before_queued_values() {
        let queue = LocalCompletionQueue::bounded(1);
        queue.try_send(1).unwrap();
        assert_eq!(queue.try_send(2), Err(2));
        assert_eq!(queue.try_recv(), Err(LocalQueueOverflow));
        assert_eq!(queue.try_recv(), Ok(Some(1)));
    }
}
