use std::sync::Arc;

use crossbeam_channel::{self, Receiver, Sender};
use dashmap::DashMap;

use crate::models::quality::TrackQuality;
use crate::models::rtp_foward_info::RtpForwardInfo;

const MAX_BUFFER: usize = 5;

#[derive(Debug, Clone)]
pub struct CrossbeamChannel<T> {
    tx: Sender<T>,
    rx: Receiver<T>,
}

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
    quality_senders: Arc<DashMap<TrackQuality, DashMap<String, CrossbeamChannel<RtpForwardInfo>>>>,
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
        let (tx, rx) = crossbeam_channel::bounded(MAX_BUFFER);
        if let Some(map) = self.quality_senders.get(&quality) {
            map.insert(id, CrossbeamChannel { tx, rx: rx.clone() });
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
                let CrossbeamChannel { tx, rx } = entry.value();
                match tx.try_send(info.clone()) {
                    Ok(_) => {}
                    Err(crossbeam_channel::TrySendError::Full(_)) => {
                        let _ = rx.try_recv();
                        tx.send(info.clone()).unwrap();
                    }
                    Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
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
