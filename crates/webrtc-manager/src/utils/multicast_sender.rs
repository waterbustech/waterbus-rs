use std::sync::Arc;

use crossbeam::channel::{self, Receiver, Sender};
use dashmap::DashMap;
use tracing::debug;

use crate::models::quality::TrackQuality;
use crate::models::rtp_foward_info::RtpForwardInfo;

/// Multi-cast sender wrapper for broadcasting to multiple receivers per quality layer
#[derive(Debug, Clone)]
pub struct MulticastSender {
    quality_senders: Arc<DashMap<TrackQuality, DashMap<String, Sender<RtpForwardInfo>>>>,
}

impl Default for MulticastSender {
    fn default() -> Self {
        Self::new()
    }
}

impl MulticastSender {
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

    /// Add a receiver for a specific quality (f/h/q)
    ///
    /// # Arguments
    ///
    /// * `quality` - The quality to add the receiver for
    /// * `id` - The id of the receiver
    ///
    pub fn add_receiver_for_quality(
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
    pub fn remove_receiver_for_quality(&self, quality: TrackQuality, id: &str) {
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
    pub fn send_to_quality(&self, quality: TrackQuality, info: RtpForwardInfo) {
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
    pub fn add_receiver(&self, id: String) -> Receiver<RtpForwardInfo> {
        self.add_receiver_for_quality(TrackQuality::Medium, id)
    }

    /// For non-simulcast legacy: remove a receiver from 'Medium' (h)
    ///
    /// # Arguments
    ///
    /// * `id` - The id of the receiver
    ///
    pub fn remove_receiver(&self, id: &str) {
        self.remove_receiver_for_quality(TrackQuality::Medium, id)
    }

    /// For non-simulcast legacy: send to 'Medium' (h)
    ///
    /// # Arguments
    ///
    /// * `info` - The RtpForwardInfo to send
    ///
    pub fn send(&self, info: RtpForwardInfo) {
        self.send_to_quality(TrackQuality::Medium, info);
    }

    /// Get the number of receivers for a specific quality
    ///
    /// # Arguments
    ///
    /// * `quality` - The quality to get the number of receivers for
    ///
    pub fn receiver_count(&self, quality: TrackQuality) -> usize {
        self.quality_senders
            .get(&quality)
            .map(|m| m.len())
            .unwrap_or(0)
    }
}
