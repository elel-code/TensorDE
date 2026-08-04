use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use async_channel::Sender;
use futures_channel::oneshot;

use super::file_ops::TransferProgress;
use super::operations::Operation;

type CompioFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;
type CompioTask = Box<dyn FnOnce() -> CompioFuture + Send + 'static>;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperationRuntimeError {
    CompioThreadStart(String),
    Stopped,
    ResultDropped,
    UnknownOperation(OperationId),
    BlockingWorkerStopped,
}

impl fmt::Display for OperationRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CompioThreadStart(err) => {
                write!(f, "failed to start operation Compio thread: {err}")
            }
            Self::Stopped => write!(f, "operation runtime stopped"),
            Self::ResultDropped => write!(f, "operation runtime stopped before returning a result"),
            Self::UnknownOperation(id) => write!(f, "unknown operation id {}", id.0),
            Self::BlockingWorkerStopped => write!(f, "operation blocking worker stopped"),
        }
    }
}

impl std::error::Error for OperationRuntimeError {}

#[derive(Clone, Debug)]
pub struct OperationController {
    state: Arc<OperationControllerState>,
}

#[derive(Debug)]
struct OperationControllerState {
    cancel: AtomicBool,
    paused: AtomicBool,
    progress: Mutex<TransferProgress>,
}

impl OperationController {
    pub fn new() -> Self {
        Self {
            state: Arc::new(OperationControllerState {
                cancel: AtomicBool::new(false),
                paused: AtomicBool::new(false),
                progress: Mutex::new(TransferProgress::default()),
            }),
        }
    }

    pub fn cancel(&self) {
        self.state.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.cancel.load(Ordering::Relaxed)
    }

