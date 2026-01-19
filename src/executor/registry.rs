//! Task Handler Registry
//!
//! A dynamic registry that maps string-based task names (e.g., "index_document")
//! to executable Rust closures. This allows the system to remain generic and
//! extensible without hardcoding specific task logic in the queue module.

use super::types::*;

use anyhow::Result;
use dashmap::DashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Type alias for a thread-safe, asynchronous task handler function.
/// It takes a `Task` object and returns a Future that resolves to a `Result<()>`.
pub type TaskHandlerFn =
    Arc<dyn Fn(Task) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

/// Registry holding the mapping between task names and their implementation.
pub struct TaskHandlerRegistry {
    handlers: DashMap<String, TaskHandlerFn>,
}

impl TaskHandlerRegistry {
    /// Creates a new, empty registry.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            handlers: DashMap::new(),
        })
    }

    /// Registers a new handler function under a specific name.
    ///
    /// # Arguments
    /// * `handler_name` - The string identifier for the task (e.g., "index_document").
    /// * `handler` - The closure/function that implements the task logic.
    pub fn register<F, Fut>(&self, handler_name: &str, handler: F)
    where
        F: Fn(Task) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        // Wrap the handler in a Box::pin to type-erase the specific Future type,
        // allowing us to store different async functions in the same Map.
        let handler_fn: TaskHandlerFn = Arc::new(move |task: Task| {
            Box::pin(handler(task)) as Pin<Box<dyn Future<Output = Result<()>> + Send>>
        });

        self.handlers.insert(handler_name.to_string(), handler_fn);

        tracing::info!("Registered task handler: {}", handler_name);
    }

    /// Looks up a handler by name and executes it with the provided task payload.
    ///
    /// # Returns
    /// * `Ok(())` if the handler executed successfully.
    /// * `Err` if the handler failed or if no handler exists for the given name.
    pub async fn execute(&self, task: &Task) -> Result<()> {
        match task {
            Task::Execute { handler, payload } => {
                if let Some(handler_fn) = self.handlers.get(handler) {
                    tracing::debug!(
                        "Executing task with handler '{}' (payload size: {} bytes)",
                        handler,
                        payload.to_string().len()
                    );

                    // Invoke the stored closure
                    handler_fn.value()(task.clone()).await
                } else {
                    let error = format!("Unknown task handler: {}", handler);
                    tracing::error!("{}", error);
                    Err(anyhow::anyhow!(error))
                }
            }
        }
    }

    /// Returns a list of all registered handler names.
    pub fn list_handlers(&self) -> Vec<String> {
        self.handlers
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Checks if a handler is registered.
    pub fn has_handler(&self, handler_name: &str) -> bool {
        self.handlers.contains_key(handler_name)
    }

    /// Returns the total number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }
}

impl Default for TaskHandlerRegistry {
    fn default() -> Self {
        Self {
            handlers: DashMap::new(),
        }
    }
}