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
/// File I/O and long-running tasks run on a dedicated Compio thread. Completion
/// is bridged with runtime-agnostic channels so callers do not need Tokio.
pub struct OperationRuntime {
    compio_tx: Sender<CompioTask>,
    next_operation_id: AtomicU64,
    operations: Mutex<BTreeMap<OperationId, OperationHandle>>,
}

impl OperationRuntime {
    fn new() -> Result<Self, OperationRuntimeError> {
        let (compio_tx, compio_rx) = async_channel::bounded::<CompioTask>(1);

        std::thread::Builder::new()
            .name("fika-operation-compio".to_string())
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
        let (result_tx, result_rx) = oneshot::channel();
        let compio_task: CompioTask = Box::new(move || {
            Box::pin(async move {
                let result = task().await;
                let _ = result_tx.send(result);
            })
        });
        self.compio_tx
            .send(compio_task)
            .await
            .map_err(|_| OperationRuntimeError::Stopped)?;
        Ok(result_rx)
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
        let result_rx = self.submit_task(move || task(controller)).await?;
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
    let result_rx = OperationRuntime::shared()?.submit_task(task).await?;
    result_rx
        .await
        .map_err(|_| OperationRuntimeError::ResultDropped)
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

#[cfg(test)]
mod tests {
    use super::{OperationRuntime, run_operation_blocking, run_operation_task};
    use crate::core::operations::Operation;
    use crate::core::pane::PaneId;

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
}
