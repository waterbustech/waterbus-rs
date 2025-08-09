// use std::sync::Weak;

// use crate::{
//     entities::publisher::Publisher,
//     models::data_channel_msg::TrackSubscribedMessage,
// };

// impl Publisher {
//     pub async fn setup_subscriber_communication(&self, _publisher_weak: Weak<Publisher>) {
//         // TODO: Set up communication channels between publisher and its subscribers
//         // This is where the publisher establishes control over its subscribers
        
//         if let Some(sender) = &self.track_event_sender {
//             // Example: Send initial track information to subscribers
//             let message = TrackSubscribedMessage {
//                 track_id: format!("{}_video", self.participant_id),
//                 subscribed_count: 0,
//                 quality: None,
//                 timestamp: std::time::SystemTime::now()
//                     .duration_since(std::time::UNIX_EPOCH)
//                     .unwrap()
//                     .as_millis() as u64,
//             };
            
//             let _ = sender.send(message);
//         }
//     }

//     pub async fn notify_subscribers_track_added(&self, track_id: String) {
//         if let Some(sender) = &self.track_event_sender {
//             let message = TrackSubscribedMessage {
//                 track_id,
//                 subscribed_count: self.subscribers.len() as u32,
//                 quality: None,
//                 timestamp: std::time::SystemTime::now()
//                     .duration_since(std::time::UNIX_EPOCH)
//                     .unwrap()
//                     .as_millis() as u64,
//             };
            
//             let _ = sender.send(message);
//         }
//     }

//     pub async fn notify_subscribers_track_removed(&self, track_id: String) {
//         if let Some(sender) = &self.track_event_sender {
//             let message = TrackSubscribedMessage {
//                 track_id,
//                 subscribed_count: 0,
//                 quality: None,
//                 timestamp: std::time::SystemTime::now()
//                     .duration_since(std::time::UNIX_EPOCH)
//                     .unwrap()
//                     .as_millis() as u64,
//             };
            
//             let _ = sender.send(message);
//         }
//     }

//     pub async fn handle_subscriber_quality_request(&self, subscriber_id: &str, track_id: &str, quality: crate::models::quality::TrackQuality) {
//         // Publisher controls subscriber quality
//         tracing::debug!("Publisher {} handling quality request from subscriber {} for track {}: {:?}", 
//                        self.participant_id, subscriber_id, track_id, quality);
        
//         // Update track quality for this subscriber
//         if let Some(track) = self.tracks.get(track_id) {
//             track.update_subscriber_quality(subscriber_id, quality);
//         }
        
//         // Notify about quality change
//         if let Some(sender) = &self.track_event_sender {
//             let message = TrackSubscribedMessage {
//                 track_id: track_id.to_string(),
//                 subscribed_count: self.subscribers.len() as u32,
//                 quality: Some(quality),
//                 timestamp: std::time::SystemTime::now()
//                     .duration_since(std::time::UNIX_EPOCH)
//                     .unwrap()
//                     .as_millis() as u64,
//             };
            
//             let _ = sender.send(message);
//         }
//     }

//     pub async fn broadcast_to_subscribers(&self, data: &[u8]) {
//         // Publisher broadcasts data to all its subscribers
//         for subscriber_entry in self.subscribers.iter() {
//             let subscriber = subscriber_entry.value();
//             if let Err(e) = subscriber.receive_media_data(data).await {
//                 tracing::error!("Failed to send data to subscriber {}: {:?}", 
//                                subscriber.participant_id, e);
//             }
//         }
//     }
// }
