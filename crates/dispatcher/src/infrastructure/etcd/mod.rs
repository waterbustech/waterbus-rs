use etcd_client::{Client, EventType, GetOptions, WatchOptions};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeMetadata {
    pub addr: String,
    pub cpu: f32, // e.g. 0.0 to 100.0
    pub ram: f32, // e.g. 0.0 to 100.0
}

#[derive(Clone)]
pub struct EtcdDispatcher {
    client: Client,
    nodes: Arc<RwLock<HashMap<String, NodeMetadata>>>,
    prefix: String,
}

impl EtcdDispatcher {
    pub async fn new(etcd_endpoints: &[&str], prefix: &str) -> anyhow::Result<Self> {
        let client = Client::connect(etcd_endpoints, None).await?;
        let mut etcd = EtcdDispatcher {
            client,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            prefix: prefix.to_string(),
        };
        etcd.sync_nodes().await?;
        etcd.start_watch();
        Ok(etcd)
    }

    async fn sync_nodes(&mut self) -> anyhow::Result<()> {
        let resp = self
            .client
            .get(self.prefix.clone(), Some(GetOptions::new().with_prefix()))
            .await?;

        let mut nodes = self.nodes.write().unwrap();
        nodes.clear();

        for kv in resp.kvs() {
            if let Some((id, meta)) =
                Self::parse_node_info(kv.key_str().unwrap(), kv.value_str().unwrap())
            {
                nodes.insert(id, meta);
            }
        }
        Ok(())
    }

    fn start_watch(&self) {
        let prefix = self.prefix.clone();
        let mut client = self.client.clone();
        let nodes = self.nodes.clone();

        tokio::spawn(async move {
            let (_, mut stream) = client
                .watch(prefix.clone(), Some(WatchOptions::new().with_prefix()))
                .await
                .expect("Failed to start watch");

            while let Some(Ok(resp)) = stream.next().await {
                for event in resp.events() {
                    match event.event_type() {
                        EventType::Put => {
                            if let Some(kv) = event.kv() {
                                if let Some((id, metadata)) = EtcdDispatcher::parse_node_info(
                                    kv.key_str().unwrap(),
                                    kv.value_str().unwrap(),
                                ) {
                                    nodes.write().unwrap().insert(id, metadata);
                                }
                            }
                        }
                        EventType::Delete => {
                            if let Some(kv) = event.kv() {
                                let key = kv.key_str().unwrap();
                                if let Some(id) = key.strip_prefix(&prefix) {
                                    nodes.write().unwrap().remove(id);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    fn parse_node_info(key: &str, val: &str) -> Option<(String, NodeMetadata)> {
        let id = key.split('/').last()?.to_string();
        let metadata: NodeMetadata = serde_json::from_str(val).ok()?;
        Some((id, metadata))
    }

    /// Return the least loaded node based on CPU usage
    pub fn get_node_least(&self) -> Option<(String, NodeMetadata)> {
        let nodes = self.nodes.read().unwrap();
        nodes
            .iter()
            .min_by(|a, b| {
                a.1.cpu
                    .partial_cmp(&b.1.cpu)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(id, meta)| (id.clone(), meta.clone()))
    }

    pub fn get_node_by_id(&self, id: &str) -> Option<NodeMetadata> {
        let nodes = self.nodes.read().unwrap();
        nodes.get(id).cloned()
    }
}
