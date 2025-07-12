use std::sync::{Arc, Weak};

use bytes::Bytes;
use webrtc::data_channel::RTCDataChannel;

use crate::{
    entities::{media::TrackSubscribedCallback, publisher::Publisher},
    models::data_channel_msg::TrackSubscribedMessage,
};

impl Publisher {
    #[allow(clippy::all)]
    pub async fn setup_media_communication(&mut self, publisher_weak: Weak<Publisher>) {
        let _ = publisher_weak;
        let media_clone = self.media.clone();

        // Create event channel
        let receiver = {
            let mut media = media_clone.write();
            media.create_event_channel()
        };
        self.track_event_receiver = Some(receiver);

        // Set up callback
        let data_channel_weak = if let Some(ref dc) = self.data_channel {
            Arc::downgrade(dc)
        } else {
            return;
        };

        let callback: TrackSubscribedCallback = Arc::new(move |message| {
            if let Some(dc) = data_channel_weak.upgrade() {
                let dc_clone = dc.clone();
                let msg_clone = message.clone();
                tokio::spawn(async move {
                    if let Err(e) = Self::send_track_subscribed_message(&dc_clone, msg_clone).await
                    {
                        tracing::error!("Failed to send track subscribed message: {}", e);
                    }
                });
            }
        });

        {
            let mut media = media_clone.write();
            media.set_track_subscribed_callback(callback);
        }

        {
            let media = media_clone.write();
            media.start_track_monitoring().await;
        }
    }

    pub async fn start_data_channel_handler(&mut self) {
        if let Some(mut receiver) = self.track_event_receiver.take() {
            let data_channel = self.data_channel.clone();
            let cancel_token = self.cancel_token.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel_token.cancelled() => {
                            break;
                        }
                        message = receiver.recv() => {
                            if let Some(msg) = message {
                                if let Some(ref dc) = data_channel {
                                    if let Err(e) = Self::send_track_subscribed_message(dc, msg).await {
                                        tracing::error!("Failed to send track subscribed via data channel: {}", e);
                                    }
                                }
                            } else {
                                break; // Channel closed
                            }
                        }
                    }
                }
            });
        }
    }

    async fn send_track_subscribed_message(
        data_channel: &Arc<RTCDataChannel>,
        message: TrackSubscribedMessage,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let json_string = serde_json::to_string(&message)?;
        let bytes_message = json_string.into_bytes();
        data_channel.send(&Bytes::from(bytes_message)).await?;

        tracing::info!("====> sent track quality over data channel");
        Ok(())
    }

    pub async fn create_data_channel(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let data_channel = self
            .peer_connection
            .create_data_channel("track_quality", None)
            .await?;
        self.data_channel = Some(data_channel);
        Ok(())
    }
}
