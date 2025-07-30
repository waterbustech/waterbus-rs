use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use parking_lot::RwLock;
use str0m::{
    Input, Rtc,
    media::{KeyframeRequest, MediaData, Mid},
};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    entities::{media::Media, room::TrackIn, subscriber::Subscriber},
    models::{
        connection_type::ConnectionType,
        input_params::{JoinRoomParams, SubscribeParams},
    },
};

pub struct Publisher {
    pub participant_id: String,
    pub rtc: Arc<RwLock<Rtc>>,
    pub media: Arc<RwLock<Media>>,
    pub connection_type: AtomicU8,
    pub cancel_token: CancellationToken,
}

impl Publisher {
    pub fn new(rtc: Rtc, params: JoinRoomParams) -> Arc<Self> {
        let rtc_arc = Arc::new(RwLock::new(rtc));

        let media = Media::new(
            params.participant_id.clone(),
            rtc_arc.clone(),
            params.connection_type.clone(),
            params.streaming_protocol,
            params.is_ipv6_supported,
        );

        let publisher = Arc::new(Self {
            participant_id: params.participant_id.clone(),
            rtc: rtc_arc,
            media: Arc::new(RwLock::new(media)),
            connection_type: AtomicU8::new(params.connection_type.into()),
            cancel_token: CancellationToken::new(),
        });

        publisher
    }

    pub fn add_subscriber(&self, subscriber_id: String, subscriber: Arc<Subscriber>) {
        let mut media = self.media.write();
        media.add_subscriber(subscriber_id, subscriber);
    }

    pub fn remove_subscriber(&self, subscriber_id: &str) {
        let mut media = self.media.write();
        media.remove_subscriber(subscriber_id);
    }

    pub fn get_subscriber(
        &self,
        subscriber_id: &str,
    ) -> Result<Arc<Subscriber>, crate::errors::WebRTCError> {
        let media = self.media.read();
        media.get_subscriber(subscriber_id)
    }

    pub fn subscribe_to_publisher(
        &self,
        params: SubscribeParams,
        subscriber_rtc: Rtc,
    ) -> Result<Arc<Subscriber>, crate::errors::WebRTCError> {
        // Create subscriber with the provided RTC instance
        let subscriber = Arc::new(Subscriber::new(
            params.participant_id.clone(),
            Arc::new(RwLock::new(subscriber_rtc)),
        ));

        // Add subscriber to this publisher
        self.add_subscriber(params.participant_id, subscriber.clone());

        Ok(subscriber)
    }

    pub fn handle_input(&self, input: Input) {
        let mut rtc = self.rtc.write();
        if let Err(e) = rtc.handle_input(input) {
            tracing::warn!("Publisher ({}) input failed: {:?}", self.participant_id, e);
        }
    }

    pub fn handle_track_open(&self, track_in: Arc<TrackIn>) {
        info!(
            "🎵 Publisher {} handling track open - mid: {:?}, kind: {:?}",
            self.participant_id, track_in.mid, track_in.kind
        );
        let mut media = self.media.write();
        media.add_track(track_in.mid, track_in.kind);
    }

    pub fn handle_media_data_out(&self, _origin_id: &str, data: &MediaData) {
        info!(
            "📹 Publisher {} forwarding media data to {} subscribers",
            self.participant_id,
            self.media.read().subscribers.len()
        );
        let media = self.media.read();

        // Forward media to all subscribers of this publisher
        for subscriber in media.subscribers.iter() {
            if let Some(writer) = subscriber.value().rtc.write().writer(data.mid) {
                if let Err(e) =
                    writer.write(data.pt, std::time::Instant::now(), data.time, &*data.data)
                {
                    tracing::warn!(
                        "Failed to forward media to subscriber {}: {:?}",
                        subscriber.value().participant_id,
                        e
                    );
                }
            }
        }
    }

    pub fn handle_keyframe_request(&self, req: KeyframeRequest, mid_in: Mid) {
        info!(
            "🎬 Publisher {} handling keyframe request for mid: {:?}",
            self.participant_id, mid_in
        );
        let media = self.media.read();

        // Check if this publisher has the incoming track
        let has_incoming_track = media.tracks.iter().any(|entry| {
            let (mid, _) = entry.pair();
            *mid == mid_in
        });

        if !has_incoming_track {
            info!(
                "❌ Publisher {} doesn't have incoming track for mid: {:?}",
                self.participant_id, mid_in
            );
            return;
        }

        // Forward the keyframe request to subscribers
        for subscriber in media.subscribers.iter() {
            if let Some(mut writer) = subscriber.value().rtc.write().writer(req.mid) {
                if let Err(e) = writer.request_keyframe(req.rid, req.kind) {
                    tracing::warn!(
                        "Failed to request keyframe from subscriber {}: {:?}",
                        subscriber.value().participant_id,
                        e
                    );
                }
            }
        }
    }

    #[inline]
    pub fn set_connection_type(&self, connection_type: ConnectionType) {
        self.connection_type
            .store(connection_type.into(), Ordering::Relaxed);
    }

    #[inline]
    pub fn get_connection_type(&self) -> ConnectionType {
        self.connection_type.load(Ordering::Relaxed).into()
    }

    #[inline]
    pub fn close(&self) {
        self.cancel_token.cancel();

        // Stop the media
        let mut media = self.media.write();
        media.stop();
    }

    pub fn get_media_state(&self) -> crate::entities::media::MediaState {
        let media = self.media.read();
        let state = media.state.read();
        state.clone()
    }

    pub fn set_video_enabled(&self, enabled: bool) {
        let mut media = self.media.write();
        media.set_video_enabled(enabled);
    }

    pub fn set_audio_enabled(&self, enabled: bool) {
        let mut media = self.media.write();
        media.set_audio_enabled(enabled);
    }

    pub fn set_e2ee_enabled(&self, enabled: bool) {
        let mut media = self.media.write();
        media.set_e2ee_enabled(enabled);
    }

    pub fn set_camera_type(&self, camera_type: u8) {
        let mut media = self.media.write();
        media.set_camera_type(camera_type);
    }

    pub fn set_screen_sharing(&self, enabled: bool, screen_track_id: Option<String>) {
        let mut media = self.media.write();
        media.set_screen_sharing(enabled, screen_track_id);
    }

    pub fn set_hand_raising(&self, enabled: bool) {
        let mut media = self.media.write();
        media.set_hand_raising(enabled);
    }
}
