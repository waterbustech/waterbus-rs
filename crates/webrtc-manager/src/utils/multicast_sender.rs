use std::sync::Arc;

use crossbeam::channel::{self, Receiver, Sender};
use dashmap::DashMap;
use tracing::debug;

use crate::models::rtp_foward_info::RtpForwardInfo;

// Multi-cast sender wrapper for broadcasting to multiple receivers
#[derive(Debug, Clone)]
pub struct MulticastSender {
    senders: Arc<DashMap<String, Sender<RtpForwardInfo>>>,
}

impl MulticastSender {
    pub fn new() -> Self {
        Self {
            senders: Arc::new(DashMap::new()),
        }
    }

    pub fn add_receiver(&self, id: String) -> Receiver<RtpForwardInfo> {
        // Use bounded channel with reasonable buffer size
        let (tx, rx) = channel::bounded(1024);
        self.senders.insert(id, tx);
        rx
    }

    pub fn remove_receiver(&self, id: &str) {
        self.senders.remove(id);
    }

    pub fn send(&self, info: RtpForwardInfo) {
        // Remove any disconnected senders and send to active ones
        let mut to_remove = Vec::new();

        for entry in self.senders.iter() {
            match entry.value().try_send(info.clone()) {
                Ok(_) => {} // Success
                Err(crossbeam::channel::TrySendError::Full(_)) => {
                    // Channel full, drop this packet for this receiver
                    debug!("Channel full for receiver {}, dropping packet", entry.key());
                }
                Err(crossbeam::channel::TrySendError::Disconnected(_)) => {
                    // Receiver disconnected, mark for removal
                    to_remove.push(entry.key().clone());
                }
            }
        }

        // Clean up disconnected receivers
        for id in to_remove {
            self.senders.remove(&id);
        }
    }

    pub fn receiver_count(&self) -> usize {
        self.senders.len()
    }
}
