use super::protocol::*;
use super::types::*;
use crate::membership::{service::MembershipService, types::NodeId};
use crate::storage::partitioner::PartitionManager;

use anyhow::Result;
use dashmap::DashMap;
use std::sync::Arc;

pub struct DistributedQueue {
    local_tasks: Arc<DashMap<u32, DashMap<TaskId, TaskEntry>>>,
    membership: Arc<MembershipService>,
    partitioner: Arc<PartitionManager>,
    http_client: reqwest::Client,
}

impl DistributedQueue {
    pub fn new(membership: Arc<MembershipService>, partitioner: Arc<PartitionManager>) -> Self {
        Self {
            local_tasks: Arc::new(DashMap::new()),
            membership,
            partitioner,
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn submit(&self, task: Task) -> Result<TaskId> {
        let task_id = TaskId::new();
        let partition = self.partitioner.get_partition(&task_id.0);
        let owners = self.partitioner.get_owners(partition);

        if owners.is_empty() {
            tracing::warn!("No alive nodes, storing task locally");
            self.store_local(
                partition,
                task_id.clone(),
                TaskEntry {
                    task,
                    status: TaskStatus::Pending,
                    assigned_to: None,
                    created_at: now_ms(),
                    lease_expires: None,
                },
            );
            return Ok(task_id);
        }

        let primary = &owners[0];

        if primary == &self.membership.local_node.id {
            tracing::debug!(
                "Storing task {} in partition {} (I'm primary)",
                task_id.0,
                partition
            );
            self.store_local(
                partition,
                task_id.clone(),
                TaskEntry {
                    task,
                    status: TaskStatus::Pending,
                    assigned_to: None,
                    created_at: now_ms(),
                    lease_expires: None,
                },
            );
        } else {
            tracing::debug!("Forwarding task {} to primary {:?}", task_id.0, primary);
            self.forward_task(primary, partition, task_id.clone(), task)
                .await?;
        }

        Ok(task_id)
    }

    pub fn store_local(&self, partition: u32, task_id: TaskId, entry: TaskEntry) {
        let partition_map = self
            .local_tasks
            .entry(partition)
            .or_insert_with(|| DashMap::new());

        partition_map.insert(task_id, entry);

        tracing::info!("Stored task in partition {}", partition);
    }

    async fn forward_task(
        &self,
        target: &NodeId,
        partition: u32,
        task_id: TaskId,
        task: Task,
    ) -> Result<()> {
        let node = self
            .membership
            .get_member(target)
            .ok_or_else(|| anyhow::anyhow!("Target node not found"))?;

        let payload = ForwardTaskRequest {
            partition,
            task_id,
            task,
        };

        let response = self
            .http_client
            .post(format!("http://{}/internal/submit_task", node.http_addr))
            .json(&payload)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Forward failed: {}", response.status()));
        }

        Ok(())
    }

    /// Get all pending tasks for partitions owned by this node
    pub fn my_pending_tasks(&self) -> Vec<(TaskId, TaskEntry)> {
        let my_partitions = self.get_my_primary_partitions();

        let mut tasks = Vec::new();

        for partition in my_partitions {
            if let Some(partition_map) = self.local_tasks.get(&partition) {
                for entry in partition_map.iter() {
                    let task_entry = entry.value();

                    let is_available = match task_entry.status {
                        TaskStatus::Pending => true,
                        TaskStatus::Running => {
                            // Check if lease expired - can reclaim
                            if let Some(lease) = task_entry.lease_expires {
                                now_ms() > lease
                            } else {
                                false
                            }
                        }
                        _ => false, // Completed/Failed - skip
                    };

                    if is_available {
                        tasks.push((entry.key().clone(), task_entry.clone()));
                    }
                }
            }
        }

        tasks
    }

    fn get_my_primary_partitions(&self) -> Vec<u32> {
        (0..self.partitioner.num_partitions)
            .filter(|&partition| {
                let owners = self.partitioner.get_owners(partition);
                !owners.is_empty() && owners[0] == self.membership.local_node.id
            })
            .collect()
    }

    pub fn try_claim_task(&self, task_id: &TaskId) -> Result<bool> {
        let partition = self.partitioner.get_partition(&task_id.0);

        if let Some(partition_map) = self.local_tasks.get(&partition) {
            if let Some(mut entry) = partition_map.get_mut(task_id) {
                if entry.status != TaskStatus::Pending {
                    return Ok(false);
                }

                entry.status = TaskStatus::Running;
                entry.assigned_to = Some(self.membership.local_node.id.clone());
                entry.lease_expires = Some(now_ms() + 30_000);

                tracing::debug!("Claimed task {}", task_id.0);
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub fn renew_lease(&self, task_id: &TaskId) -> Result<()> {
        let partition = self.partitioner.get_partition(&task_id.0);

        if let Some(partition_map) = self.local_tasks.get(&partition) {
            if let Some(mut entry) = partition_map.get_mut(task_id) {
                if entry.status == TaskStatus::Running {
                    entry.lease_expires = Some(now_ms() + 30_000);
                    tracing::trace!("Renewed lease for task {}", task_id.0);
                    return Ok(());
                } else {
                    return Err(anyhow::anyhow!(
                        "Task not running (status: {:?})",
                        entry.status
                    ));
                }
            }
        }

        Err(anyhow::anyhow!("Task not found"))
    }

    pub fn complete_task(&self, task_id: &TaskId, result: Result<()>) -> Result<()> {
        let partition = self.partitioner.get_partition(&task_id.0);

        if let Some(partition_map) = self.local_tasks.get(&partition) {
            if let Some(mut entry) = partition_map.get_mut(task_id) {
                match result {
                    Ok(_) => {
                        entry.status = TaskStatus::Completed;
                        tracing::info!("Task {} completed", task_id.0);
                    }
                    Err(e) => {
                        entry.status = TaskStatus::Failed {
                            error: e.to_string(),
                        };
                        tracing::error!("Task {} failed: {}", task_id.0, e);
                    }
                }
                entry.lease_expires = None;
                return Ok(());
            }
        }

        Err(anyhow::anyhow!("Task not found"))
    }

    pub fn get_task(&self, task_id: &TaskId) -> Option<TaskEntry> {
        let partition = self.partitioner.get_partition(&task_id.0);

        if let Some(partition_map) = self.local_tasks.get(&partition) {
            if let Some(entry) = partition_map.get(task_id) {
                return Some(entry.clone());
            }
        }

        None
    }
}
