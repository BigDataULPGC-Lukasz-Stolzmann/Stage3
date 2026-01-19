//! Distributed Map Implementation
//!
//! The core component providing a `HashMap`-like interface that spans the entire cluster.
//! It handles the complexity of local vs. remote storage, replication, and concurrency.
//!
//! ## Features
//! - **Concurrency**: Uses `DashMap` for lock-free local access.
//! - **Routing**: Automatically forwards requests to the correct Primary node.
//! - **Replication**: Ensures data is copied to backup nodes before confirming success.
//! - **Idempotency**: Prevents duplicate processing of operations using UUIDs.

use super::partitioner::PartitionManager;
use super::protocol::*;
use crate::membership::{service::MembershipService, types::NodeId};

use anyhow::Result;
use dashmap::DashMap;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::hash::Hash;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// A distributed, concurrent key-value store.
///
/// Generic over `K` (Key) and `V` (Value), supporting any serializable data types.
pub struct DistributedMap<K, V> {
    /// Local in-memory storage shard.
    /// Structure: `Partition ID -> Key -> Value`.
    local_data: Arc<DashMap<u32, DashMap<K, V>>>,
    /// History of processed Operation IDs for idempotency deduplication.
    processed_ops: Arc<DashMap<String, u64>>,
    /// Service for cluster discovery.
    membership: Arc<MembershipService>,
    /// Logic for data distribution.
    partitioner: Arc<PartitionManager>,
    /// Client for inter-node HTTP communication.
    http_client: reqwest::Client,
    /// Optional URL prefix for HTTP requests (e.g., "/api").
    base_path: String,
}

