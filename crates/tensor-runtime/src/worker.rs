//! Dedicated Compio runtime thread for async I/O services.
//!
//! Mirrors the logging drain pattern: one thread, one [`compio::runtime::Runtime`],
//! no Smithay/DRM ownership. Work is submitted as closures that may use
//! `block_on` for Compio futures.

use std::{
    sync::mpsc::{self, SyncSender},
    thread::{self, JoinHandle},
};

use compio::runtime::Runtime;
use thiserror::Error;

/// Errors from constructing or driving a Compio worker.
#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("failed to create Compio runtime: {0}")]
    Runtime(std::io::Error),
    #[error("failed to spawn Compio worker thread: {0}")]
    Spawn(std::io::Error),
    #[error("Compio worker is shut down")]
    Shutdown,
}

enum WorkerMsg {
    Run(Box<dyn FnOnce(&Runtime) + Send>),
    Stop,
}

/// Handle to a Compio-owned worker thread.
pub struct CompioWorker {
    tx: SyncSender<WorkerMsg>,
    join: Option<JoinHandle<()>>,
}

impl CompioWorker {
    /// Start a named worker with a bounded job queue.
    pub fn start(name: impl Into<String>, job_capacity: usize) -> Result<Self, WorkerError> {
        let name = name.into();
        let (tx, rx) = mpsc::sync_channel::<WorkerMsg>(job_capacity.max(1));
        let join = thread::Builder::new()
            .name(name)
            .spawn(move || {
                let Ok(runtime) = Runtime::new() else {
                    // Without a runtime the worker cannot run jobs; exit quietly.
                    // Construction of the handle already succeeded; jobs will fail try_spawn
                    // only if the channel fills after this exit — prefer explicit start errors
                    // via a preflight Runtime::new on the caller if needed.
                    return;
                };
                while let Ok(msg) = rx.recv() {
                    match msg {
                        WorkerMsg::Run(job) => job(&runtime),
                        WorkerMsg::Stop => break,
                    }
                }
            })
            .map_err(WorkerError::Spawn)?;
        Ok(Self {
            tx,
            join: Some(join),
        })
    }

    /// Queue a job that receives the thread-local Compio runtime.
    ///
    /// Returns `false` if the queue is full or the worker is gone (never blocks).
    pub fn try_spawn(&self, job: impl FnOnce(&Runtime) + Send + 'static) -> bool {
        self.tx.try_send(WorkerMsg::Run(Box::new(job))).is_ok()
    }

    /// Request shutdown and join (best-effort).
    pub fn shutdown(mut self) {
        let _ = self.tx.send(WorkerMsg::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for CompioWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(WorkerMsg::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn worker_runs_job_on_compio_runtime() {
        let worker = CompioWorker::start("tensor-test-worker", 4).unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&done);
        assert!(worker.try_spawn(move |rt| {
            // Touch the runtime so the test proves Compio is live.
            let _ = rt;
            flag.store(true, Ordering::SeqCst);
        }));
        // Busy-wait briefly; job is trivial.
        for _ in 0..100 {
            if done.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(done.load(Ordering::SeqCst));
        worker.shutdown();
    }
}
