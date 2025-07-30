use std::sync::Arc;
use dashmap::DashMap;
use parking_lot::RwLock;
use str0m::{media::Mid, Rtc};

#[derive(Debug, Clone)]
pub enum TrackOutState {
    ToOpen,
    Negotiating(Mid),
    Open(Mid),
}

#[derive(Debug)]
pub struct Subscriber {
    pub participant_id: String,
    pub rtc: Arc<RwLock<Rtc>>,
    pub tracks_out: Arc<DashMap<Mid, TrackOutState>>,
}

impl Subscriber {
    pub fn new(participant_id: String, rtc: Arc<RwLock<Rtc>>) -> Self {
        Self {
            participant_id,
            rtc,
            tracks_out: Arc::new(DashMap::new()),
        }
    }

    pub fn add_track_out(&mut self, mid: Mid, state: TrackOutState) {
        self.tracks_out.insert(mid, state);
    }

    pub fn remove_track_out(&mut self, mid: Mid) {
        self.tracks_out.remove(&mid);
    }

    pub fn close(&self) {
        // Clean up subscriber resources
        self.tracks_out.clear();
    }
} 