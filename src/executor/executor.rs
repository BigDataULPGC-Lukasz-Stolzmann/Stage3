//! Worker Pool Implementation
//!
//! Manages the lifecycle of task execution. It spawns background workers that continuously
//! poll the `DistributedQueue` for pending tasks assigned to this node.
//!
//! ## Responsibilities
//! - **Polling**: continuously checking for `Pending` tasks in owned partitions.
//! - **Lease Management**: Spawns a background thread to renew task leases during long-running operations.
//! - **Execution**: Invoking the appropriate handler from the `TaskHandlerRegistry`.use super::queue::DistributedQueue;

use super::registry::TaskHandlerRegistry;
use super::types::*;

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;

pub struct TaskExecutor {
    queue: Arc<DistributedQueue>,
    handlers: Arc<TaskHandlerRegistry>,
    worker_count: usize,
}

impl TaskExecutor {
    pub fn new(
        queue: Arc<DistributedQueue>,
        handlers: Arc<TaskHandlerRegistry>,
        worker_count: usize,
    ) -> Arc<Self> {
        Arc::new(Self {
            queue,
            handlers,
            worker_count,
        })
    }

    pub async fn start(self: Arc<Self>) {
        tracing::info!("Starting {} task workers", self.worker_count);

        for worker_id in 0..self.worker_count {
            let executor = self.clone();
            tokio::spawn(async move {
                executor.worker_loop(worker_id).await;
            });
        }

        tracing::info!("Task executor started with {} workers", self.worker_count);
    }


    /// The main loop for a single worker thread.
    ///
    /// 1. Fetches pending tasks from local primary partitions.
    /// 2. Attempts to "claim" a task (atomic state change).
    /// 3. If claimed, executes the task while maintaining a liveness lease.
    async fn worker_loop(&self, worker_id: usize) {
        tracing::info!("Worker {} started", worker_id);

        loop {
            let tasks = self.queue.my_pending_tasks();

            if tasks.is_empty() {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            tracing::trace!("Worker {} found {} available tasks", worker_id, tasks.len());

            let mut claimed = false;
            for (task_id, entry) in tasks {
                match self.queue.try_claim_task(&task_id) {
                    Ok(true) => {
                        tracing::info!(
                            "Worker {} claimed task {} (handler: {:?})",
                            worker_id,
                            task_id.0,
                            match &entry.task {
                                Task::Execute { handler, .. } => handler,
                            }
                        );

                        self.execute_with_lease(&task_id, entry.task).await;

                        claimed = true;
                        break;
                    }
                    Ok(false) => {
                        tracing::trace!("Task {} already claimed by another worker", task_id.0);
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to claim task {}: {}", task_id.0, e);
                        continue;
                    }
                }
            }

            if !claimed {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }

    async fn execute_with_lease(&self, task_id: &TaskId, task: Task) {
        let renewal_handle = self.spawn_lease_renewal(task_id);

        let result = self.execute_task(&task).await;

        renewal_handle.abort();

        match self.queue.complete_task(task_id, result) {
            Ok(_) => {
                tracing::debug!("Task {} marked as complete", task_id.0);
            }
            Err(e) => {
                tracing::error!("Failed to complete task {}: {}", task_id.0, e);
            }
        }
    }


    /// Spawns a background task to periodically renew the lease of a running task.
    ///
    /// This prevents the system from marking a long-running task as "failed/timeout"
    /// while it is actually still processing.
    fn spawn_lease_renewal(&self, task_id: &TaskId) -> tokio::task::JoinHandle<()> {
        let queue = self.queue.clone();
        let task_id = task_id.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;

                match queue.renew_lease(&task_id) {
                    Ok(_) => {
                        tracing::trace!("Renewed lease for task {}", task_id.0);
                    }
                    Err(_) => {
                        tracing::trace!("Task {} no longer needs lease renewal", task_id.0);
                        break;
                    }
                }
            }
        })
    }

    async fn execute_task(&self, task: &Task) -> Result<()> {
        match task {
            Task::Execute { handler, .. } => {
                tracing::debug!("Executing task with handler: {}", handler);
                self.handlers.execute(task).await
            }
        }
    }
}
