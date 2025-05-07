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
use webrtc::{
    peer_connection::RTCPeerConnection,
    rtcp::{
        payload_feedbacks::receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate,
        transport_feedbacks::transport_layer_cc::TransportLayerCc,
    },
    rtp_transceiver::{
        RTCRtpTransceiverInit, rtp_transceiver_direction::RTCRtpTransceiverDirection,
    },
    track::track_local::TrackLocal,
};

use crate::{errors::WebRTCError, models::TrackMutexWrapper};

use super::{forward_track::ForwardTrack, quality::TrackQuality};

// Configurable thresholds that can be adjusted based on network conditions
const BWE_THRESHOLD_H: f32 = 1_000_000.0;
const BWE_THRESHOLD_F: f32 = 2_500_000.0;

// Network stability thresholds
const JITTER_THRESHOLD_HIGH: Duration = Duration::from_millis(100);
const DELAY_THRESHOLD_LOW: Duration = Duration::from_millis(30);
const DELAY_THRESHOLD_MEDIUM: Duration = Duration::from_millis(150);
const HIGH_JITTER_THRESHOLD_LOW: usize = 2;
const HIGH_JITTER_THRESHOLD_MEDIUM: usize = 6;

// Configurable time interval for monitoring RTCP
const RTCP_MONITOR_INTERVAL: Duration = Duration::from_secs(1);

// Minimum time between quality changes to prevent rapid fluctuations
const MIN_QUALITY_CHANGE_INTERVAL: Duration = Duration::from_secs(4);

type TrackMap = Arc<DashMap<String, Arc<ForwardTrack>>>;

#[derive(Debug)]
struct NetworkStats {
    remb_quality: TrackQuality,
    twcc_quality: TrackQuality,
    last_quality_change: Instant,
    // History for average calculations
    remb_history: VecDeque<f32>,
    delay_history: VecDeque<Duration>,
    jitter_history: VecDeque<usize>,
}

impl Default for NetworkStats {
    fn default() -> Self {
        Self {
            remb_quality: TrackQuality::High,
            twcc_quality: TrackQuality::Low,
            last_quality_change: Instant::now(),
            remb_history: VecDeque::with_capacity(15),
            delay_history: VecDeque::with_capacity(15),
            jitter_history: VecDeque::with_capacity(15),
        }
    }
}

impl NetworkStats {
    fn update_remb(&mut self, bitrate: f32) {
        // Add to history and maintain max size
        self.remb_history.push_back(bitrate);
        if self.remb_history.len() > 15 {
            self.remb_history.pop_front();
        }

        // Calculate average bitrate (more stable than single reading)
        let avg_bitrate: f32 =
            self.remb_history.iter().sum::<f32>() / self.remb_history.len() as f32;

        // Update REMB quality assessment
        self.remb_quality = if avg_bitrate >= BWE_THRESHOLD_F {
            TrackQuality::High
        } else if avg_bitrate >= BWE_THRESHOLD_H {
            TrackQuality::Medium
        } else {
            TrackQuality::Low
        };
    }

    fn update_twcc(&mut self, avg_delay: Duration, high_jitter_count: usize) {
        // Add to history and maintain max size
        self.delay_history.push_back(avg_delay);
        self.jitter_history.push_back(high_jitter_count);

        if self.delay_history.len() > 15 {
            self.delay_history.pop_front();
        }

        if self.jitter_history.len() > 15 {
            self.jitter_history.pop_front();
        }

        // Calculate averages
        let avg_delay_ms: u64 = self
            .delay_history
            .iter()
            .map(|d| d.as_millis() as u64)
            .sum::<u64>()
            / self.delay_history.len() as u64;

        let avg_jitter_count: usize =
            self.jitter_history.iter().sum::<usize>() / self.jitter_history.len();

        // Update TWCC quality assessment
        self.twcc_quality = if avg_delay_ms < DELAY_THRESHOLD_LOW.as_millis() as u64
            && avg_jitter_count <= HIGH_JITTER_THRESHOLD_LOW
        {
            TrackQuality::High
        } else if avg_delay_ms < DELAY_THRESHOLD_MEDIUM.as_millis() as u64
            && avg_jitter_count <= HIGH_JITTER_THRESHOLD_MEDIUM
        {
            TrackQuality::Medium
        } else {
            TrackQuality::Low
        };
    }

    fn calculate_combined_quality(&self) -> TrackQuality {
        // Take the more conservative quality between REMB and TWCC
        // This ensures we don't overestimate network capability
        let quality = if self.remb_quality.as_u8() < self.twcc_quality.as_u8() {
            self.remb_quality.clone()
        } else {
            self.twcc_quality.clone()
        };

        // Prevent rapid quality changes by checking if enough time has passed
        if Instant::now().duration_since(self.last_quality_change) < MIN_QUALITY_CHANGE_INTERVAL {
            return quality; // Return without updating last_quality_change
        }

        quality
    }

