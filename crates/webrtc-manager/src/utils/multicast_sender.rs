use std::sync::Arc;

use dashmap::DashMap;
use kanal::{self, AsyncReceiver, AsyncSender};

use crate::models::quality::TrackQuality;
use crate::models::rtp_foward_info::RtpForwardInfo;
use std::sync::Mutex;
use std::thread;

const MAX_BUFFER: usize = 1;

/// Trait for multicast sender
pub trait MulticastSender: Send + Sync {
    fn add_receiver_for_quality(
        &self,
        quality: TrackQuality,
        id: String,
    ) -> AsyncReceiver<RtpForwardInfo>;
    fn remove_receiver_for_quality(&self, quality: TrackQuality, id: &str);
    fn send_to_quality(&self, quality: TrackQuality, info: RtpForwardInfo);
    fn clear(&self);
}

/// Multi-cast sender wrapper for broadcasting to multiple receivers per quality layer
#[derive(Debug, Clone)]
pub struct MulticastSenderImpl {
    quality_senders: Arc<DashMap<TrackQuality, DashMap<String, AsyncSender<RtpForwardInfo>>>>,
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
    #[inline]
    fn add_receiver_for_quality(
        &self,
        quality: TrackQuality,
        id: String,
    ) -> AsyncReceiver<RtpForwardInfo> {
        let (tx, rx) = kanal::bounded_async(MAX_BUFFER);
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
    #[inline]
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
            let to_remove = Arc::new(Mutex::new(Vec::new()));

            thread::scope(|s| {
                for entry in map.iter() {
                    let tx = entry.value().clone_sync();
                    let id = entry.key().clone();
                    let info = info.clone();
                    let to_remove = Arc::clone(&to_remove);

                    s.spawn(move || match tx.send(info) {
                        Ok(_) => {}
                        Err(kanal::SendError::ReceiveClosed) | Err(kanal::SendError::Closed) => {
                            to_remove.lock().unwrap().push(id);
                        }
                    });
                }
            });

            // Remove closed channels
            let to_remove = to_remove.lock().unwrap();
            for id in to_remove.iter() {
                map.remove(id);
            }
        }
    }

    /// Clear the multicast sender
    #[inline]
    fn clear(&self) {
        self.quality_senders.clear();
    }
}