    pub fn pause(&self) {
        self.state.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.state.paused.store(false, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.state.paused.load(Ordering::Relaxed)
    }

    pub fn set_progress(&self, progress: TransferProgress) {
        if let Ok(mut state) = self.state.progress.lock() {
            *state = progress;
        }
    }

    pub fn progress(&self) -> TransferProgress {
        self.state
            .progress
            .lock()
            .map(|progress| *progress)
            .unwrap_or_default()
    }
}

impl Default for OperationController {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct OperationHandle {
    pub id: OperationId,
    pub operation: Operation,
    pub controller: OperationController,
    pub started_at: Instant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationSnapshot {
    pub id: OperationId,
    pub operation: Operation,
    pub started_at: Instant,
    pub progress: TransferProgress,
    pub cancelled: bool,
    pub paused: bool,
}

pub struct OperationSubmission<T> {
    pub id: OperationId,
    pub controller: OperationController,
    pub result_rx: oneshot::Receiver<T>,
}

/// Compio-only async operation dispatcher.
///
/// File I/O and long-running tasks run on a dedicated Compio thread. Callers on
/// the UI thread should prefer the non-async `spawn_*` helpers so they do not
/// need an intermediate `thread::spawn` + `block_on` wrapper.
pub struct OperationRuntime {
    compio_tx: Sender<CompioTask>,
    next_operation_id: AtomicU64,
    operations: Mutex<BTreeMap<OperationId, OperationHandle>>,
}

impl OperationRuntime {
    fn new() -> Result<Self, OperationRuntimeError> {
        // Unbounded: UI submit paths must not block waiting for the Compio thread
        // to accept work (bounded(1) previously forced intermediate worker threads).
        let (compio_tx, compio_rx) = async_channel::unbounded::<CompioTask>();

        std::thread::Builder::new()
            .name("tensor-files-operation-compio".to_string())
            .spawn(move || {
                let Ok(runtime) = compio::runtime::RuntimeBuilder::new().build() else {
                    return;
                };
                runtime.block_on(async move {
                    while let Ok(task) = compio_rx.recv().await {
                        compio::runtime::spawn(task()).detach();
                    }
                });
            })
            .map_err(|err| OperationRuntimeError::CompioThreadStart(err.to_string()))?;

        Ok(Self {
            compio_tx,
            next_operation_id: AtomicU64::new(1),
            operations: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn shared() -> Result<&'static Self, OperationRuntimeError> {
        static OPERATION_RUNTIME: OnceLock<Result<OperationRuntime, OperationRuntimeError>> =
            OnceLock::new();
        OPERATION_RUNTIME
            .get_or_init(Self::new)
            .as_ref()
            .map_err(Clone::clone)
    }

    pub fn register_operation(&self, operation: Operation) -> OperationHandle {
        let id = OperationId(self.next_operation_id.fetch_add(1, Ordering::Relaxed));
        let handle = OperationHandle {
            id,
            operation,
            controller: OperationController::new(),
            started_at: Instant::now(),
        };
        if let Ok(mut operations) = self.operations.lock() {
            operations.insert(id, handle.clone());
        }
        handle
    }

    pub fn complete_operation(&self, id: OperationId) -> Option<OperationHandle> {
        self.operations.lock().ok()?.remove(&id)
    }

    pub fn operation_controller(&self, id: OperationId) -> Option<OperationController> {
        self.operations
            .lock()
            .ok()?
            .get(&id)
            .map(|operation| operation.controller.clone())
    }

    pub fn cancel_operation(&self, id: OperationId) -> bool {
        let Some(controller) = self.operation_controller(id) else {
            return false;
        };
        controller.cancel();
        true
    }

    pub fn active_operations(&self) -> Vec<OperationSnapshot> {
        self.operations.lock().map_or_else(
            |_| Vec::new(),
            |operations| {
                operations
                    .values()
                    .map(|handle| OperationSnapshot {
                        id: handle.id,
                        operation: handle.operation.clone(),
                        started_at: handle.started_at,
                        progress: handle.controller.progress(),
                        cancelled: handle.controller.is_cancelled(),
                        paused: handle.controller.is_paused(),
                    })
                    .collect()
            },
        )
    }

    pub async fn submit<F, Fut, T>(
        &self,
        operation: Operation,
        task: F,
    ) -> Result<OperationSubmission<T>, OperationRuntimeError>
    where
        F: FnOnce(OperationController) -> Fut + Send + 'static,
        Fut: Future<Output = T> + 'static,
        T: Send + 'static,
    {
        let handle = self.register_operation(operation);
        let controller = handle.controller.clone();
        match self
            .submit_task({
                let controller = controller.clone();
                move || task(controller)
            })
            .await
        {
            Ok(result_rx) => Ok(OperationSubmission {
                id: handle.id,
                controller,
                result_rx,
            }),
            Err(err) => {
                self.complete_operation(handle.id);
                Err(err)
            }
        }
    }

    pub async fn submit_task<F, Fut, T>(
        &self,
        task: F,
    ) -> Result<oneshot::Receiver<T>, OperationRuntimeError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + 'static,
        T: Send + 'static,
    {
        self.spawn_task(task)
    }

    /// Queue `task` on the Compio thread without awaiting a result channel.
    pub fn spawn_detached_task<F, Fut>(&self, task: F) -> Result<(), OperationRuntimeError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let compio_task: CompioTask = Box::new(move || Box::pin(task()));
        self.compio_tx
            .try_send(compio_task)
            .map_err(|_| OperationRuntimeError::Stopped)
    }

    /// Queue `task` and return a oneshot that completes with its output.
    pub fn spawn_task<F, Fut, T>(
        &self,
        task: F,
    ) -> Result<oneshot::Receiver<T>, OperationRuntimeError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + 'static,
        T: Send + 'static,
    {
        let (result_tx, result_rx) = oneshot::channel();
        self.spawn_detached_task(move || async move {
            let result = task().await;
            let _ = result_tx.send(result);
        })?;
        Ok(result_rx)
    }

    /// Queue `task` and invoke `on_complete` on a blocking worker when it finishes.
    ///
    /// `on_complete` is intentionally run via `spawn_blocking` so UI bridges can
    /// wake the event loop / send channel results without pinning the Compio reactor.
    pub fn spawn_task_with_completion<F, Fut, T, C>(
        &self,
        task: F,
        on_complete: C,
    ) -> Result<(), OperationRuntimeError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + 'static,
        T: Send + 'static,
        C: FnOnce(T) + Send + 'static,
    {
        self.spawn_detached_task(move || async move {
            let result = task().await;
            let _ = compio::runtime::spawn_blocking(move || on_complete(result)).await;
        })
    }

    pub async fn run_registered<F, Fut, T>(
        &self,
        id: OperationId,
        task: F,
    ) -> Result<T, OperationRuntimeError>
    where
        F: FnOnce(OperationController) -> Fut + Send + 'static,
        Fut: Future<Output = T> + 'static,
        T: Send + 'static,
    {
        let controller = self
            .operation_controller(id)
            .ok_or(OperationRuntimeError::UnknownOperation(id))?;
        let result_rx = self.spawn_task(move || task(controller))?;
        result_rx
            .await
            .map_err(|_| OperationRuntimeError::ResultDropped)
    }
}

pub async fn run_operation_task<F, Fut, T>(task: F) -> Result<T, OperationRuntimeError>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + 'static,
    T: Send + 'static,
{
    let result_rx = OperationRuntime::shared()?.spawn_task(task)?;
    result_rx
        .await
        .map_err(|_| OperationRuntimeError::ResultDropped)
}

/// Fire-and-forget submission for UI / background bridges that already own
/// their completion channel (or only need side effects).
pub fn spawn_operation_task<F, Fut>(task: F) -> Result<(), OperationRuntimeError>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + 'static,
{
    OperationRuntime::shared()?.spawn_detached_task(task)
}

/// Submit an async task and deliver its value through `on_complete`.
pub fn spawn_operation_task_with_completion<F, Fut, T, C>(
    task: F,
    on_complete: C,
) -> Result<(), OperationRuntimeError>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + 'static,
    T: Send + 'static,
    C: FnOnce(T) + Send + 'static,
{
    OperationRuntime::shared()?.spawn_task_with_completion(task, on_complete)
}

/// Submit blocking work on Compio's blocking pool and deliver the result.
///
/// Prefer this for directory listing / channel waits that must not pin the
/// Compio reactor thread.
pub fn spawn_blocking_operation_with_completion<F, T, C>(
    task: F,
    on_complete: C,
) -> Result<(), OperationRuntimeError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
    C: FnOnce(T) + Send + 'static,
{
    OperationRuntime::shared()?.spawn_detached_task(move || async move {
        match run_operation_blocking(task).await {
            Ok(value) => on_complete(value),
            Err(_) => {
                // Runtime is shutting down; drop completion quietly.
            }
        }
    })
}

pub async fn run_registered_operation<F, Fut, T>(
    id: OperationId,
    task: F,
) -> Result<T, OperationRuntimeError>
where
    F: FnOnce(OperationController) -> Fut + Send + 'static,
    Fut: Future<Output = T> + 'static,
    T: Send + 'static,
{
    OperationRuntime::shared()?.run_registered(id, task).await
}

pub async fn run_operation_blocking<F, T>(task: F) -> Result<T, OperationRuntimeError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    compio::runtime::spawn_blocking(task)
        .await
        .map_err(|_| OperationRuntimeError::BlockingWorkerStopped)
}

/// Run a blocking closure on Compio's blocking pool and wait for the result.
///
/// Prefer this over spawning a dedicated OS thread for short blocking work that
/// still needs to report back synchronously on the caller thread.
pub fn run_blocking_operation<F, T>(task: F) -> Result<T, OperationRuntimeError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // Must enter via OperationRuntime: `compio::runtime::spawn_blocking` only
    // works while a Compio runtime is active on the worker thread.
    futures_lite::future::block_on(run_operation_task(move || async move {
        run_operation_blocking(task).await
    }))?
}

#[cfg(test)]
mod tests {
    use super::{
        OperationRuntime, run_blocking_operation, run_operation_blocking, run_operation_task,
        spawn_operation_task, spawn_operation_task_with_completion,
    };
    use crate::core::operations::Operation;
    use crate::core::pane::PaneId;
    use std::time::{Duration, Instant};

