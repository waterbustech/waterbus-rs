use std::time::Duration;
use tokio::task::JoinHandle;
use tonic::{Request, Status, transport::Channel};
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
    pub fn new(port: u16) -> Self {
        let this = Self { port, client: None };

        this.clone().start_client();

        this
    }

    pub fn start_client(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                match DispatcherServiceClient::connect(format!("[::1]:{}", self.port)).await {
                    Ok(client) => {
                        println!("Dispatcher client connected successfully.");
                        self.client.replace(client);

                        // Keep the connection alive and handle potential disconnections
                        let mut interval = tokio::time::interval(Duration::from_secs(5)); // Check connection periodically
                        loop {
                            interval.tick().await;
                            if self.client.is_none() {
                                println!(
                                    "Dispatcher client disconnected, attempting to reconnect..."
                                );
                                break;
                            }
                            // You could add a simple ping or health check here if the service supports it
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to connect to dispatcher: {:?}", e);
                        eprintln!("Retrying in 1 second...");
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
                    eprintln!("Error sending new_user_joined: {:?}", e);
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
                    eprintln!("Error sending subscriber_renegotiate: {:?}", e);
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
                Ok(_) => Ok(()),
                Err(e) => {
                    eprintln!("Error sending on_publisher_candidate: {:?}", e);
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
                    eprintln!("Error sending on_subscriber_candidate: {:?}", e);
                    self.client = None; // Mark as disconnected
                    Err(e)
                }
            }
        } else {
            Err(Status::unavailable("Dispatcher client not connected"))
        }
    }
}
