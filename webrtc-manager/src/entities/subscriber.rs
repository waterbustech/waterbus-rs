use crate::{errors::WebRTCError, models::TrackMutexWrapper};
use dashmap::DashMap;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tracing::warn;
use webrtc::{
    peer_connection::RTCPeerConnection,
    rtcp::payload_feedbacks::receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate,
    rtp_transceiver::{
        RTCRtpTransceiver, RTCRtpTransceiverInit,
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
    },
    track::track_local::{TrackLocal, track_local_static_rtp::TrackLocalStaticRTP},
};

const BWE_THRESHOLD_H: f32 = 550_000.0;
const BWE_THRESHOLD_F: f32 = 1_600_000.0;

#[derive(Debug)]
struct TrackContext {
    transceiver: Arc<RTCRtpTransceiver>,
    local_track: Arc<TrackLocalStaticRTP>,
}

type TrackMap = Arc<Mutex<HashMap<String, TrackContext>>>;

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum PreferredQuality {
    Low = 0,
    Medium = 1,
    High = 2,
}

impl PreferredQuality {
    pub fn from_u8(value: u8) -> PreferredQuality {
        match value {
            0 => PreferredQuality::Low,
            2 => PreferredQuality::High,
            _ => PreferredQuality::Medium,
        }
    }

    pub fn as_u8(&self) -> u8 {
        self.clone() as u8
    }
}

#[derive(Debug)]
pub struct Subscriber {
    pub peer_connection: Arc<RTCPeerConnection>,
    cancel_token: CancellationToken,
    preferred_quality: Arc<AtomicU8>,
    tracks: Arc<DashMap<String, TrackMutexWrapper>>,
    track_map: TrackMap,
    quality_change_counter: Arc<DashMap<PreferredQuality, u32>>,
}

impl Subscriber {
    pub fn new(peer_connection: Arc<RTCPeerConnection>) -> Self {
        let cancel_token = CancellationToken::new();
        let (tx, _rx) = watch::channel(());

        let this = Self {
            peer_connection,
            cancel_token: cancel_token.clone(),
            preferred_quality: Arc::new(AtomicU8::new(PreferredQuality::Medium.as_u8())),
            tracks: Arc::new(DashMap::new()),
            track_map: Arc::new(Mutex::new(HashMap::new())),
            quality_change_counter: Arc::new(DashMap::new()),
        };

        this._spawn_remb_loop(cancel_token, tx.clone());
        this._spawn_track_update_loop(tx);

        this
    }

    pub async fn add_track(&self, remote_track: TrackMutexWrapper) -> Result<(), WebRTCError> {
        let track_id = {
            let track_guard = remote_track.lock().await;
            track_guard.id.clone()
        };

        if let Some(mut existing) = self.tracks.get_mut(&track_id) {
            *existing = remote_track;
        } else {
            self.tracks
                .insert(track_id.to_string(), remote_track.clone());
            self._add_transceiver(remote_track, &track_id).await?;
        }

        Ok(())
    }

