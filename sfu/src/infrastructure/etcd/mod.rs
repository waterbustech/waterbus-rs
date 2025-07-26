use etcd_client::{Client, PutOptions};
use serde::Serialize;
use std::time::Duration;
use sysinfo::System;
use tokio::{sync::oneshot, time::interval};
use tracing::{debug, error, info, warn};

/// NodeMetadata is the metadata of the node
#[derive(Debug, Serialize)]
struct NodeMetadata {
    addr: String,
    cpu: f32,
    ram: f32,
    group_id: String,
}

/// EtcdNode is a node that is used to register and deregister the sfu node to etcd
pub struct EtcdNode {
    lease_id: i64,
    client: Client,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl EtcdNode {
    /// Register the node to etcd
    ///
    /// # Arguments
    ///
    /// * `etcd_addr` - The address of the etcd server
    /// * `node_id` - The id of the node
    /// * `node_ip` - The ip of the node
    /// * `group_id` - The id of the group
    /// * `ttl` - The ttl of the lease
    ///
    /// # Returns
    ///
    /// * `Self` - The EtcdNode
    pub async fn register(
        etcd_addr: String,
        node_id: String,
        node_ip: String,
        group_id: String,
        ttl: i64,
    ) -> anyhow::Result<Self> {
        let mut client = Client::connect([etcd_addr], None).await?;
        let lease_id = client.lease_grant(ttl, None).await?.id();

        let key = format!("/sfu/nodes/{node_id}");
        let metadata = NodeMetadata {
            addr: node_ip.clone(),
            cpu: 0.0,
            ram: 0.0,
            group_id: group_id.clone(),
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
            let mut metrics_tick = interval(Duration::from_secs(15));

            let mut keepalive_tick = interval(Duration::from_secs((ttl - 1).try_into().unwrap()));

            metrics_tick.tick().await;
            keepalive_tick.tick().await;

            let mut system: Option<System> = None;
            let mut last_cpu = 0.0f32;
            let mut last_ram = 0.0f32;

            let mut metrics_counter = 0u32;

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        info!("Stopping lease keep-alive");
                        break;
                    }

                    _ = keepalive_tick.tick() => {
                        if let Err(err) = keeper.keep_alive().await {
                            error!("Keep-alive error: {:?}", err);
                            break;
                        }

                        if let Ok(Some(msg)) = responses.message().await {
                            debug!("Lease keep-alive: {:?}", msg);
                        }
                    }

                    _ = metrics_tick.tick() => {
                        metrics_counter += 1;

                        if metrics_counter % 5 != 0 {
                            continue;
                        }

                        if system.is_none() {
                            system = Some(System::new());
                        }

                        let (cpu_free, ram_free) = if let Some(ref mut sys) = system {
                            let cpu = Self::get_free_cpu_efficient(sys).unwrap_or(0.0);
                            let ram = Self::get_free_ram_efficient(sys).unwrap_or(0.0);
                            (cpu, ram)
                        } else {
                            (0.0, 0.0)
                        };

                        let cpu_diff = (cpu_free - last_cpu).abs();
                        let ram_diff = (ram_free - last_ram).abs();

                        if cpu_diff >= 5.0 || ram_diff >= 5.0 {
                            let updated_metadata = NodeMetadata {
                                addr: node_ip.clone(),
                                cpu: cpu_free,
                                ram: ram_free,
                                group_id: group_id.clone(),
                            };

                            if let Ok(new_value) = serde_json::to_string(&updated_metadata) {
                                match client_clone.put(
                                    key_clone.clone(),
                                    new_value,
                                    Some(PutOptions::new().with_lease(lease_id))
                                ).await {
                                    Ok(_) => {
                                        last_cpu = cpu_free;
                                        last_ram = ram_free;
                                        debug!("Updated node metrics: CPU: {:.1}%, RAM: {:.1}%", cpu_free, ram_free);
                                    }
                                    Err(err) => {
                                        error!("Failed to update node resource info: {:?}", err);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            lease_id,
            client,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Deregister the node from etcd
    ///
    /// # Arguments
    ///
    /// * `self` - The EtcdNode
    ///
    pub async fn deregister(mut self) {
        info!("Revoking lease and shutting down etcd registration");
        if let Err(err) = self.client.lease_revoke(self.lease_id).await {
            warn!("Failed to revoke lease: {:?}", err);
        }
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Get the free cpu
    ///
    /// # Arguments
    ///
    /// * `system` - The system
    ///
    /// # Returns
    fn get_free_cpu_efficient(system: &mut System) -> Option<f32> {
        system.refresh_cpu_usage();

        let cpus = system.cpus();
        if cpus.is_empty() {
            return Some(50.0);
        }

        let total_usage: f32 = cpus.iter().map(|cpu| cpu.cpu_usage()).sum();
        let avg_usage = total_usage / cpus.len() as f32;
        let free_percent = (100.0 - avg_usage).max(0.0).min(100.0);

        Some(free_percent)
    }

    /// Get the free ram
    ///
    /// # Arguments
    ///
    /// * `system` - The system
    ///
    /// # Returns
    fn get_free_ram_efficient(system: &mut System) -> Option<f32> {
        system.refresh_memory();

        let total = system.total_memory() as f64;
        let available = system.available_memory() as f64;

        if total > 0.0 {
            let free_percent = ((available / total) * 100.0) as f32;
            Some(free_percent.max(0.0).min(100.0))
        } else {
            Some(50.0)
        }
    }
}
