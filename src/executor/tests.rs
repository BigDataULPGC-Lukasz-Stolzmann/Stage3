//! Executor Module Tests
//!
//! This module contains unit and integration tests for the task execution system.
//!
//! ## Test Scopes
//! - **Registry**: Verifies task registration, lookup, and execution mechanics.
//! - **Data Types**: Validates serialization/deserialization of task entries and status logic.
//! - **Business Logic**: Simulates real-world indexing scenarios to ensure handlers process payloads correctly.
//!
//! *Note: Integration tests requiring network interactions (e.g., cross-node forwarding)
//! are located in the top-level `test_cluster.sh` script.*

#[cfg(test)]
mod tests {
    use crate::executor::registry::TaskHandlerRegistry;
    use crate::executor::types::{Task, TaskId, TaskStatus, TaskEntry, now_ms};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // ============================================================
    // TEST 1: TaskHandlerRegistry - Registration and Execution
    // ============================================================

    #[tokio::test]
    async fn test_registry_register_and_execute() {
        // ARRANGE: Create registry and call counter
        let registry = TaskHandlerRegistry::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        // ACT: Register a simple handler that increments a counter
        registry.register("test_handler", move |_task| {
            let count = call_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        });

        // ASSERT: Handler is registered
        assert!(registry.has_handler("test_handler"));
        assert_eq!(registry.handler_count(), 1);

        // ACT: Execute the task via the registry
        let task = Task::Execute {
            handler: "test_handler".to_string(),
            payload: serde_json::json!({"test": "data"}),
        };

        let result = registry.execute(&task).await;

        // ASSERT: Handler was called successfully
        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_registry_unknown_handler_returns_error() {
        // ARRANGE
        let registry = TaskHandlerRegistry::new();

        let task = Task::Execute {
            handler: "non_existent_handler".to_string(),
            payload: serde_json::json!({}),
        };

        // ACT
        let result = registry.execute(&task).await;

        // ASSERT: Should return an error because the handler doesn't exist
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown task handler"));
    }

    #[tokio::test]
    async fn test_registry_handler_can_fail() {
        // ARRANGE: Register a handler that always returns an error
        let registry = TaskHandlerRegistry::new();

        registry.register("failing_handler", |_task| async {
            Err(anyhow::anyhow!("Intentional error"))
        });

        let task = Task::Execute {
            handler: "failing_handler".to_string(),
            payload: serde_json::json!({}),
        };

        // ACT
        let result = registry.execute(&task).await;

        // ASSERT: The executor should capture the error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Intentional error"));
    }

    #[tokio::test]
    async fn test_registry_handler_receives_payload() {
        // ARRANGE: Verify that the JSON payload is correctly passed to the handler
        let registry = TaskHandlerRegistry::new();
        let received_payload = Arc::new(tokio::sync::Mutex::new(None));
        let received_clone = received_payload.clone();

        registry.register("payload_handler", move |task| {
            let received = received_clone.clone();
            async move {
                if let Task::Execute { payload, .. } = task {
                    *received.lock().await = Some(payload);
                }
                Ok(())
            }
        });

        let task = Task::Execute {
            handler: "payload_handler".to_string(),
            payload: serde_json::json!({"book_id": "123", "title": "Test Book"}),
        };

        // ACT
        registry.execute(&task).await.unwrap();

        // ASSERT
        let payload = received_payload.lock().await;
        assert!(payload.is_some());
        let p = payload.as_ref().unwrap();
        assert_eq!(p["book_id"], "123");
        assert_eq!(p["title"], "Test Book");
    }

    // ============================================================
    // TEST 2: TaskId
    // ============================================================

    #[test]
    fn test_task_id_is_unique() {
        let id1 = TaskId::new();
        let id2 = TaskId::new();

        // UUIDs should be unique
        assert_ne!(id1.0, id2.0);
    }

    // ============================================================
    // TEST 3: TaskStatus
    // ============================================================

    #[test]
    fn test_task_status_equality() {
        assert_eq!(TaskStatus::Pending, TaskStatus::Pending);
        assert_eq!(TaskStatus::Running, TaskStatus::Running);
        assert_eq!(TaskStatus::Completed, TaskStatus::Completed);

        assert_ne!(TaskStatus::Pending, TaskStatus::Running);

        let failed1 = TaskStatus::Failed { error: "test".to_string() };
        let failed2 = TaskStatus::Failed { error: "test".to_string() };
        let failed3 = TaskStatus::Failed { error: "other".to_string() };

        assert_eq!(failed1, failed2);
        assert_ne!(failed1, failed3);
    }

    // ============================================================
    // TEST 4: TaskEntry serialization
    // ============================================================

    #[test]
    fn test_task_entry_serialization() {
        let entry = TaskEntry {
            task: Task::Execute {
                handler: "index_book".to_string(),
                payload: serde_json::json!({"book_id": "abc123"}),
            },
            status: TaskStatus::Pending,
            assigned_to: None,
            created_at: now_ms(),
            lease_expires: None,
        };

        // Serialize
        let json = serde_json::to_string(&entry).expect("Serialization failed");

        // Deserialize
        let restored: TaskEntry = serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(restored.status, TaskStatus::Pending);
        if let Task::Execute { handler, payload } = restored.task {
            assert_eq!(handler, "index_book");
            assert_eq!(payload["book_id"], "abc123");
        } else {
            panic!("Wrong task type");
        }
    }
}