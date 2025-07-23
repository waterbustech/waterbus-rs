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
    data_channel::{RTCDataChannel, data_channel_message::DataChannelMessage},
    peer_connection::RTCPeerConnection,
    rtcp::{
        payload_feedbacks::{
            full_intra_request::{FirEntry, FullIntraRequest},
            picture_loss_indication::PictureLossIndication,
        },
        transport_feedbacks::transport_layer_cc::TransportLayerCc,
    },
    rtp_transceiver::{
        RTCRtpTransceiverInit, rtp_transceiver_direction::RTCRtpTransceiverDirection,
    },
    track::track_local::TrackLocal,
};

use crate::{
    errors::WebRTCError,
    models::{
        params::TrackMutexWrapper, quality::TrackQuality,
        track_quality_request::TrackQualityRequest,
    },
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
// const KEYFRAME_REQUEST_INTERVAL: Duration = Duration::from_secs(1);

// History sizes for better stability
const HISTORY_SIZE: usize = 10;

type TrackMap = Arc<DashMap<String, Arc<ForwardTrack>>>;

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

        // Calculate moving averages for smoother quality transitions
        let avg_delay_ms = if !self.delay_history.is_empty() {
            self.delay_history
                .iter()
                .map(|d| d.as_millis() as u64)
                .sum::<u64>()
                / self.delay_history.len() as u64
        } else {
            0
        };

        let avg_jitter_count = if !self.jitter_history.is_empty() {
            self.jitter_history.iter().sum::<usize>() / self.jitter_history.len()
        } else {
            0
        };

        // More aggressive quality degradation on packet loss
        let loss_penalty = if self.packet_loss_count > 0
            && Instant::now().duration_since(self.last_quality_change) > Duration::from_secs(5)
        {
            1 // Degrade quality by one level
        } else {
            0
        };

        // Update TWCC quality assessment with packet loss consideration
        self.twcc_quality = if avg_delay_ms < DELAY_THRESHOLD_LOW.as_millis() as u64
            && avg_jitter_count <= HIGH_JITTER_THRESHOLD_LOW
            && loss_penalty == 0
        {
            TrackQuality::High
        } else if avg_delay_ms < DELAY_THRESHOLD_MEDIUM.as_millis() as u64
            && avg_jitter_count <= HIGH_JITTER_THRESHOLD_MEDIUM
            && loss_penalty <= 1
        {
            TrackQuality::Medium
        } else {
            TrackQuality::Low
        };

        // Reset packet loss counter periodically
        if Instant::now().duration_since(self.last_quality_change) > Duration::from_secs(10) {
            self.packet_loss_count = 0;
        }
    }

    fn should_update_quality(&self, current_quality: TrackQuality) -> bool {
        // More responsive quality changes
        if self.twcc_quality == current_quality {
            return false;
        }

        // Shorter interval for quality improvements, longer for degradation
        let min_interval = if self.twcc_quality.as_u8() > current_quality.as_u8() {
            Duration::from_millis(1500) // Faster upgrade
        } else {
            MIN_QUALITY_CHANGE_INTERVAL // Normal downgrade
        };

        Instant::now().duration_since(self.last_quality_change) >= min_interval
    }

    fn update_quality_timestamp(&mut self) {
        self.last_quality_change = Instant::now();
    }
}

pub struct Subscriber {
    pub peer_connection: Arc<RTCPeerConnection>,
    cancel_token: CancellationToken,
    preferred_quality: Arc<AtomicU8>,
    network_stats: Arc<RwLock<NetworkStats>>,
    tracks: Arc<DashMap<String, TrackMutexWrapper>>,
    track_map: TrackMap,
    user_id: String,
    data_channel: Option<Arc<RTCDataChannel>>,
    client_requested_quality: Arc<RwLock<Option<TrackQuality>>>,
}

