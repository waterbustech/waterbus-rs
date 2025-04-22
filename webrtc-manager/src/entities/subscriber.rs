use crate::{errors::WebRTCError, models::TrackMutexWrapper};
use std::{collections::HashMap, sync::Arc, time::Duration};
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

#[derive(Debug, Clone)]
pub enum PreferredQuality {
    Low,
    Medium,
    High,
}

#[derive(Debug)]
pub struct Subscriber {
    pub peer_connection: Arc<RTCPeerConnection>,
    cancel_token: CancellationToken,
    preferred_quality: Arc<Mutex<PreferredQuality>>,
    tracks: Arc<Mutex<HashMap<String, TrackMutexWrapper>>>,
    track_map: TrackMap,
}

impl Subscriber {
    pub fn new(peer_connection: Arc<RTCPeerConnection>) -> Self {
        let cancel_token = CancellationToken::new();
        let (tx, _rx) = watch::channel(());

        let this = Self {
            peer_connection,
            cancel_token: cancel_token.clone(),
            preferred_quality: Arc::new(Mutex::new(PreferredQuality::Medium)),
            tracks: Arc::new(Mutex::new(HashMap::new())),
            track_map: Arc::new(Mutex::new(HashMap::new())),
        };

        this._spawn_remb_loop(cancel_token, tx.clone());
        this._spawn_track_update_loop(tx);

        this
    }

    pub async fn add_track(&self, remote_track: TrackMutexWrapper) -> Result<(), WebRTCError> {
        let mut tracks = self.tracks.lock().await;
        let track_id = {
            let track_guard = remote_track.lock().await;
            track_guard.id.clone()
        };

        if let Some(existing) = tracks.get_mut(&track_id) {
            *existing = remote_track;
        } else {
            tracks.insert(track_id.to_string(), remote_track.clone());
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

        self.track_map.lock().await.insert(
            track_id.to_owned(),
            TrackContext {
                transceiver: Arc::clone(&transceiver),
                local_track: Arc::clone(&track_selected),
            },
        );

        Ok(())
    }

    fn _spawn_remb_loop(&self, cancel_token: CancellationToken, tx: watch::Sender<()>) {
        let pc2 = Arc::downgrade(&self.peer_connection);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        // info!("[RTCP] REMB loop cancelled");
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(4)) => {
                        if let Some(pc) = pc2.upgrade() {
                            Self::_monitor_remb(pc, Arc::clone(&preferred_quality), tx.clone()).await;
                        }
                    }
                }
            }
        });
    }

    async fn _monitor_remb(
        peer_connection: Arc<RTCPeerConnection>,
        preferred_quality: Arc<Mutex<PreferredQuality>>,
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
                                PreferredQuality::High
                            } else if bitrate >= BWE_THRESHOLD_H {
                                PreferredQuality::Medium
                            } else {
                                PreferredQuality::Low
                            };

                            let mut quality = preferred_quality.lock().await;
                            if std::mem::discriminant(&*quality)
                                != std::mem::discriminant(&new_quality)
                            {
                                *quality = new_quality;
                                // info!(
                                //     "Bandwidth quality changed, updating tracks to {:?}",
                                //     *quality
                                // );
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

    fn _spawn_track_update_loop(&self, tx: watch::Sender<()>) {
        let tracks = Arc::clone(&self.tracks);
        let preferred_quality = Arc::clone(&self.preferred_quality);
        let track_map = Arc::clone(&self.track_map);
        // let peer_connection = Arc::clone(&self.peer_connection);
        let mut rx = tx.subscribe();

        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let preferred = preferred_quality.lock().await.clone();
                let tracks = tracks.lock().await;
                let mut track_map = track_map.lock().await;

                for (track_id, track) in tracks.iter() {
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
        let preferred_quality = self.preferred_quality.lock().await;

        let track = remote_track.lock().await;

        track
            .get_track_appropriate(&preferred_quality)
            .await
            .ok_or(WebRTCError::FailedToAddTrack)
    }

    pub async fn close(&self) {
        self.cancel_token.cancel();
        let _ = self.peer_connection.close().await;
    }
}