    async fn _add_transceiver(
        &self,
        remote_track: TrackMutexWrapper,
        track_id: &str,
    ) -> Result<(), WebRTCError> {
        let peer_connection = self.peer_connection.clone();

        let track_selected = self.get_appropriate_track(&remote_track).await?;

        let transceiver = peer_connection
            .add_transceiver_from_track(
                Arc::clone(&track_selected) as Arc<dyn TrackLocal + Send + Sync>,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    send_encodings: vec![],
                }),
            )
            .await
            .map_err(|_| WebRTCError::FailedToAddTrack)?;

        self._read_rtcp(&transceiver).await;

        self.track_map.lock().await.insert(
            track_id.to_owned(),
            TrackContext {
                transceiver: Arc::clone(&transceiver),
                local_track: Arc::clone(&track_selected),
            },
        );

        Ok(())
    }

    async fn _read_rtcp(&self, transceiver: &Arc<RTCRtpTransceiver>) {
        let rtp_sender = transceiver.sender().await;
        let cancel_token = self.cancel_token.clone();

        // Read incoming RTCP packets
        // Before these packets are returned they are processed by interceptors. For things
        // like NACK this needs to be called.
        tokio::spawn(async move {
            let mut rtcp_buf = vec![0u8; 1500];
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    result = rtp_sender.read(&mut rtcp_buf) => {
                        if result.is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }

    fn _spawn_remb_loop(&self, cancel_token: CancellationToken, tx: watch::Sender<()>) {
        let pc2 = Arc::downgrade(&self.peer_connection);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        let quality_change_counter = Arc::clone(&self.quality_change_counter);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        // info!("[RTCP] REMB loop cancelled");
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(4)) => {
                        if let Some(pc) = pc2.upgrade() {
                            // Passing the cloned `subscriber` instead of `self`
                            Self::_monitor_remb(quality_change_counter.clone(), pc, preferred_quality.clone(), tx.clone()).await;
                        }
                    }
                }
            }
        });
    }

    async fn _monitor_remb(
        quality_change_counter: Arc<DashMap<PreferredQuality, u32>>,
        peer_connection: Arc<RTCPeerConnection>,
        preferred_quality: Arc<AtomicU8>,
        _tx: watch::Sender<()>,
    ) {
        let senders = peer_connection.get_senders().await;
        for sender in senders {
            let result = tokio::time::timeout(Duration::from_secs(3), sender.read_rtcp()).await;
            match result {
                Ok(Ok((rtcp_packets, _attrs))) => {
                    for packet in rtcp_packets {
                        if let Some(remb) = packet
                            .as_any()
                            .downcast_ref::<ReceiverEstimatedMaximumBitrate>()
                        {
                            let bitrate = remb.bitrate;
                            let new_quality = if bitrate >= BWE_THRESHOLD_F {
                                PreferredQuality::High
                            } else if bitrate >= BWE_THRESHOLD_H {
                                PreferredQuality::Medium
                            } else {
                                PreferredQuality::Low
                            };

                            let current_u8 =
                                preferred_quality.load(std::sync::atomic::Ordering::Relaxed);
                            let current_quality = PreferredQuality::from_u8(current_u8);

                            if std::mem::discriminant(&current_quality)
                                != std::mem::discriminant(&new_quality)
                            {
                                let mut counter = quality_change_counter
                                    .entry(new_quality.clone())
                                    .or_insert(0);
                                *counter += 1;

                                if *counter >= 3 {
                                    preferred_quality.store(
                                        new_quality.as_u8(),
                                        std::sync::atomic::Ordering::Relaxed,
                                    );
                                    *counter = 0;
                                    // let _ = tx.send(());
                                }
                            } else {
                                if let Some(mut counter) =
                                    quality_change_counter.get_mut(&new_quality)
                                {
                                    *counter = 0;
                                }
                            }
                        }
                    }
                }
                Ok(Err(_)) => {}
                Err(_) => {}
            }
        }
    }

    fn _spawn_track_update_loop(&self, tx: watch::Sender<()>) {
        let tracks = Arc::clone(&self.tracks);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        let track_map = Arc::clone(&self.track_map);
        // let peer_connection = Arc::clone(&self.peer_connection);
        let mut rx = tx.subscribe();

        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let preferred = PreferredQuality::from_u8(preferred_quality.load(Ordering::SeqCst));
                let mut track_map = track_map.lock().await;

                for entry in tracks.iter() {
                    let track_id = entry.key();
                    let track = entry.value();

                    let track_guard = track.lock().await;

                    if track_guard.is_simulcast {
                        if let Some(new_encoding) =
                            track_guard.get_track_appropriate(&preferred).await
                        {
                            if let Some(ctx) = track_map.get_mut(track_id) {
                                let sender = ctx.transceiver.sender().await;
                                let cur_track = sender.track().await;

                                match cur_track {
                                    Some(cur_track) => {
                                        if cur_track.rid() != new_encoding.rid() {
                                            let result = sender
                                                .replace_track(Some(Arc::clone(&new_encoding)
                                                    as Arc<dyn TrackLocal + Send + Sync>))
                                                .await;

                                            match result {
                                                Ok(()) => {
                                                    ctx.local_track = Arc::clone(&new_encoding);

                                                    // info!("Replaced track with new quality");
                                                }
                                                Err(err) => {
                                                    warn!("Got err when replace track: {:?}", err);
                                                }
                                            }
                                        }
                                    }
                                    None => {}
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    async fn get_appropriate_track(
        &self,
        remote_track: &TrackMutexWrapper,
    ) -> Result<Arc<TrackLocalStaticRTP>, WebRTCError> {
        let preferred_quality =
            PreferredQuality::from_u8(self.preferred_quality.load(Ordering::SeqCst));

        let track = remote_track.lock().await;

        track
            .get_track_appropriate(&preferred_quality)
            .await
            .ok_or(WebRTCError::FailedToAddTrack)
    }

    pub fn close(&self) {
        self.cancel_token.cancel();

        self.tracks.clear();
        self.quality_change_counter.clear();

        let pc = Arc::clone(&self.peer_connection);
        let track_map = Arc::clone(&self.track_map);

        tokio::spawn(async move {
            track_map.lock().await.clear();
            let _ = pc.close().await;
        });
    }
}
