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

    /// Clear the multicast sender
    fn clear(&self) {
        self.quality_senders.clear();
    }
}
