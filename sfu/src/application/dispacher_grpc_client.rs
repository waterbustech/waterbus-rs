use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, task::JoinHandle};
use tonic::{Request, Status, transport::Channel};
use tracing::{info, warn};
use waterbus_proto::{
    NewUserJoinedRequest, PublisherCandidateRequest, SubscriberCandidateRequest,
    SubscriberRenegotiateRequest, dispatcher_service_client::DispatcherServiceClient,
};

#[derive(Debug, Clone)]
pub struct DispatcherGrpcClient {
    port: u16,
    client: Option<DispatcherServiceClient<Channel>>,
}

impl DispatcherGrpcClient {
    pub fn new(port: u16) -> Arc<Mutex<Self>> {
        let client = Arc::new(Mutex::new(Self { port, client: None }));
        DispatcherGrpcClient::start_client(client.clone());
        client
    }

    fn start_client(this: Arc<Mutex<Self>>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let port;
                {
                    let locked = this.lock().await;
                    port = locked.port;
                }

                match DispatcherServiceClient::connect(format!("http://[::1]:{}", port)).await {
                    Ok(client) => {
                        info!("Dispatcher client connected successfully.");
                        {
                            let mut locked = this.lock().await;
                            locked.client.replace(client);
                        }

                        let mut interval = tokio::time::interval(Duration::from_secs(5));
                        loop {
                            interval.tick().await;

                            let disconnected = {
                                let locked = this.lock().await;
                                locked.client.is_none()
                            };

                            if disconnected {
                                warn!("Dispatcher client disconnected, attempting to reconnect...");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to connect to dispatcher: {:?}", e);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        })
    }

    pub async fn new_user_joined(&mut self, req: NewUserJoinedRequest) -> Result<(), Status> {
        if let Some(ref mut client) = self.client {
            let request = Request::new(req);
            match client.new_user_joined(request).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    warn!("Error sending new_user_joined: {:?}", e);
                    self.client = None; // Mark as disconnected
                    Err(e)
                }
            }
        } else {
            Err(Status::unavailable("Dispatcher client not connected"))
        }
    }

    pub async fn subsriber_renegotiate(
        &mut self,
        req: SubscriberRenegotiateRequest,
    ) -> Result<(), Status> {
        if let Some(ref mut client) = self.client {
            let request = Request::new(req);
            match client.subscriber_renegotiate(request).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    warn!("Error sending subscriber_renegotiate: {:?}", e);
                    self.client = None; // Mark as disconnected
                    Err(e)
                }
            }
        } else {
            Err(Status::unavailable("Dispatcher client not connected"))
        }
    }

    pub async fn on_publisher_candidate(
        &mut self,
        req: PublisherCandidateRequest,
    ) -> Result<(), Status> {
        if let Some(ref mut client) = self.client {
            let request = Request::new(req);
            match client.on_publisher_candidate(request).await {
                Ok(_) => {
                    return Ok(());
                }
                Err(e) => {
                    warn!("Error sending on_publisher_candidate: {:?}", e);
                    self.client = None; // Mark as disconnected
                    Err(e)
                }
            }
        } else {
            Err(Status::unavailable("Dispatcher client not connected"))
        }
    }

    pub async fn on_subscriber_candidate(
        &mut self,
        req: SubscriberCandidateRequest,
    ) -> Result<(), Status> {
        if let Some(ref mut client) = self.client {
            let request = Request::new(req);
            match client.on_subscriber_candidate(request).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    warn!("Error sending on_subscriber_candidate: {:?}", e);
                    self.client = None; // Mark as disconnected
                    Err(e)
                }
            }
        } else {
            Err(Status::unavailable("Dispatcher client not connected"))
        }
    }
}
