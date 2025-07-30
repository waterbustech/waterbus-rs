use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::RwLock;
use str0m::{
    media::{KeyframeRequest, MediaData, MediaKind, Mid},
    Rtc,
};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::{
    entities::media::{Media, Subscriber},
    models::{
        connection_type::ConnectionType,
        input_params::{JoinRoomParams, SubscribeParams},
        streaming_protocol::StreamingProtocol,
    },
};

pub struct Publisher {
    pub rtc: Arc<Rtc>,
    pub media: Arc<RwLock<Media>>,
    pub connection_type: AtomicU8,
    pub cancel_token: CancellationToken,
}

impl Publisher {
    pub async fn new(
        rtc: Arc<Rtc>,
        params: JoinRoomParams,
    ) -> Arc<Self> {
        let media = Media::new(
            params.participant_id.clone(),
            rtc.clone(),
            params.connection_type.clone(),
            params.streaming_protocol,
            params.is_ipv6_supported,
        );

        let publisher = Arc::new(Self {
            rtc,
            media: Arc::new(RwLock::new(media)),
            connection_type: AtomicU8::new(params.connection_type.into()),
            cancel_token: CancellationToken::new(),
        });

        // Start the media forwarding loop
        let publisher_clone = Arc::clone(&publisher);
        tokio::spawn(async move {
            publisher_clone.run_media_loop().await;
        });

        publisher
    }

    async fn run_media_loop(&self) {
        let mut buf = vec![0u8; 1500];
        
        loop {
            if self.cancel_token.is_cancelled() {
                break;
            }

            // Poll for media data from the RTC
            match self.rtc.poll_output() {
                Ok(output) => {
                    match output {
                        str0m::Output::MediaData(media_data) => {
                            // Forward media to all subscribers
                            self.forward_media_to_subscribers(media_data).await;
                        }
                        str0m::Output::KeyframeRequest(req) => {
                            // Handle keyframe requests
                            self.handle_keyframe_request(req).await;
                        }
                        _ => {
                            // Handle other outputs as needed
                        }
                    }
                }
                Err(_) => {
                    // RTC is no longer alive
                    break;
                }
            }

            // Small delay to prevent busy waiting
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    async fn forward_media_to_subscribers(&self, media_data: MediaData) {
        let media = self.media.read();
        media.forward_media_to_subscribers(media_data);
    }

    async fn handle_keyframe_request(&self, req: KeyframeRequest) {
        let media = self.media.read();
        media.handle_keyframe_request(req);
    }

    pub async fn add_subscriber(&self, subscriber_id: String, subscriber: Arc<Subscriber>) {
        let mut media = self.media.write();
        media.add_subscriber(subscriber_id, subscriber);
    }

    pub async fn remove_subscriber(&self, subscriber_id: &str) {
        let mut media = self.media.write();
        media.remove_subscriber(subscriber_id);
    }

    pub async fn subscribe_to_publisher(
        &self,
        params: SubscribeParams,
    ) -> Result<Arc<Subscriber>, crate::errors::WebRTCError> {
        // Create a new RTC instance for the subscriber
        let subscriber_rtc = Arc::new(str0m::Rtc::builder().build());
        
        // Create subscriber
        let subscriber = Arc::new(Subscriber::new(
            params.participant_id.clone(),
            subscriber_rtc,
        ));

        // Add subscriber to this publisher
        self.add_subscriber(params.participant_id, subscriber.clone()).await;

        Ok(subscriber)
    }

    #[inline]
    pub fn set_connection_type(&self, connection_type: ConnectionType) {
        self.connection_type
            .store(connection_type.into(), Ordering::Relaxed);
    }

    #[inline]
    pub fn get_connection_type(&self) -> ConnectionType {
        self.connection_type.load(Ordering::Relaxed).into()
    }

    #[inline]
    pub fn close(&self) {
        self.cancel_token.cancel();
        
        // Stop the media
        let mut media = self.media.write();
        media.stop();
    }

    pub fn get_media_state(&self) -> crate::entities::media::MediaState {
        let media = self.media.read();
        let state = media.state.read();
        state.clone()
    }

    pub fn set_video_enabled(&self, enabled: bool) {
        let mut media = self.media.write();
        media.set_video_enabled(enabled);
    }

    pub fn set_audio_enabled(&self, enabled: bool) {
        let mut media = self.media.write();
        media.set_audio_enabled(enabled);
    }

    pub fn set_e2ee_enabled(&self, enabled: bool) {
        let mut media = self.media.write();
        media.set_e2ee_enabled(enabled);
    }

    pub fn set_camera_type(&self, camera_type: u8) {
        let mut media = self.media.write();
        media.set_camera_type(camera_type);
    }

    pub fn set_screen_sharing(&self, enabled: bool, screen_track_id: Option<String>) {
        let mut media = self.media.write();
        media.set_screen_sharing(enabled, screen_track_id);
    }

    pub fn set_hand_raising(&self, enabled: bool) {
        let mut media = self.media.write();
        media.set_hand_raising(enabled);
    }
}