    fn should_update_quality(&self, current_quality: TrackQuality) -> bool {
        // Determine if a quality update is needed based on the combined assessment
        // and anti-flapping protection
        let combined_quality = self.calculate_combined_quality();

        // If qualities match, no update needed
        if combined_quality == current_quality {
            return false;
        }

        // Check if enough time has passed since last change to prevent flapping
        if Instant::now().duration_since(self.last_quality_change) < MIN_QUALITY_CHANGE_INTERVAL {
            return false;
        }

        true
    }

    fn update_quality_timestamp(&mut self) {
        self.last_quality_change = Instant::now();
    }
}

#[derive(Debug)]
pub struct Subscriber {
    pub peer_connection: Arc<RTCPeerConnection>,
    cancel_token: CancellationToken,
    preferred_quality: Arc<AtomicU8>,
    network_stats: Arc<RwLock<NetworkStats>>,
    tracks: Arc<DashMap<String, TrackMutexWrapper>>,
    track_map: TrackMap,
    user_id: String,
}

impl Subscriber {
    pub fn new(peer_connection: Arc<RTCPeerConnection>, user_id: String) -> Self {
        let cancel_token = CancellationToken::new();
        let (tx, _rx) = watch::channel(());

        let this = Self {
            peer_connection,
            cancel_token: cancel_token.clone(),
            preferred_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            network_stats: Arc::new(RwLock::new(NetworkStats::default())),
            tracks: Arc::new(DashMap::new()),
            track_map: Arc::new(DashMap::new()),
            user_id,
        };

        this.spawn_remb_loop(cancel_token, tx.clone());
        this.spawn_track_update_loop(tx);

        this
    }

    pub async fn add_track(&self, remote_track: TrackMutexWrapper) -> Result<(), WebRTCError> {
        let track_id = {
            let track_guard = remote_track.read().await;
            track_guard.id.clone()
        };

        if let Some(mut existing) = self.tracks.get_mut(&track_id) {
            *existing = remote_track;
        } else {
            self.tracks.insert(track_id.clone(), remote_track.clone());
            self.add_transceiver(remote_track, &track_id).await?;
        }

        Ok(())
    }