impl Subscriber {
    pub async fn new(peer_connection: Arc<RTCPeerConnection>, user_id: String) -> Self {
        let cancel_token = CancellationToken::new();
        let (tx, _rx) = watch::channel(());

        let mut this = Self {
            peer_connection,
            cancel_token: cancel_token.clone(),
            preferred_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            network_stats: Arc::new(RwLock::new(NetworkStats::default())),
            tracks: Arc::new(DashMap::new()),
            track_map: Arc::new(DashMap::new()),
            user_id,
            data_channel: None,
            client_requested_quality: Arc::new(RwLock::new(None)),
        };

        this.spawn_rtcp_monitor(cancel_token, tx.clone());
        this.spawn_track_update_loop(tx);

        let _ = this.create_data_channel().await;

        this
    }

    pub async fn create_data_channel(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let data_channel = self
            .peer_connection
            .create_data_channel("track_quality", None)
            .await?;

        // Clone the necessary fields before moving into the closure
        let client_requested_quality = Arc::clone(&self.client_requested_quality);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        let user_id = self.user_id.clone();
        let track_map = Arc::clone(&self.track_map);

        self.peer_connection
            .on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
                let label = dc.label();
                info!("New data channel from client: {}", label);

                if label == "track_quality" {
                    let client_quality = Arc::clone(&client_requested_quality);
                    let preferred = Arc::clone(&preferred_quality);
                    let uid = user_id.clone();
                    let tracks = Arc::clone(&track_map);

                    tokio::spawn(async move {
                        // Handle the data channel setup here instead of calling listen_data_channel
                        let user_id_on_msg = uid.clone();

                        dc.on_message(Box::new(move |msg: DataChannelMessage| {
                            let client_quality = Arc::clone(&client_quality);
                            let preferred = Arc::clone(&preferred);
                            let uid = user_id_on_msg.clone();
                            let tracks = Arc::clone(&tracks);

                            Box::pin(async move {
                                let data = msg.data;

                                let parsed = str::from_utf8(&data)
                                    .map_err(|e| format!("Invalid UTF-8: {e}"))
                                    .and_then(|json_str| {
                                        serde_json::from_str::<TrackQualityRequest>(json_str)
                                            .map_err(|e| format!("JSON parse error: {e}"))
                                    });

                                match parsed {
                                    Ok(request) => {
                                        info!(
                                            "Received quality request from client {}: {:?}",
                                            uid, request
                                        );

                                        let quality = request.quality.clone();

                                        {
                                            let mut client_quality_guard =
                                                client_quality.write().await;
                                            *client_quality_guard = Some(quality.clone());
                                        }

                                        preferred.store(quality.as_u8(), Ordering::Relaxed);

                                        if let Some(track) = tracks.get(&request.track_id) {
                                            track.set_effective_quality(&request.quality);
                                            info!(
                                                "Applied requested quality {:?} to track {}",
                                                request.quality, request.track_id
                                            );
                                        } else {
                                            warn!(
                                                "Received quality request for unknown track_id: {}",
                                                request.track_id
                                            );
                                        }
                                    }
                                    Err(err) => {
                                        warn!(
                                            "Failed to parse data channel message from {}: {}",
                                            uid, err
                                        );
                                    }
                                }
                            })
                        }));

                        let user_id_open = uid.clone();
                        dc.on_open(Box::new(move || {
                            info!(
                                "Quality control data channel opened for user: {}",
                                user_id_open
                            );
                            Box::pin(async {})
                        }));

                        let user_id_close = uid.clone();
                        dc.on_close(Box::new(move || {
                            info!(
                                "Quality control data channel closed for user: {}",
                                user_id_close
                            );
                            Box::pin(async {})
                        }));
                    });
                }

                Box::pin(async {})
            }));

        self.data_channel = Some(data_channel);

        Ok(())
    }

    pub async fn add_track(&self, remote_track: TrackMutexWrapper) -> Result<(), WebRTCError> {
        let track_id = {
            let track_guard = remote_track.read();
            track_guard.id.clone()
        };

        if let Some(mut existing) = self.tracks.get_mut(&track_id) {
            *existing = remote_track;
        } else {
            self.tracks.insert(track_id.clone(), remote_track.clone());
            self.add_transceiver(remote_track, &track_id).await?;
        }

        // Request keyframe immediately for new tracks to reduce initial delay
        self.request_keyframe().await;

        Ok(())
    }

    async fn add_transceiver(
        &self,
        remote_track: TrackMutexWrapper,
        track_id: &str,
    ) -> Result<(), WebRTCError> {
        let peer_connection = self.peer_connection.clone();

        let forward_track = {
            let track_guard = remote_track.read();
            let ssrc = track_guard.ssrc;
            track_guard.new_forward_track(&self.user_id, ssrc)?
        };

        let local_track = { forward_track.local_track.clone() };

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

    fn spawn_rtcp_monitor(&self, cancel_token: CancellationToken, tx: watch::Sender<()>) {
        let pc = Arc::downgrade(&self.peer_connection);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        let network_stats = Arc::clone(&self.network_stats);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(RTCP_MONITOR_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    _ = interval.tick() => {
                        if let Some(pc_strong) = pc.upgrade() {
                            Self::monitor_rtcp(pc_strong, preferred_quality.clone(), network_stats.clone(), tx.clone()).await;
                        } else {
                            break; // PeerConnection was dropped
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
        let mut twcc_processed = false;

        for sender in senders {
            let result = tokio::time::timeout(Duration::from_millis(200), sender.read_rtcp()).await;
            match result {
                Ok(Ok((rtcp_packets, _attrs))) => {
                    for packet in rtcp_packets {
                        // Process TWCC packets only
                        if let Some(tcc) = packet.as_any().downcast_ref::<TransportLayerCc>() {
                            twcc_processed = true;
                            Self::process_twcc_feedback(tcc, &network_stats).await;
                        }
                    }
                }
                Ok(Err(e)) => {
                    debug!("Error reading RTCP: {:?}", e);
                }
                Err(_) => {
                    // Timeout is normal, continue
                }
            }
        }

        // Update quality based on TWCC analysis
        if twcc_processed {
            let current_quality = TrackQuality::from_u8(preferred_quality.load(Ordering::Relaxed));
            let mut stats = network_stats.write().await;

            if stats.should_update_quality(current_quality.clone()) {
                let new_quality = stats.twcc_quality.clone();

                info!(
                    "Quality change: {:?} -> {:?} (TWCC analysis)",
                    current_quality, new_quality
                );

                preferred_quality.store(new_quality.as_u8(), Ordering::Relaxed);
                stats.update_quality_timestamp();

                // Notify tracks about quality change
                let _ = tx.send(());
            }
        }
    }

    async fn process_twcc_feedback(
        tcc: &TransportLayerCc,
        network_stats: &Arc<RwLock<NetworkStats>>,
    ) {
        let deltas = &tcc.recv_deltas;
        let mut high_jitter_count = 0;
        let mut total_delay = Duration::ZERO;
        let mut valid_deltas = 0;
        let mut packet_loss = false;

        for delta in deltas {
            if delta.delta < 0 {
                packet_loss = true;
                continue;
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

        // Update network stats
        let mut stats = network_stats.write().await;
        stats.update_twcc(avg_delay, high_jitter_count, packet_loss);
    }

    fn spawn_track_update_loop(&self, tx: watch::Sender<()>) {
        // let tracks = Arc::clone(&self.tracks);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        let track_map = Arc::clone(&self.track_map);
        let mut rx = tx.subscribe();

        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let preferred = TrackQuality::from_u8(preferred_quality.load(Ordering::Relaxed));

                // Batch update all tracks for better performance
                let update_tasks: Vec<_> = track_map
                    .iter()
                    .map(|entry| {
                        let _track_id = entry.key().clone();
                        let forward_track = entry.value().clone();
                        let quality = preferred.clone();

                        tokio::spawn(async move {
                            forward_track.set_effective_quality(&quality);
                            // debug!("Track {} quality -> {:?}", track_id, quality);
                        })
                    })
                    .collect();

                // Wait for all updates to complete
                for task in update_tasks {
                    let _ = task.await;
                }
            }
        });
    }

    // Helper method to request keyframes - crucial for P2P to SFU migration
    async fn request_keyframe(&self) {
        // Get all transceivers to find SSRCs
        let transceivers = self.peer_connection.get_transceivers().await;

        for transceiver in &transceivers {
            let sender = transceiver.sender().await;
            // Try to get SSRC from sender parameters
            let params = sender.get_parameters().await;

            for encoding in params.encodings {
                let ssrc = encoding.ssrc;

                // Send PLI (Picture Loss Indication) to request keyframe
                let pli = PictureLossIndication {
                    sender_ssrc: 0, // Will be filled by WebRTC stack
                    media_ssrc: ssrc,
                };

                let rtcp_packets: Vec<Box<dyn webrtc::rtcp::packet::Packet + Send + Sync>> =
                    vec![Box::new(pli)];

                match self.peer_connection.write_rtcp(&rtcp_packets).await {
                    Ok(_) => {
                        debug!("PLI keyframe request sent for SSRC: {}", ssrc);
                    }
                    Err(e) => {
                        debug!("Failed to send PLI for SSRC {}: {:?}", ssrc, e);
                    }
                }

                // Also try FIR as backup
                let fir = FullIntraRequest {
                    sender_ssrc: 0,
                    media_ssrc: ssrc,
                    fir: vec![FirEntry {
                        ssrc,
                        sequence_number: 1,
                    }],
                };

                let fir_packets: Vec<Box<dyn webrtc::rtcp::packet::Packet + Send + Sync>> =
                    vec![Box::new(fir)];

                if (self.peer_connection.write_rtcp(&fir_packets).await).is_ok() {
                    debug!("FIR keyframe request sent for SSRC: {}", ssrc);
                }
            }
        }

        // Fallback: Send generic PLI without specific SSRC
        if transceivers.is_empty() {
            let generic_pli = PictureLossIndication {
                sender_ssrc: 0,
                media_ssrc: 0, // Generic request
            };

            let rtcp_packets: Vec<Box<dyn webrtc::rtcp::packet::Packet + Send + Sync>> =
                vec![Box::new(generic_pli)];

            if (self.peer_connection.write_rtcp(&rtcp_packets).await).is_ok() {
                debug!("Generic PLI keyframe request sent");
            }
        }
    }

    // Force quality for migration scenarios
    pub async fn force_quality_for_migration(&self, quality: TrackQuality) {
        self.preferred_quality
            .store(quality.as_u8(), Ordering::Relaxed);

        let mut stats = self.network_stats.write().await;
        stats.update_quality_timestamp();

        // Immediately update all tracks
        for entry in self.track_map.iter() {
            entry.value().set_effective_quality(&quality);
        }

        // Request keyframe to ensure smooth transition
        drop(stats); // Release lock before async call
        self.request_keyframe().await;

        info!("Forced quality to {:?} for migration", quality);
    }

    // Get current network stats for monitoring
    pub async fn get_network_stats(&self) -> (TrackQuality, TrackQuality) {
        let stats = self.network_stats.read().await;
        let current = TrackQuality::from_u8(self.preferred_quality.load(Ordering::Relaxed));

        (current, stats.twcc_quality.clone())
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

        // Batch clear for better performance
        let clear_tasks: Vec<_> = self
            .tracks
            .iter()
            .map(|track| {
                let user_id = user_id.clone();
                let track = track.clone();
                tokio::spawn(async move {
                    let writer = track.read();
                    writer.remove_forward_track(&user_id);
                })
            })
            .collect();

        // Don't wait for tasks to complete, let them run in background
        self.tracks.clear();
        self.track_map.clear();

        tokio::spawn(async move {
            for task in clear_tasks {
                let _ = task.await;
            }
        });
    }
}
