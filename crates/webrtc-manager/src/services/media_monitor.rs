use std::{collections::HashMap, sync::atomic::Ordering, time::Duration};

use tokio::sync::mpsc;
use tracing::info;

use crate::{
    entities::media::{Media, TrackSubscribedCallback},
    models::{data_channel_msg::TrackSubscribedMessage, quality::TrackQuality}, services::track_monitor::TrackSubscribed,
};

impl Media {
    pub fn set_track_subscribed_callback(&mut self, callback: TrackSubscribedCallback) {
        self.track_subscribed_callback = Some(callback);
    }

    pub fn create_event_channel(&mut self) -> mpsc::UnboundedReceiver<TrackSubscribedMessage> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.track_event_sender = Some(sender);
        receiver
    }

    pub async fn start_track_monitoring(&self) {
        let tracks = self.tracks.clone();
        let callback = self.track_subscribed_callback.clone();
        let event_sender = self.track_event_sender.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            let mut last_subscribed_states: HashMap<String, TrackSubscribed> = HashMap::new();

            loop {
                interval.tick().await;

                for entry in tracks.iter() {
                    let track_id = entry.key().clone();
                    let track_mutex = entry.value().clone();
                    let track = track_mutex.read();

                    if !track.is_simulcast.load(Ordering::Relaxed) {
                        continue;
                    }

                    let current_subscribed = track.get_track_subscribed();
                    let has_changed =
                        if let Some(last_state) = last_subscribed_states.get(&track_id) {
                            // Check if any layer's subscriber count changed
                            Self::track_subscribed_changed(last_state, &current_subscribed)
                        } else {
                            true // First time, always send
                        };

                    // Only send update if subscription state changed
                    if has_changed {
                        // Calculate total subscriber count across all layers
                        let total_subscribers: usize = current_subscribed
                            .layers
                            .values()
                            .map(|layer| layer.subscriber_count)
                            .sum();

                        // Find the highest quality layer with subscribers
                        let active_quality = Self::get_highest_active_quality(&current_subscribed);

                        let message = TrackSubscribedMessage {
                            track_id: track_id.clone(),
                            subscribed_count: total_subscribers as u32,
                            quality: active_quality,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64,
                        };

                        // Send via callback if available
                        if let Some(ref cb) = callback {
                            cb(message.clone());
                        }

                        // Send via internal channel if available
                        if let Some(ref sender) = event_sender {
                            let _ = sender.send(message);
                        }

                        last_subscribed_states.insert(track_id.clone(), current_subscribed);

                        info!(
                            "[track_subscribed_changed]: track_id: {}, total_count: {}",
                            track_id, total_subscribers,
                        );
                    }
                }
            }
        });
    }

    // Helper function to check if track subscription state changed
    fn track_subscribed_changed(old_state: &TrackSubscribed, new_state: &TrackSubscribed) -> bool {
        // Check if simulcast status changed
        if old_state.is_simulcast != new_state.is_simulcast {
            return true;
        }

        // Check if layer count changed
        if old_state.layers.len() != new_state.layers.len() {
            return true;
        }

        // Check if any layer's subscriber count changed
        for (quality, new_layer) in &new_state.layers {
            if let Some(old_layer) = old_state.layers.get(quality) {
                if old_layer.subscriber_count != new_layer.subscriber_count {
                    return true;
                }
            } else {
                return true; // New layer added
            }
        }

        false
    }

    // Helper function to get the highest quality layer with active subscribers
    fn get_highest_active_quality(track_subscribed: &TrackSubscribed) -> Option<TrackQuality> {
        if !track_subscribed.is_simulcast {
            return None;
        }

        // Define quality priority (highest to lowest)
        let quality_priority = [TrackQuality::High, TrackQuality::Medium, TrackQuality::Low];

        for quality in &quality_priority {
            if let Some(layer) = track_subscribed.layers.get(quality) {
                if layer.subscriber_count > 0 {
                    return Some(quality.clone());
                }
            }
        }

        None
    }
}
