use dashmap::DashMap;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};
use tokio::sync::{RwLock, watch};
use tokio_util::sync::CancellationToken;
use webrtc::{
    peer_connection::RTCPeerConnection,
    rtcp::payload_feedbacks::receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate,
    rtp_transceiver::{
        RTCRtpTransceiver, RTCRtpTransceiverInit,
        rtp_transceiver_direction::RTCRtpTransceiverDirection,
    },
    track::track_local::TrackLocal,
};

use crate::{errors::WebRTCError, models::TrackMutexWrapper};

use super::{forward_track::ForwardTrack, quality::TrackQuality};

const BWE_THRESHOLD_H: f32 = 1_000_000.0;
const BWE_THRESHOLD_F: f32 = 2_500_000.0;

type TrackMap = Arc<DashMap<String, Arc<RwLock<ForwardTrack>>>>;

#[derive(Debug)]
pub struct Subscriber {
    pub peer_connection: Arc<RTCPeerConnection>,
    cancel_token: CancellationToken,
    preferred_quality: Arc<AtomicU8>,
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
            let reader = forward_track.read().await;
            reader.local_track.clone()
        };

        let transceiver = peer_connection
            .add_transceiver_from_track(
                Arc::clone(&local_track) as Arc<dyn TrackLocal + Send + Sync>,
                Some(RTCRtpTransceiverInit {
                    direction: RTCRtpTransceiverDirection::Sendonly,
                    send_encodings: vec![],
                }),
            )
            .await
            .map_err(|_| WebRTCError::FailedToAddTrack)?;

        self.read_rtcp(&transceiver).await;

        self.track_map
            .insert(track_id.to_owned(), Arc::clone(&forward_track));

        Ok(())
    }

    async fn read_rtcp(&self, transceiver: &Arc<RTCRtpTransceiver>) {
        let rtp_sender = transceiver.sender().await;
        let cancel_token = self.cancel_token.clone();

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

    fn spawn_remb_loop(&self, cancel_token: CancellationToken, tx: watch::Sender<()>) {
        let pc2 = Arc::downgrade(&self.peer_connection);
        let preferred_quality = Arc::clone(&self.preferred_quality);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(4)) => {
                        if let Some(pc) = pc2.upgrade() {
                            Self::monitor_remb( pc, preferred_quality.clone(), tx.clone()).await;
                        }
                    }
                }
            }
        });
    }

    async fn monitor_remb(
        peer_connection: Arc<RTCPeerConnection>,
        preferred_quality: Arc<AtomicU8>,
        tx: watch::Sender<()>,
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
                                TrackQuality::High
                            } else if bitrate >= BWE_THRESHOLD_H {
                                TrackQuality::Medium
                            } else {
                                TrackQuality::Low
                            };

                            let current_u8 =
                                preferred_quality.load(std::sync::atomic::Ordering::Relaxed);
                            let current_quality = TrackQuality::from_u8(current_u8);

                            if current_quality != new_quality {
                                preferred_quality.store(
                                    new_quality.as_u8(),
                                    std::sync::atomic::Ordering::Relaxed,
                                );

                                let _ = tx.send(());
                            }
                        }
                    }
                }
                Ok(Err(_)) => {}
                Err(_) => {}
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
                        let mut track_writer = ctx.write().await;
                        track_writer.set_effective_quality(&preferred);
                    }
                }
            }
        });
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
