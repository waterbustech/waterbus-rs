use std::sync::Arc;

use async_channel::Sender;
use tokio::sync::RwLock;
use waterbus_proto::{
    AddPublisherCandidateRequest, AddSubscriberCandidateRequest, JoinRoomRequest, JoinRoomResponse,
    LeaveRoomRequest, PublisherRenegotiationRequest, PublisherRenegotiationResponse, SetCameraType,
    SetEnabledRequest, SetScreenSharingRequest, SetSubscriberSdpRequest, SubscribeRequest,
    SubscribeResponse,
};

use crate::{
    application::sfu_grpc_client::SfuGrpcClient,
    domain::DispatcherCallback,
    infrastructure::{
        cache::cache::{CacheKey, CacheManager, ClientMetadata},
        etcd::EtcdDispatcher,
        grpc::grpc::GrpcServer,
    },
};

pub struct DispatcherConfigs {
    pub group_id: String,
    pub dispatcher_port: u16,
    pub sfu_port: u16,
    pub redis_uri: String,
    pub etcd_uri: String,
    pub sender: Sender<DispatcherCallback>,
}

#[derive(Clone)]
pub struct DispatcherManager {
    sfu_grpc_client: SfuGrpcClient,
    cache_manager: CacheManager,
    etcd_dispatcher: Arc<RwLock<EtcdDispatcher>>,
    sfu_port: u16,
}

impl DispatcherManager {
    pub async fn new(configs: DispatcherConfigs) -> Self {
        GrpcServer::start(configs.dispatcher_port, configs.sender.clone());

        let etcd_dispatcher = EtcdDispatcher::new(
            &[&configs.etcd_uri],
            "/sfu/nodes",
            &configs.group_id,
            configs.sender,
        )
        .await
        .unwrap();

        let sfu_grpc_client = SfuGrpcClient::default();
        let cache_manager = CacheManager::new(configs.redis_uri);

        Self {
            sfu_grpc_client,
            cache_manager,
            etcd_dispatcher: Arc::new(RwLock::new(etcd_dispatcher)),
            sfu_port: configs.sfu_port,
        }
    }

