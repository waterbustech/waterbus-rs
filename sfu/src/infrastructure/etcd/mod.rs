use etcd_client::{Client, PutOptions};
use serde::Serialize;
use std::time::Duration;
use sysinfo::System;
use tokio::{sync::oneshot, time::interval};
use tracing::{debug, error, info};

#[derive(Debug, Serialize)]
struct NodeMetadata {
    addr: String,
    cpu: f32,
    ram: f32,
}

pub struct EtcdNode {
    lease_id: i64,
    client: Client,
    // key: String,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl EtcdNode {
    pub async fn register(
        etcd_addr: String,
        node_id: String,
        node_ip: String,
        ttl: i64,
    ) -> anyhow::Result<Self> {
        let mut client = Client::connect([etcd_addr], None).await?;
        let lease_id = client.lease_grant(ttl, None).await?.id();

        let key = format!("/sfu/nodes/{}", node_id);
        let metadata = NodeMetadata {
            addr: node_ip.clone(),
            cpu: 0.0,
            ram: 0.0,
        };
        let value = serde_json::to_string(&metadata)?;

        client
            .put(
                key.clone(),
                value,
                Some(PutOptions::new().with_lease(lease_id)),
            )
            .await?;

        let (mut keeper, mut responses) = client.lease_keep_alive(lease_id).await?;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let mut client_clone = client.clone();
        let key_clone = key.clone();

        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(5));
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("Stopping lease keep-alive");
                        break;
                    }
                    result = keeper.keep_alive() => {
                        if let Err(err) = result {
                            error!("Keep-alive error: {:?}", err);
                            break;
                        }
                    }
                    resp = responses.message() => {
                        if let Ok(Some(msg)) = resp {
                            debug!("Lease keep-alive: {:?}", msg);
                        }
                    }
                    _ = tick.tick() => {
                        let cpu_free = Self::get_free_cpu().unwrap_or(0.0);
                        let ram_free = Self::get_free_ram().unwrap_or(0.0);

                        let updated_metadata = NodeMetadata {
                            addr: node_ip.clone(),
                            cpu: cpu_free,
                            ram: ram_free,
                        };

                        let new_value = serde_json::to_string(&updated_metadata).unwrap();
                        if let Err(err) = client_clone.put(
                            key_clone.clone(),
                            new_value,
                            Some(PutOptions::new().with_lease(lease_id))
                        ).await {
                            error!("Failed to update node resource info: {:?}", err);
                        }
                    }
                }
            }
        });

        Ok(Self {
            lease_id,
            client,
            // key,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    pub async fn deregister(mut self) {
        info!("Revoking lease and shutting down etcd registration");
        let _ = self.client.lease_revoke(self.lease_id).await;
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    fn get_free_cpu() -> Option<f32> {
        let mut system = System::new();
        system.refresh_cpu_all();

        std::thread::sleep(std::time::Duration::from_millis(100));
        system.refresh_cpu_all();

        let avg_idle = system
            .cpus()
            .iter()
            .map(|cpu| 100.0 - cpu.cpu_usage())
            .sum::<f32>()
            / system.cpus().len() as f32;
        Some(avg_idle)
    }

    fn get_free_ram() -> Option<f32> {
        let mut system = System::new();
        system.refresh_memory();

        let total = system.total_memory() as f32;
        let free = system.available_memory() as f32;

        println!("total: {}, free: {}", total, free);
        if total > 0.0 {
            Some((free / total) * 100.0)
        } else {
            None
        }
    }
}