impl<K, V> DistributedMap<K, V>
where
    K: ToString + FromStr + Clone + Hash + Eq + Send + Sync,
    <K as FromStr>::Err: std::fmt::Display,
    V: Clone + Serialize + DeserializeOwned + Send + Sync,
{
    /// Creates a new distributed map instance.
    pub fn new(membership: Arc<MembershipService>, partitioner: Arc<PartitionManager>) -> Self {
        Self::new_with_base(membership, partitioner, "")
    }

    /// Creates a new instance with a specific base path for API routing.
    pub fn new_with_base(
        membership: Arc<MembershipService>,
        partitioner: Arc<PartitionManager>,
        base_path: &str,
    ) -> Self {
        let local_data: Arc<DashMap<u32, DashMap<K, V>>> = Arc::new(DashMap::new());
        let cleaned = base_path.trim_end_matches('/');
        let base_path = if cleaned.is_empty() {
            String::new()
        } else if cleaned.starts_with('/') {
            cleaned.to_string()
        } else {
            format!("/{}", cleaned)
        };

        Self {
            local_data,
            processed_ops: Arc::new(DashMap::new()),
            membership,
            partitioner,
            http_client: reqwest::Client::new(),
            base_path,
        }
    }

    /// Idempotency check.
    ///
    /// Tracks recently processed operation IDs (`op_id`) to prevent applying the
    /// same replication message multiple times (exactly-once processing).
    /// Includes a simple cleanup mechanism to prevent unbounded memory growth.
    fn should_process(&self, op_id: &str) -> bool {
        if self.processed_ops.contains_key(op_id) {
            return false;
        }
        if self.processed_ops.len() > 10_000 {
            self.processed_ops.clear();
        }
        self.processed_ops
            .insert(op_id.to_string(), now_ms());
        true
    }

    /// Helper for sending HTTP POST requests with exponential backoff and jitter.
    async fn post_with_retry<T: serde::Serialize>(
        &self,
        url: String,
        payload: &T,
        timeout: std::time::Duration,
        attempts: usize,
    ) -> Result<reqwest::Response> {
        let mut delay_ms = 150u64;

        for attempt in 0..attempts {
            let response = self
                .http_client
                .post(url.clone())
                .json(payload)
                .timeout(timeout)
                .send()
                .await;

            match response {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    if attempt + 1 == attempts {
                        return Err(anyhow::anyhow!(e));
                    }
                    let jitter = rand::random::<u64>() % 50;
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms + jitter)).await;
                    delay_ms = (delay_ms * 2).min(1200);
                }
            }
        }

        Err(anyhow::anyhow!("Retry attempts exhausted"))
    }

    /// Helper for sending HTTP GET requests with exponential backoff.
    async fn get_with_retry(
        &self,
        url: String,
        timeout: std::time::Duration,
        attempts: usize,
    ) -> Result<reqwest::Response> {
        let mut delay_ms = 150u64;

        for attempt in 0..attempts {
            let response = self
                .http_client
                .get(url.clone())
                .timeout(timeout)
                .send()
                .await;

            match response {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    if attempt + 1 == attempts {
                        return Err(anyhow::anyhow!(e));
                    }
                    let jitter = rand::random::<u64>() % 50;
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms + jitter)).await;
                    delay_ms = (delay_ms * 2).min(1200);
                }
            }
        }

        Err(anyhow::anyhow!("Retry attempts exhausted"))
    }

    /// Forwards a write request to the designated Primary node.
    ///
    /// Used when the local node receives a request for a partition it does not own.
    async fn forward_put(
        &self,
        primary_node_id: &NodeId,
        partition: u32,
        op_id: String,
        key: K,
        value: V,
    ) -> Result<()> {
        let node = self
            .membership
            .get_member(primary_node_id)
            .ok_or_else(|| anyhow::anyhow!("Primary node not found"))?;
        let addr = node.http_addr;

        let value_json = serde_json::to_string(&value)?;
        let payload = ForwardPutRequest {
            partition,
            op_id,
            key: key.to_string(),
            value_json,
        };
        let response = self
            .post_with_retry(
                format!("http://{}{}{}", addr, self.base_path, ENDPOINT_FORWARD_PUT),
                &payload,
                std::time::Duration::from_millis(500),
                3,
            )
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("ForwardPut failed {}", response.status()));
        }

        Ok(())
    }

    /// Executes a write operation on the Primary node.
    ///
    /// 1. Checks idempotency using `op_id`.
    /// 2. Writes the data to the local `DashMap`.
    /// 3. Synchronously replicates the data to all calculated Backup nodes.
    pub async fn store_as_primary(
        &self,
        partition: u32,
        op_id: String,
        key: K,
        value: V,
    ) -> Result<()> {
        if !self.should_process(&op_id) {
            return Ok(());
        }
        self.store_local(partition, key.clone(), value.clone());

        let owners = self.partitioner.get_owners(partition);
        for backup in owners.iter().skip(1) {
            self.replicate_to_backup(backup, partition, op_id.clone(), key.clone(), value.clone())
                .await?;
        }

        Ok(())
    }

    /// Sends a replication payload to a specific Backup node.
    async fn replicate_to_backup(
        &self,
        backup_node_id: &NodeId,
        partition: u32,
        op_id: String,
        key: K,
        value: V,
    ) -> Result<()> {
        let node = self
            .membership
            .get_member(backup_node_id)
            .ok_or_else(|| anyhow::anyhow!("Backup node not found"))?;
        let addr = node.http_addr;

        let value_json = serde_json::to_string(&value)?;
        let payload = ReplicateRequest {
            partition,
            op_id,
            key: key.to_string(),
            value_json,
        };
        let response = self
            .post_with_retry(
                format!("http://{}{}{}", addr, self.base_path, ENDPOINT_REPLICATE),
                &payload,
                std::time::Duration::from_millis(500),
                3,
            )
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Replication failed: {}", response.status()));
        }
        Ok(())
    }

    /// Direct low-level write to the local storage engine.
    /// Creates the partition map if it doesn't exist.
    pub fn store_local(&self, partition: u32, key: K, value: V) {
        let partition_map = self
            .local_data
            .entry(partition)
            .or_insert_with(|| DashMap::new());
        partition_map.insert(key, value);
    }

    /// Retrieves all data within a specific partition.
    /// Used for Anti-Entropy (bulk synchronization) or debugging.
    pub fn dump_partition(&self, partition: u32) -> Vec<(K, V)> {
        let mut entries = Vec::new();
        if let Some(partition_map) = self.local_data.get(&partition) {
            for entry in partition_map.iter() {
                entries.push((entry.key().clone(), entry.value().clone()));
            }
        }
        entries
    }

    /// Checks if a partition exists in local storage.
    pub fn has_partition(&self, partition: u32) -> bool {
        self.local_data
            .get(&partition)
            .map(|map| !map.is_empty())
            .unwrap_or(false)
    }

    /// Bulk inserts data into a local partition.
    /// Typically used when applying a snapshot received from another node.
    pub fn apply_partition_entries(&self, partition: u32, entries: Vec<(K, V)>) {
        for (key, value) in entries {
            self.store_local(partition, key, value);
        }
    }

    // --- Statistics ---

    pub fn local_node_id(&self) -> NodeId {
        self.membership.local_node.id.clone()
    }

    pub fn local_partition_count(&self) -> usize {
        self.local_data.len()
    }

    pub fn local_entry_count(&self) -> usize {
        self.local_data
            .iter()
            .map(|entry| entry.value().len())
            .sum()
    }

    /// Writes data received via replication mechanism.
    /// Idempotent operation similar to `store_as_primary` but does not trigger further replication.
    pub fn store_replica(&self, partition: u32, op_id: String, key: K, value: V) -> Result<()> {
        if !self.should_process(&op_id) {
            return Ok(());
        }
        self.store_local(partition, key, value);
        Ok(())
    }

    /// Retrieves a value only if it exists in local memory.
    pub fn get_local(&self, key: &K) -> Option<V> {
        let partition = self.partitioner.get_partition(&key.to_string());

        if let Some(partition_map) = self.local_data.get(&partition)
            && let Some(value) = partition_map.get(key)
        {
            return Some(value.clone());
        }

        None
    }

    /// Retrieves a value from the cluster (Distributed Read).
    ///
    /// 1. **Local Check**: Returns immediately if found locally.
    /// 2. **Primary Query**: If not local, determines the Primary owner and fetches remotely.
    /// 3. **Backup Fallback**: If Primary is unreachable or doesn't have the data, tries Backups.
    pub async fn get(&self, key: &K) -> Option<V> {
        let partition = self.partitioner.get_partition(&key.to_string());

        // 1. Try local
        if let Some(partition_map) = self.local_data.get(&partition)
            && let Some(value) = partition_map.get(key)
        {
            tracing::debug!("GET: Found key locally is partition {}", partition);
            return Some(value.clone());
        }

        let owners = self.partitioner.get_owners(partition);

        if owners.is_empty() {
            tracing::warn!("GET: No alives nodes to fetch from");
            return None;
        }

        let primary_owner = &owners[0];

        // If we are primary but didn't find it locally, try backups directly
        if primary_owner == &self.membership.local_node.id {
            for backup in owners.iter().skip(1) {
                if let Ok(value) = self.fetch_remote(backup, key).await {
                    if value.is_some() {
                        return value;
                    }
                }
            }
            return None;
        }

        // 2. Try Primary
        match self.fetch_remote(primary_owner, key).await {
            Ok(Some(value)) => {
                tracing::debug!("GET: Fetched from remote owner {:?}", primary_owner);
                return Some(value);
            }
            Ok(None) => {
                tracing::debug!("GET: Key not found on owner");
            }
            Err(e) => {
                tracing::error!("GET: Failed to fetch from owner: {}", e);
            }
        }

        // 3. Fallback to Backups
        for backup in owners.iter().skip(1) {
            if let Ok(value) = self.fetch_remote(backup, key).await {
                if value.is_some() {
                    return value;
                }
            }
        }

        None
    }

    /// Performs an internal HTTP GET request to retrieve a value from a remote node.
    /// Used during "scatter-gather" queries or when the local node is not the owner.
    pub async fn fetch_remote(&self, owner_id: &NodeId, key: &K) -> Result<Option<V>> {
        let node = self
            .membership
            .get_member(owner_id)
            .ok_or_else(|| anyhow::anyhow!("Owner node not found: {:?}", owner_id))?;

        let addr = node.http_addr;

        let url = format!(
            "http://{}{}{}/{}",
            addr,
            self.base_path,
            ENDPOINT_GET_INTERNAL,
            key.to_string()
        );

        let response = self
            .get_with_retry(url, std::time::Duration::from_millis(500), 3)
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("GET request failed {}", response.status()));
        }

        let get_response: GetResponse = response.json().await?;

        match get_response.value_json {
            Some(json_str) => {
                let value: V = serde_json::from_str(&json_str)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Fetches the entire contents of a partition from a remote node.
    /// Used by Anti-Entropy to restore missing or corrupted partitions.
    pub async fn fetch_partition(
        &self,
        owner_id: &NodeId,
        partition: u32,
    ) -> Result<Vec<(K, V)>> {
        let node = self
            .membership
            .get_member(owner_id)
            .ok_or_else(|| anyhow::anyhow!("Owner node not found: {:?}", owner_id))?;

        let url = format!(
            "http://{}{}{}/{}",
            node.http_addr,
            self.base_path,
            ENDPOINT_PARTITION_DUMP,
            partition
        );

        let response = self
            .get_with_retry(url, std::time::Duration::from_millis(500), 3)
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Partition dump failed {}", response.status()));
        }

        let dump: PartitionDumpResponse = response.json().await?;
        let mut entries = Vec::new();
        for item in dump.entries {
            let key: K = item
                .key
                .parse()
                .map_err(|e: <K as FromStr>::Err| anyhow::anyhow!(e.to_string()))?;
            let value: V = serde_json::from_str(&item.value_json)?;
            entries.push((key, value));
        }
        Ok(entries)
    }

    /// Stores a value locally and treats the local node as Primary.
    /// Triggers replication to backup nodes.
    pub async fn put_local(&self, key: K, value: V) -> Result<()> {
        let partition = self.partitioner.get_partition(&key.to_string());

        self.store_local(partition, key.clone(), value.clone());

        tracing::info!("Stored locally as primary for partition {}", partition);

        let owners = self.partitioner.get_owners(partition);
        if owners.len() > 1 {
            let op_id = Uuid::new_v4().to_string();
            for backup in owners.iter().skip(1) {
                self.replicate_to_backup(backup, partition, op_id.clone(), key.clone(), value.clone())
                    .await?;
            }
        }

        Ok(())
    }

    /// Public API for writing data to the cluster.
    /// Generates a new `op_id` and delegates to `put_with_op`.
    pub async fn put(&self, key: K, value: V) -> Result<()> {
        let op_id = Uuid::new_v4().to_string();
        self.put_with_op(key, value, op_id).await
    }

    /// The main entry point for writing data (Distributed Write).
    ///
    /// 1. Determines the partition for the key.
    /// 2. If local node is Primary: Stores locally and replicates to backups.
    /// 3. If local node is NOT Primary: Forwards the request to the correct owner.
    /// 4. If local node is Primary but offline: Falls back to local storage (emergency mode).
    pub async fn put_with_op(&self, key: K, value: V, op_id: String) -> Result<()> {
        if !self.should_process(&op_id) {
            return Ok(());
        }
        let partition = self.partitioner.get_partition(&key.to_string());
        let owners = self.partitioner.get_owners(partition);
        
        // Edge case: No nodes found (cluster startup)
        if owners.is_empty() {
            tracing::warn!("No alive nodes, storing locally as fallback");
            self.store_local(partition, key, value);
            return Ok(());
        }
        
        // Forward if we are not the primary
        if self.membership.local_node.id != owners[0] {
            self.forward_put(&owners[0], partition, op_id, key, value)
                .await?;
            return Ok(());
        } else {
            // We are primary
            self.store_local(partition, key.clone(), value.clone());

            // Replicate to backups
            if owners.len() > 1 {
                for backup in owners.iter().skip(1) {
                    self.replicate_to_backup(
                        backup,
                        partition,
                        op_id.clone(),
                        key.clone(),
                        value.clone(),
                    )
                    .await?;
                }
            }
        }

        Ok(())
    }
}

/// Helper to get current timestamp in milliseconds.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}