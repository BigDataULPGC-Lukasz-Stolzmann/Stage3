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

pub struct DistributedMap<K, V> {
    local_data: Arc<DashMap<u32, DashMap<K, V>>>,
    membership: Arc<MembershipService>,
    partitioner: Arc<PartitionManager>,
    http_client: reqwest::Client,
}

impl<K, V> DistributedMap<K, V>
where
    K: ToString + FromStr + Clone + Hash + Eq + Send + Sync,
    <K as FromStr>::Err: std::fmt::Display,
    V: Clone + Serialize + DeserializeOwned + Send + Sync,
{
    pub fn new(membership: Arc<MembershipService>, partitioner: Arc<PartitionManager>) -> Self {
        let local_data: Arc<DashMap<u32, DashMap<K, V>>> = Arc::new(DashMap::new());
        Self {
            local_data,
            membership,
            partitioner,
            http_client: reqwest::Client::new(),
        }
    }

    async fn forward_put(
        &self,
        primary_node_id: &NodeId,
        partition: u32,
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
            key: key.to_string(),
            value_json,
        };
        let response = self
            .http_client
            .post(format!("http://{}{}", addr, ENDPOINT_FORWARD_PUT))
            .json(&payload)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("ForwardPut failed {}", response.status()));
        }

        Ok(())
    }

    pub async fn store_as_primary(&self, partition: u32, key: K, value: V) -> Result<()> {
        self.store_local(partition, key.clone(), value.clone());

        let owners = self.partitioner.get_owners(partition);
        if owners.len() > 1 {
            self.replicate_to_backup(&owners[1], partition, key, value)
                .await?;
        }

        Ok(())
    }

    async fn replicate_to_backup(
        &self,
        backup_node_id: &NodeId,
        partition: u32,
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
            key: key.to_string(),
            value_json,
        };
        let response = self
            .http_client
            .post(format!("http://{}{}", addr, ENDPOINT_REPLICATE))
            .json(&payload)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Replication failed: {}", response.status()));
        }
        Ok(())
    }

    pub fn store_local(&self, partition: u32, key: K, value: V) {
        let partition_map = self
            .local_data
            .entry(partition)
            .or_insert_with(|| DashMap::new());
        partition_map.insert(key, value);
    }

    pub fn get_local(&self, key: &K) -> Option<V> {
        let partition = self.partitioner.get_partition(&key.to_string());

        if let Some(partition_map) = self.local_data.get(&partition)
            && let Some(value) = partition_map.get(key)
        {
            return Some(value.clone());
        }

        None
    }

    pub async fn get(&self, key: &K) -> Option<V> {
        let partition = self.partitioner.get_partition(&key.to_string());

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

        if primary_owner == &self.membership.local_node.id {
            if owners.len() > 1 {
                return self.fetch_remote(&owners[1], key).await.ok().flatten();
            }
            return None;
        }

        match self.fetch_remote(primary_owner, key).await {
            Ok(Some(value)) => {
                tracing::debug!("GET: Fetched from remote owner {:?}", primary_owner);
                Some(value)
            }
            Ok(None) => {
                tracing::debug!("GET: Key not found on owner");
                None
            }
            Err(e) => {
                tracing::error!("GET: Failed to fetch from owner: {}", e);

                if owners.len() > 1 {
                    self.fetch_remote(&owners[1], key).await.ok().flatten()
                } else {
                    None
                }
            }
        }
    }

    pub async fn fetch_remote(&self, owner_id: &NodeId, key: &K) -> Result<Option<V>> {
        let node = self
            .membership
            .get_member(owner_id)
            .ok_or_else(|| anyhow::anyhow!("Owner node not found: {:?}", owner_id))?;

        let addr = node.http_addr;

        let url = format!(
            "http://{}{}/{}",
            addr,
            ENDPOINT_GET_INTERNAL,
            key.to_string()
        );

        let response = self
            .http_client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;

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

    pub async fn put_local(&self, key: K, value: V) -> Result<()> {
        let partition = self.partitioner.get_partition(&key.to_string());

        self.store_local(partition, key.clone(), value.clone());

        tracing::info!("Stored locally as primary for partition {}", partition);

        let owners = self.partitioner.get_owners(partition);
        if owners.len() > 1 {
            self.replicate_to_backup(&owners[1], partition, key, value)
                .await?;
        }

        Ok(())
    }

    pub async fn put(&self, key: K, value: V) -> Result<()> {
        let partition = self.partitioner.get_partition(&key.to_string());
        let owners = self.partitioner.get_owners(partition);
        if owners.is_empty() {
            tracing::warn!("No alive nodes, storing locally as fallback");
            self.store_local(partition, key, value);
            return Ok(());
        }
        if self.membership.local_node.id != owners[0] {
            self.forward_put(&owners[0], partition, key, value).await?;
            return Ok(());
        } else {
            self.store_local(partition, key.clone(), value.clone());

            if owners.len() > 1 {
                self.replicate_to_backup(&owners[1], partition, key, value)
                    .await?;
                // let backup = owners[1].clone();
                // let self_clone = self.clone();
                //
                // tokio::spawn(async move {
                //     let _ = self_clone
                //         .replicate_to_backup(&backup, partition, key, value)
                //         .await;
                // });
            }
        }

        Ok(())
    }
}
