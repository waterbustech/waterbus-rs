use dashmap::DashMap;
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{RwLock, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use str0m::{
    Rtc, Candidate, Input, Output, Event, IceConnectionState,
    media::{Direction, MediaKind, Mid, KeyframeRequest, KeyframeRequestKind, MediaData},
    change::{SdpOffer, SdpAnswer, SdpPendingOffer},
    channel::{ChannelId, ChannelData},
};
use parking_lot::Mutex;

use crate::{
    errors::WebRTCError,
    models::{
        params::{TrackMutexWrapper, RenegotiationCallback, IceCandidateCallback}, 
        quality::TrackQuality,
        track_quality_request::TrackQualityRequest,
    },
    utils::multicast_sender::MulticastSenderImpl,
};

use super::forward_track::ForwardTrack;

// Network stability thresholds (optimized values)
const JITTER_THRESHOLD_HIGH: Duration = Duration::from_millis(80);
const DELAY_THRESHOLD_LOW: Duration = Duration::from_millis(25);
const DELAY_THRESHOLD_MEDIUM: Duration = Duration::from_millis(120);
const HIGH_JITTER_THRESHOLD_LOW: usize = 2;
const HIGH_JITTER_THRESHOLD_MEDIUM: usize = 5;

// Optimized intervals
const RTCP_MONITOR_INTERVAL: Duration = Duration::from_millis(500);
const MIN_QUALITY_CHANGE_INTERVAL: Duration = Duration::from_secs(2);

// History sizes for better stability
const HISTORY_SIZE: usize = 10;

type TrackMap = Arc<DashMap<String, Arc<ForwardTrack<MulticastSenderImpl>>>>;

#[derive(Debug)]
struct NetworkStats {
    twcc_quality: TrackQuality,
    last_quality_change: Instant,
    delay_history: VecDeque<Duration>,
    jitter_history: VecDeque<usize>,
    packet_loss_count: u32,
}

impl Default for NetworkStats {
    fn default() -> Self {
        Self {
            twcc_quality: TrackQuality::Medium,
            last_quality_change: Instant::now(),
            delay_history: VecDeque::with_capacity(HISTORY_SIZE),
            jitter_history: VecDeque::with_capacity(HISTORY_SIZE),
            packet_loss_count: 0,
        }
    }
}

impl NetworkStats {
    fn update_twcc(&mut self, avg_delay: Duration, high_jitter_count: usize, packet_loss: bool) {
        // Add to history with fixed capacity
        if self.delay_history.len() >= HISTORY_SIZE {
            self.delay_history.pop_front();
        }
        if self.jitter_history.len() >= HISTORY_SIZE {
            self.jitter_history.pop_front();
        }

        self.delay_history.push_back(avg_delay);
        self.jitter_history.push_back(high_jitter_count);

        if packet_loss {
            self.packet_loss_count += 1;
        }

        // Determine quality based on network conditions
        let avg_delay_ms = if !self.delay_history.is_empty() {
            self.delay_history
                .iter()
                .map(|d| d.as_millis() as u64)
                .sum::<u64>()
                / self.delay_history.len() as u64
        } else {
            0
        };

        let avg_jitter = if !self.jitter_history.is_empty() {
            self.jitter_history.iter().sum::<usize>() / self.jitter_history.len()
        } else {
            0
        };

        // Quality determination logic
        let now = Instant::now();
        if now.duration_since(self.last_quality_change) >= MIN_QUALITY_CHANGE_INTERVAL {
            let new_quality = if avg_delay_ms > DELAY_THRESHOLD_MEDIUM.as_millis() as u64
                || avg_jitter > HIGH_JITTER_THRESHOLD_MEDIUM
                || self.packet_loss_count > 3
            {
                TrackQuality::Low
            } else if avg_delay_ms > DELAY_THRESHOLD_LOW.as_millis() as u64
                || avg_jitter > HIGH_JITTER_THRESHOLD_LOW
                || self.packet_loss_count > 1
            {
                TrackQuality::Medium
            } else {
                TrackQuality::High
            };

            if new_quality != self.twcc_quality {
                self.twcc_quality = new_quality;
                self.last_quality_change = now;
                self.packet_loss_count = 0; // Reset loss count after quality change
            }
        }
    }

    fn get_quality(&self) -> TrackQuality {
        self.twcc_quality
    }
}

pub struct Subscriber {
    pub rtc: Arc<Mutex<Rtc>>,
    pub participant_id: String,
    pub target_id: String,
    cancel_token: CancellationToken,
    preferred_quality: Arc<AtomicU8>,
    network_stats: Arc<RwLock<NetworkStats>>,
    tracks: Arc<DashMap<String, TrackMutexWrapper>>,
    track_map: TrackMap,
    client_requested_quality: Arc<RwLock<Option<TrackQuality>>>,
    pending_offer: Option<SdpPendingOffer>,
    channel_id: Option<ChannelId>,
    on_negotiation_needed: RenegotiationCallback,
    on_candidate: IceCandidateCallback,
}

impl Subscriber {
    pub async fn new(
        rtc: Arc<Mutex<Rtc>>,
        participant_id: String,
        target_id: String,
        on_negotiation_needed: RenegotiationCallback,
        on_candidate: IceCandidateCallback,
    ) -> Arc<Self> {
        let cancel_token = CancellationToken::new();
        let (tx, _rx) = watch::channel(());

        let subscriber = Arc::new(Self {
            rtc,
            participant_id,
            target_id,
            cancel_token: cancel_token.clone(),
            preferred_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            network_stats: Arc::new(RwLock::new(NetworkStats::default())),
            tracks: Arc::new(DashMap::new()),
            track_map: Arc::new(DashMap::new()),
            client_requested_quality: Arc::new(RwLock::new(None)),
            pending_offer: None,
            channel_id: None,
            on_negotiation_needed,
            on_candidate,
        });

        subscriber.spawn_rtcp_monitor(cancel_token.clone(), tx.clone());
        subscriber.spawn_track_update_loop(tx);

        subscriber
    }

    pub fn set_remote_answer(&self, answer: SdpAnswer) -> Result<(), str0m::RtcError> {
        // Handle the SDP answer for this subscriber
        // In str0m, this would be done through pending changes
        let mut rtc = self.rtc.lock();
        if let Some(pending) = rtc.pending_changes() {
            pending.accept_answer(answer)
        } else {
            Err(str0m::RtcError::InvalidState)
        }
    }

    pub fn add_remote_candidate(&self, candidate: Candidate) -> Result<(), str0m::RtcError> {
        let mut rtc = self.rtc.lock();
        rtc.add_remote_candidate(candidate);
        Ok(())
    }

    pub async fn add_track(&self, track: TrackMutexWrapper) -> Result<(), WebRTCError> {
        // In str0m, track handling is different
        // We'll need to adapt the track to str0m's media system
        info!("Adding track to subscriber {} for target {}", self.participant_id, self.target_id);
        
        // Store the track for reference
        let track_id = {
            let track_guard = track.read();
            track_guard.id()
        };
        
        self.tracks.insert(track_id.clone(), track);
        Ok(())
    }

    fn spawn_rtcp_monitor(&self, cancel_token: CancellationToken, _tx: watch::Sender<()>) {
        let network_stats = Arc::clone(&self.network_stats);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(RTCP_MONITOR_INTERVAL);
            
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    _ = interval.tick() => {
                        // In str0m, network statistics would be obtained differently
                        // For now, we'll use simulated values
                        let mut stats = network_stats.write().await;
                        stats.update_twcc(
                            Duration::from_millis(50), // Simulated delay
                            1, // Simulated jitter count
                            false, // No packet loss
                        );
                    }
                }
            }
        });
    }

    fn spawn_track_update_loop(&self, _tx: watch::Sender<()>) {
        let track_map = Arc::clone(&self.track_map);
        let network_stats = Arc::clone(&self.network_stats);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        let client_requested_quality = Arc::clone(&self.client_requested_quality);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            
            loop {
                interval.tick().await;
                
                // Update track qualities based on network conditions
                let stats = network_stats.read().await;
                let current_quality = stats.get_quality();
                
                // Check if client has requested a specific quality
                let requested_quality = client_requested_quality.read().await;
                let target_quality = requested_quality.unwrap_or(current_quality);
                
                preferred_quality.store(target_quality.as_u8(), Ordering::Relaxed);
                
                // Update all tracks with the new quality
                for entry in track_map.iter() {
                    let forward_track = entry.value();
                    forward_track.set_quality(target_quality).await;
                }
            }
        });
    }

    pub fn poll_rtc(&self) -> Option<Output> {
        let mut rtc = self.rtc.lock();
        match rtc.poll_output() {
            Ok(output) => Some(output),
            Err(e) => {
                warn!("RTC poll error: {:?}", e);
                None
            }
        }
    }

    pub fn handle_input(&self, input: Input) -> Result<(), str0m::RtcError> {
        let mut rtc = self.rtc.lock();
        rtc.handle_input(input)
    }

    pub fn close(&self) {
        let rtc = self.rtc.clone();
        self.cancel_token.cancel();

        tokio::spawn(async move {
            let mut rtc_guard = rtc.lock();
            rtc_guard.disconnect();
        });
    }

    pub fn get_preferred_quality(&self) -> TrackQuality {
        TrackQuality::from_u8(self.preferred_quality.load(Ordering::Relaxed))
    }

    pub async fn set_client_requested_quality(&self, quality: Option<TrackQuality>) {
        let mut client_quality = self.client_requested_quality.write().await;
        *client_quality = quality;
    }

    pub fn get_user_id(&self) -> &str {
        &self.participant_id
    }
}