    pub async fn join_room(&self, req: JoinRoomRequest) -> Result<JoinRoomResponse, anyhow::Error> {
        let etcd_writer = self.etcd_dispatcher.read().await;

        let result = etcd_writer.get_node_least();

        match result {
            Some((node_id, metadata)) => {
                let server_addr = format!("{}:{}", metadata.addr, self.sfu_port);
                let response = self
                    .sfu_grpc_client
                    .join_room(server_addr, req.clone())
                    .await;

                match response {
                    Ok(resp) => {
                        let cache_key = CacheKey::new(req.client_id);
                        let client_metadata = ClientMetadata {
                            room_id: req.room_id,
                            participant_id: req.participant_id,
                            sfu_node_id: node_id,
                            node_addr: metadata.addr,
                        };
                        let _ = self.cache_manager.insert(cache_key, &client_metadata);

                        return Ok(resp.into_inner());
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "Failed to join room on node {}: {}",
                            node_id,
                            e
                        ));
                    }
                }
            }
            None => {
                return Err(anyhow::anyhow!("No available SFU node found!"));
            }
        }
    }

    pub async fn subscribe(
        &self,
        req: SubscribeRequest,
    ) -> Result<SubscribeResponse, anyhow::Error> {
        let client = self.cache_manager.get_by_participant_id(&req.target_id);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let node_id = client.sfu_node_id;
                    let node_addr = client.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self.sfu_grpc_client.subscribe(server_addr, req).await;

                    match response {
                        Ok(resp) => {
                            return Ok(resp.into_inner());
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn set_subscribe_sdp(
        &self,
        req: SetSubscriberSdpRequest,
    ) -> Result<(), anyhow::Error> {
        let client = self.cache_manager.get_by_participant_id(&req.target_id);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let node_id = client.sfu_node_id;
                    let node_addr = client.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self
                        .sfu_grpc_client
                        .set_subscriber_sdp(server_addr, req)
                        .await;

                    match response {
                        Ok(_) => {
                            return Ok(());
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn publisher_renegotiate(
        &self,
        req: PublisherRenegotiationRequest,
    ) -> Result<PublisherRenegotiationResponse, anyhow::Error> {
        let cache_key = CacheKey::new(req.clone().client_id);
        let client = self.cache_manager.get(&cache_key);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let node_id = client.sfu_node_id;
                    let node_addr = client.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self
                        .sfu_grpc_client
                        .publisher_renegotiation(server_addr, req)
                        .await;

                    match response {
                        Ok(resp) => {
                            return Ok(resp.into_inner());
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn add_publisher_candidate(
        &self,
        req: AddPublisherCandidateRequest,
    ) -> Result<(), anyhow::Error> {
        let cache_key = CacheKey::new(req.clone().client_id);
        let client = self.cache_manager.get(&cache_key);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let node_id = client.sfu_node_id;
                    let node_addr = client.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self
                        .sfu_grpc_client
                        .add_publisher_candidate(server_addr, req)
                        .await;

                    match response {
                        Ok(_) => {
                            return Ok(());
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn add_subscriber_candidate(
        &self,
        req: AddSubscriberCandidateRequest,
    ) -> Result<(), anyhow::Error> {
        let cache_key = CacheKey::new(req.clone().client_id);
        let client = self.cache_manager.get(&cache_key);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let node_id = client.sfu_node_id;
                    let node_addr = client.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self
                        .sfu_grpc_client
                        .add_subscriber_candidate(server_addr, req)
                        .await;

                    match response {
                        Ok(_) => {
                            return Ok(());
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn leave_room(&self, req: LeaveRoomRequest) -> Result<ClientMetadata, anyhow::Error> {
        let cache_key = CacheKey::new(req.clone().client_id);
        let client = self.cache_manager.get(&cache_key);

        let _ = self.cache_manager.remove(&cache_key);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let node_addr = client.clone().node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let _ = self.sfu_grpc_client.leave_room(server_addr, req).await;

                    return Ok(client);
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn set_video_enabled(
        &self,
        req: SetEnabledRequest,
    ) -> Result<ClientMetadata, anyhow::Error> {
        let cache_key = CacheKey::new(req.clone().client_id);
        let client = self.cache_manager.get(&cache_key);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let client_clone = client.clone();

                    let node_id = client_clone.sfu_node_id;
                    let node_addr = client_clone.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self
                        .sfu_grpc_client
                        .set_video_enabled(server_addr, req)
                        .await;

                    match response {
                        Ok(_) => {
                            return Ok(client);
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn set_audio_enabled(
        &self,
        req: SetEnabledRequest,
    ) -> Result<ClientMetadata, anyhow::Error> {
        let cache_key = CacheKey::new(req.clone().client_id);
        let client = self.cache_manager.get(&cache_key);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let client_clone = client.clone();
                    let node_id = client_clone.sfu_node_id;
                    let node_addr = client_clone.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self
                        .sfu_grpc_client
                        .set_audio_enabled(server_addr, req)
                        .await;

                    match response {
                        Ok(_) => {
                            return Ok(client);
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn set_hand_raising(
        &self,
        req: SetEnabledRequest,
    ) -> Result<ClientMetadata, anyhow::Error> {
        let cache_key = CacheKey::new(req.clone().client_id);
        let client = self.cache_manager.get(&cache_key);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let client_clone = client.clone();
                    let node_id = client_clone.sfu_node_id;
                    let node_addr = client_clone.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self
                        .sfu_grpc_client
                        .set_hand_raising(server_addr, req)
                        .await;

                    match response {
                        Ok(_) => {
                            return Ok(client);
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn set_screen_sharing(
        &self,
        req: SetScreenSharingRequest,
    ) -> Result<ClientMetadata, anyhow::Error> {
        let cache_key = CacheKey::new(req.clone().client_id);
        let client = self.cache_manager.get(&cache_key);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let client_clone = client.clone();
                    let node_id = client_clone.sfu_node_id;
                    let node_addr = client_clone.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self
                        .sfu_grpc_client
                        .set_screen_sharing(server_addr, req)
                        .await;

                    match response {
                        Ok(_) => {
                            return Ok(client);
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }

    pub async fn set_camera_type(
        &self,
        req: SetCameraType,
    ) -> Result<ClientMetadata, anyhow::Error> {
        let cache_key = CacheKey::new(req.clone().client_id);
        let client = self.cache_manager.get(&cache_key);

        match client {
            Ok(client) => {
                if let Some(client) = client {
                    let client_clone = client.clone();
                    let node_id = client_clone.sfu_node_id;
                    let node_addr = client_clone.node_addr;

                    let server_addr = format!("{}:{}", node_addr, self.sfu_port);

                    let response = self.sfu_grpc_client.set_camera_type(server_addr, req).await;

                    match response {
                        Ok(_) => {
                            return Ok(client);
                        }
                        Err(e) => {
                            return Err(anyhow::anyhow!(
                                "Failed to join room on node {}: {}",
                                node_id,
                                e
                            ));
                        }
                    }
                } else {
                    return Err(anyhow::anyhow!("Client not found!"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("Client not found!")),
        }
    }
}
