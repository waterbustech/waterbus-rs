use std::sync::Arc;

use crossbeam::channel::{self, Receiver, Sender};
use dashmap::DashMap;
use tracing::debug;

use crate::models::quality::TrackQuality;
use crate::models::rtp_foward_info::RtpForwardInfo;

/// Trait for multicast sender
pub trait MulticastSender: Send + Sync {
    fn add_receiver_for_quality(
        &self,
        quality: TrackQuality,
        id: String,
    ) -> Receiver<RtpForwardInfo>;
    fn remove_receiver_for_quality(&self, quality: TrackQuality, id: &str);
    fn send_to_quality(&self, quality: TrackQuality, info: RtpForwardInfo);
    fn add_receiver(&self, id: String) -> Receiver<RtpForwardInfo>;
    fn remove_receiver(&self, id: &str);
    fn send(&self, info: RtpForwardInfo);
    fn receiver_count(&self, quality: TrackQuality) -> usize;
    fn clear(&self);
}

/// Multi-cast sender wrapper for broadcasting to multiple receivers per quality layer
#[derive(Debug, Clone)]
pub struct MulticastSenderImpl {
    quality_senders: Arc<DashMap<TrackQuality, DashMap<String, Sender<RtpForwardInfo>>>>,
}

impl Default for MulticastSenderImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl MulticastSenderImpl {
    /// Create a new MulticastSender
    pub fn new() -> Self {
        let quality_senders = DashMap::new();
        for q in [TrackQuality::High, TrackQuality::Medium, TrackQuality::Low] {
            quality_senders.insert(q, DashMap::new());
        }
        Self {
            quality_senders: Arc::new(quality_senders),
        }
    }
}

impl MulticastSender for MulticastSenderImpl {
    /// Add a receiver for a specific quality (f/h/q)
    ///
    /// # Arguments
    ///
    /// * `quality` - The quality to add the receiver for
    /// * `id` - The id of the receiver
    ///
    fn add_receiver_for_quality(
        &self,
        quality: TrackQuality,
        id: String,
    ) -> Receiver<RtpForwardInfo> {
        let (tx, rx) = channel::bounded(1024);
        if let Some(map) = self.quality_senders.get(&quality) {
            map.insert(id, tx);
        }
        rx
    }

    /// Remove a receiver for a specific quality
    ///
    /// # Arguments
    ///
    /// * `quality` - The quality to remove the receiver for
    /// * `id` - The id of the receiver
    ///
    fn remove_receiver_for_quality(&self, quality: TrackQuality, id: &str) {
        if let Some(map) = self.quality_senders.get(&quality) {
            map.remove(id);
        }
    }

    /// Send to all receivers for a specific quality
    ///
    /// # Arguments
    ///
    /// * `quality` - The quality to send to
    /// * `info` - The RtpForwardInfo to send
    ///
    fn send_to_quality(&self, quality: TrackQuality, info: RtpForwardInfo) {
        if let Some(map) = self.quality_senders.get(&quality) {
            let mut to_remove = Vec::new();
            for entry in map.iter() {
                match entry.value().try_send(info.clone()) {
                    Ok(_) => {}
                    Err(crossbeam::channel::TrySendError::Full(_)) => {
                        debug!(
                            "Channel full for receiver {} (quality {:?}), dropping packet",
                            entry.key(),
                            quality
                        );
                    }
                    Err(crossbeam::channel::TrySendError::Disconnected(_)) => {
                        to_remove.push(entry.key().clone());
                    }
                }
            }
            for id in to_remove {
                map.remove(&id);
            }
        }
    }

    /// For non-simulcast legacy: add a receiver to 'Medium' (h)
    ///
    /// # Arguments
    ///
    /// * `id` - The id of the receiver
    ///
    fn add_receiver(&self, id: String) -> Receiver<RtpForwardInfo> {
        self.add_receiver_for_quality(TrackQuality::Medium, id)
    }

    /// For non-simulcast legacy: remove a receiver from 'Medium' (h)
    ///
    /// # Arguments
    ///
    /// * `id` - The id of the receiver
    ///
    fn remove_receiver(&self, id: &str) {
        self.remove_receiver_for_quality(TrackQuality::Medium, id)
    }

    /// For non-simulcast legacy: send to 'Medium' (h)
    ///
    /// # Arguments
    ///
    /// * `info` - The RtpForwardInfo to send
    ///
    fn send(&self, info: RtpForwardInfo) {
        self.send_to_quality(TrackQuality::Medium, info);
    }