    async fn add_transceiver(
        &self,
        remote_track: TrackMutexWrapper,
        track_id: &str,
    ) -> Result<(), WebRTCError> {
        let peer_connection = self.peer_connection.clone();

        let forward_track = {
            let track = remote_track
                .read()
                .await
                .new_forward_track(self.user_id.clone())?;
            track
        };

        let local_track = {
            let track = forward_track.local_track.clone();
            track
        };

        let _ = peer_connection
            .add_transceiver_from_track(
                Arc::clone(&local_track) as Arc<dyn TrackLocal + Send + Sync>,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    send_encodings: vec![],
                }),
            )
            .await
            .map_err(|_| WebRTCError::FailedToAddTrack)?;

        self.track_map
            .insert(track_id.to_owned(), Arc::clone(&forward_track));

        Ok(())
    }

    fn spawn_remb_loop(&self, cancel_token: CancellationToken, tx: watch::Sender<()>) {
        let pc2 = Arc::downgrade(&self.peer_connection);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        let network_stats = Arc::clone(&self.network_stats);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    _ = tokio::time::sleep(RTCP_MONITOR_INTERVAL) => {
                        if let Some(pc) = pc2.upgrade() {
                            Self::monitor_rtcp(pc, preferred_quality.clone(), network_stats.clone(), tx.clone()).await;
                        }
                    }
                }
            }
        });
    }

    async fn monitor_rtcp(
        peer_connection: Arc<RTCPeerConnection>,
        preferred_quality: Arc<AtomicU8>,
        network_stats: Arc<RwLock<NetworkStats>>,
        tx: watch::Sender<()>,
    ) {
        let senders = peer_connection.get_senders().await;
        let mut remb_seen = false;
        let mut twcc_seen = false;

        for sender in senders {
            let result = tokio::time::timeout(Duration::from_secs(1), sender.read_rtcp()).await;
            match result {
                Ok(Ok((rtcp_packets, _attrs))) => {
                    for packet in rtcp_packets {
                        // Process REMB packets
                        if let Some(remb) = packet
                            .as_any()
                            .downcast_ref::<ReceiverEstimatedMaximumBitrate>()
                        {
                            remb_seen = true;
                            let bitrate = remb.bitrate;

                            // Update REMB stats
                            let mut stats = network_stats.write().await;
                            stats.update_remb(bitrate);

                            debug!(
                                "REMB bitrate: {}, quality assessment: {:?}",
                                bitrate, stats.remb_quality
                            );
                        }

                        // Process TWCC packets
                        if let Some(tcc) = packet.as_any().downcast_ref::<TransportLayerCc>() {
                            twcc_seen = true;
                            let deltas = &tcc.recv_deltas;

                            let mut high_jitter_count = 0;
                            let mut total_delay = Duration::ZERO;
                            let mut valid_deltas = 0;

                            for delta in deltas {
                                if delta.delta < 0 {
                                    continue; // skip negative deltas
                                }

                                let delta_duration = Duration::from_micros(delta.delta as u64);
                                total_delay += delta_duration;
                                valid_deltas += 1;

                                if delta_duration > JITTER_THRESHOLD_HIGH {
                                    high_jitter_count += 1;
                                }
                            }

                            let avg_delay = if valid_deltas > 0 {
                                total_delay / valid_deltas as u32
                            } else {
                                Duration::ZERO
                            };

                            // Update TWCC stats
                            let mut stats = network_stats.write().await;
                            stats.update_twcc(avg_delay, high_jitter_count);

                            debug!(
                                "TWCC avg delay: {:?}, high jitter packets: {}, quality assessment: {:?}",
                                avg_delay, high_jitter_count, stats.twcc_quality
                            );
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("Error reading RTCP: {:?}", e);
                }
                Err(e) => {
                    debug!("Timeout reading RTCP: {:?}", e);
                }
            }
        }

        // After processing all packets, determine if we need to update quality
        if remb_seen || twcc_seen {
            // Check if we should update quality
            let current_quality = TrackQuality::from_u8(preferred_quality.load(Ordering::Relaxed));
            let mut stats = network_stats.write().await;

            if stats.should_update_quality(current_quality.clone()) {
                let new_quality = stats.calculate_combined_quality();

                info!(
                    "Updating quality from {:?} to {:?} (REMB: {:?}, TWCC: {:?})",
                    current_quality, new_quality, stats.remb_quality, stats.twcc_quality
                );

                preferred_quality.store(new_quality.as_u8(), Ordering::Relaxed);
                stats.update_quality_timestamp();

                // Notify tracks about quality change
                let _ = tx.send(());
            }
        }
    }

    fn spawn_track_update_loop(&self, tx: watch::Sender<()>) {
        let tracks = Arc::clone(&self.tracks);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        let track_map = Arc::clone(&self.track_map);
        let mut rx = tx.subscribe();

        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let preferred = TrackQuality::from_u8(preferred_quality.load(Ordering::SeqCst));

                for entry in tracks.iter() {
                    let track_id = entry.key();

                    if let Some(ctx) = track_map.get_mut(track_id) {
                        ctx.set_effective_quality(&preferred);
                        debug!("Setting track {} quality to {:?}", track_id, preferred);
                    }
                }
            }
        });
    }

    // Allows external components to adjust the quality thresholds based on external factors
    pub fn adjust_quality_thresholds(
        &self,
        min_change_interval: Option<Duration>,
        force_quality: Option<TrackQuality>,
    ) {
        tokio::spawn({
            let network_stats = self.network_stats.clone();
            let preferred_quality = self.preferred_quality.clone();
            let tx = watch::channel(()).0;

            async move {
                let mut stats = network_stats.write().await;

                // Update minimum change interval if provided
                if let Some(interval) = min_change_interval {
                    // Update logic for dynamically adjusting thresholds would go here
                    debug!(
                        "Adjusting minimum quality change interval to {:?}",
                        interval
                    );
                }

                // Force a specific quality if requested
                if let Some(quality) = force_quality {
                    debug!("Forcing quality to {:?}", quality);
                    preferred_quality.store(quality.as_u8(), Ordering::Relaxed);
                    stats.update_quality_timestamp();
                    let _ = tx.send(());
                }
            }
        });
    }

    // Get current network stats for monitoring/debugging
    pub async fn get_network_stats(&self) -> (TrackQuality, TrackQuality, TrackQuality) {
        let stats = self.network_stats.read().await;
        let current = TrackQuality::from_u8(self.preferred_quality.load(Ordering::Relaxed));

        (
            current,
            stats.remb_quality.clone(),
            stats.twcc_quality.clone(),
        )
    }

    pub fn close(&self) {
        self.cancel_token.cancel();

        self.clear_all_forward_tracks();

        let pc = Arc::clone(&self.peer_connection);

        tokio::spawn(async move {
            let _ = pc.close().await;
        });
    }

    pub fn clear_all_forward_tracks(&self) {
        let user_id = self.user_id.clone();

        for track in self.tracks.iter() {
            let user_id = user_id.clone();
            let track = track.clone();
            tokio::spawn(async move {
                let writer = track.write().await;

                writer.remove_forward_track(&user_id);
            });
        }

        self.tracks.clear();
        self.track_map.clear();
    }
}