    #[test]
    fn operation_runtime_runs_compio_and_blocking_tasks() {
        let result = futures_lite::future::block_on(run_operation_task(|| async {
            let blocking = run_operation_blocking(|| 21_u8).await.unwrap();
            blocking * 2
        }))
        .unwrap();

        assert_eq!(result, 42);
    }

    #[test]
    fn operation_runtime_accepts_multiple_submitted_tasks() {
        let first = futures_lite::future::block_on(run_operation_task(|| async { 1_u8 })).unwrap();
        let second = futures_lite::future::block_on(run_operation_task(|| async { 2_u8 })).unwrap();

        assert_eq!((first, second), (1, 2));
    }

    #[test]
    fn spawn_operation_task_runs_without_caller_thread_wrapper() {
        let (tx, rx) = std::sync::mpsc::channel();
        spawn_operation_task(move || async move {
            let _ = tx.send(7_u8);
        })
        .unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 7);
    }

    #[test]
    fn spawn_operation_task_with_completion_delivers_result() {
        let (tx, rx) = std::sync::mpsc::channel();
        spawn_operation_task_with_completion(
            || async move { 11_u8 },
            move |value| {
                let _ = tx.send(value);
            },
        )
        .unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 11);
    }

    #[test]
    fn run_blocking_operation_returns_value_synchronously() {
        assert_eq!(run_blocking_operation(|| 99_u8).unwrap(), 99);
    }

    #[test]
    fn operation_runtime_tracks_registered_operations() {
        let runtime = OperationRuntime::shared().unwrap();
        let handle = runtime.register_operation(Operation::External {
            pane_id: PaneId(9),
            title: "Working".to_string(),
            detail: None,
            cancellable: true,
        });

        handle.controller.set_progress(super::TransferProgress {
            bytes_done: 5,
            bytes_total: 10,
        });
        assert!(runtime.cancel_operation(handle.id));

        let snapshot = runtime
            .active_operations()
            .into_iter()
            .find(|snapshot| snapshot.id == handle.id)
            .unwrap();
        assert_eq!(snapshot.operation.pane_id(), PaneId(9));
        assert_eq!(snapshot.progress.bytes_done, 5);
        assert!(snapshot.cancelled);

        assert!(runtime.complete_operation(handle.id).is_some());
    }

    #[test]
    fn spawn_task_does_not_block_caller_while_other_tasks_are_pending() {
        let runtime = OperationRuntime::shared().unwrap();
        runtime
            .spawn_detached_task(|| async move {
                std::future::pending::<()>().await;
            })
            .unwrap();

        let submit_started = Instant::now();
        runtime
            .spawn_detached_task(|| async move {})
            .expect("unbounded queue should accept work while other tasks are pending");
        assert!(submit_started.elapsed() < Duration::from_millis(100));
    }
}