    /// Get the number of receivers for a specific quality
    ///
    /// # Arguments
    ///
    /// * `quality` - The quality to get the number of receivers for
    ///
    fn receiver_count(&self, quality: TrackQuality) -> usize {
        self.quality_senders
            .get(&quality)
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// Clear the multicast sender
    fn clear(&self) {
        self.quality_senders.clear();
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::models::quality::TrackQuality;
    use crate::models::rtp_foward_info::RtpForwardInfo;
    use std::sync::Arc;

    // Dummy Packet for testing
    fn dummy_packet() -> Arc<webrtc::rtp::packet::Packet> {
        Arc::new(webrtc::rtp::packet::Packet {
            header: Default::default(),
            payload: Bytes::from_static(b"123"),
        })
    }

    fn dummy_info(quality: TrackQuality) -> RtpForwardInfo {
        RtpForwardInfo {
            packet: dummy_packet(),
            is_svc: false,
            track_quality: quality,
        }
    }

    #[test]
    fn test_add_and_remove_receiver_for_quality() {
        let sender = MulticastSenderImpl::new();
        let rx = sender.add_receiver_for_quality(TrackQuality::High, "id1".to_string());
        assert_eq!(sender.receiver_count(TrackQuality::High), 1);
        sender.remove_receiver_for_quality(TrackQuality::High, "id1");
        assert_eq!(sender.receiver_count(TrackQuality::High), 0);
        drop(rx); // avoid unused warning
    }

    #[test]
    fn test_send_to_quality_and_receive() {
        let sender = MulticastSenderImpl::new();
        let rx = sender.add_receiver_for_quality(TrackQuality::Medium, "id2".to_string());
        let info = dummy_info(TrackQuality::Medium);
        sender.send_to_quality(TrackQuality::Medium, info.clone());
        let received = rx.try_recv().unwrap();
        assert_eq!(received.track_quality, TrackQuality::Medium);
    }

    #[test]
    fn test_send_to_quality_channel_full_removes_none() {
        let sender = MulticastSenderImpl::new();
        let rx = sender.add_receiver_for_quality(TrackQuality::Low, "id3".to_string());
        // Fill the channel
        for _ in 0..1024 {
            sender.send_to_quality(TrackQuality::Low, dummy_info(TrackQuality::Low));
        }
        // This send should be dropped (channel full), but not remove the receiver
        sender.send_to_quality(TrackQuality::Low, dummy_info(TrackQuality::Low));
        assert_eq!(sender.receiver_count(TrackQuality::Low), 1);
        drop(rx);
    }

    #[test]
    fn test_send_to_quality_channel_disconnected_removes_receiver() {
        let sender = MulticastSenderImpl::new();
        let rx = sender.add_receiver_for_quality(TrackQuality::Low, "id4".to_string());
        drop(rx); // disconnect receiver
        sender.send_to_quality(TrackQuality::Low, dummy_info(TrackQuality::Low));
        assert_eq!(sender.receiver_count(TrackQuality::Low), 0);
    }

    #[test]
    fn test_add_and_remove_receiver_legacy() {
        let sender = MulticastSenderImpl::new();
        let rx = sender.add_receiver("legacy1".to_string());
        assert_eq!(sender.receiver_count(TrackQuality::Medium), 1);
        sender.remove_receiver("legacy1");
        assert_eq!(sender.receiver_count(TrackQuality::Medium), 0);
        drop(rx);
    }

    #[test]
    fn test_send_legacy() {
        let sender = MulticastSenderImpl::new();
        let rx = sender.add_receiver("legacy2".to_string());
        let info = dummy_info(TrackQuality::Medium);
        sender.send(info.clone());
        let received = rx.try_recv().unwrap();
        assert_eq!(received.track_quality, TrackQuality::Medium);
    }

    #[test]
    fn test_clear() {
        let sender = MulticastSenderImpl::new();
        let _rx1 = sender.add_receiver_for_quality(TrackQuality::High, "id5".to_string());
        let _rx2 = sender.add_receiver_for_quality(TrackQuality::Medium, "id6".to_string());
        let _rx3 = sender.add_receiver_for_quality(TrackQuality::Low, "id7".to_string());
        sender.clear();
        assert_eq!(sender.receiver_count(TrackQuality::High), 0);
        assert_eq!(sender.receiver_count(TrackQuality::Medium), 0);
        assert_eq!(sender.receiver_count(TrackQuality::Low), 0);
    }
}
