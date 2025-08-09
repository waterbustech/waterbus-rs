use std::time::{Duration, Instant};
use std::{
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread,
};

use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::mpsc::{self, Receiver};
use str0m::{
    change::SdpOffer,
    media::{Direction, KeyframeRequestKind, MediaData, MediaKind, Mid, Rid},
    Event, IceConnectionState, Rtc,
};
use tokio_util::sync::CancellationToken;

use crate::{
    errors::RtcError,
    models::{
        callbacks::{IceCandidateHandler, JoinedHandler},
        connection_type::ConnectionType,
        streaming_protocol::StreamingProtocol,
    },
};

use super::{subscriber::Subscriber, track::Track};
use crate::utils::udp_runtime::RtcUdpRuntime;

pub struct Publisher {
    pub participant_id: String,
    pub room_id: String,
    pub rtc: Arc<RwLock<Rtc>>,
    pub connection_type: AtomicU8,
    pub cancel_token: CancellationToken,
    pub subscribers: Arc<DashMap<String, Arc<Subscriber>>>,
    pub tracks: Arc<DashMap<String, Arc<Track>>>,
    pub is_video_enabled: Arc<AtomicU8>,
    pub is_audio_enabled: Arc<AtomicU8>,
    pub is_e2ee_enabled: Arc<AtomicU8>,
    pub streaming_protocol: StreamingProtocol,
    // pub track_event_sender: Option<mpsc::UnboundedSender<TrackSubscribedMessage>>,
    // pub track_event_receiver: Option<mpsc::UnboundedReceiver<TrackSubscribedMessage>>,
    // pub ice_handler: Option<I>,
    // pub joined_handler: Option<J>,
    // Map incoming mid -> kind, populated on MediaAdded
    pub incoming_mid_kind: Arc<DashMap<Mid, MediaKind>>,
    // Throttle keyframe requests per mid
    pub last_kf_req: Arc<DashMap<Mid, Instant>>,
}

impl Publisher {
    pub fn new<I, J>(
        participant_id: String,
        room_id: String,
        connection_type: ConnectionType,
        is_video_enabled: bool,
        is_audio_enabled: bool,
        is_e2ee_enabled: bool,
        streaming_protocol: StreamingProtocol,
        ice_handler: I,
        joined_handler: J,
    ) -> Result<Arc<Self>, RtcError>
    where
        I: IceCandidateHandler,
        J: JoinedHandler + Clone,
    {
        // Create str0m RTC instance
        let rtc = Rtc::builder().build();

        // Create event channel for runtime -> publisher
        let (event_tx, event_rx) = mpsc::sync_channel(1);

        let publisher = Arc::new(Self {
            participant_id,
            room_id,
            rtc: Arc::new(RwLock::new(rtc)),
            connection_type: AtomicU8::new(connection_type.into()),
            cancel_token: CancellationToken::new(),
            subscribers: Arc::new(DashMap::new()),
            tracks: Arc::new(DashMap::new()),
            is_video_enabled: Arc::new(AtomicU8::new(if is_video_enabled { 1 } else { 0 })),
            is_audio_enabled: Arc::new(AtomicU8::new(if is_audio_enabled { 1 } else { 0 })),
            is_e2ee_enabled: Arc::new(AtomicU8::new(if is_e2ee_enabled { 1 } else { 0 })),
            streaming_protocol,
            // track_event_sender: Some(track_event_sender),
            // track_event_receiver: Some(track_event_receiver),
            // ice_handler: Some(ice_handler),
            // joined_handler: Some(joined_handler),
            incoming_mid_kind: Arc::new(DashMap::new()),
            last_kf_req: Arc::new(DashMap::new()),
        });

        // Register with global UDP runtime
        let runtime = RtcUdpRuntime::global();
        runtime
            .register_rtc(
                format!("pub:{}:{}", publisher.participant_id, publisher.room_id),
                Arc::clone(&publisher.rtc),
                event_tx,
            )
            .ok();

        // Announce host candidate to the client via callback
        if let (Some(cb), Some(host)) =
            (Some(ice_handler), RtcUdpRuntime::global().host_candidate())
        {
            let ice = crate::utils::ice_utils::IceUtils::convert_from_str0m_candidate(
                &host,
                Some("0".to_string()),
                Some(0),
            );
            cb.handle_candidate(ice);
        }

        // Start the RTC event loop (events from runtime)
        let publisher_clone = Arc::clone(&publisher);
        thread::spawn(move || {
            publisher_clone.run_event_loop(event_rx, joined_handler);
        });

        Ok(publisher)
    }

    pub fn handle_offer(&self, offer_sdp: String) -> Result<String, RtcError> {
        let mut rtc = self.rtc.write();

        // Parse and set remote offer (support both raw SDP and JSON-wrapped SDP)
        let offer: str0m::change::SdpOffer = match SdpOffer::from_sdp_string(&offer_sdp) {
            Ok(o) => o,
            Err(_) => serde_json::from_str(&offer_sdp).map_err(|_| RtcError::FailedToSetSdp)?,
        };

        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| RtcError::Str0mError(e))?;

        // Convert answer to SDP string
        let answer_sdp = answer.to_sdp_string();

        Ok(answer_sdp)
    }

    pub async fn create_offer(&self) -> Result<String, RtcError> {
        let mut rtc = self.rtc.write();

        // Add media tracks based on enabled flags
        let mut changes = rtc.sdp_api();

        if self.is_video_enabled.load(Ordering::Relaxed) == 1 {
            let _video_mid =
                changes.add_media(MediaKind::Video, Direction::SendRecv, None, None, None);
        }

        if self.is_audio_enabled.load(Ordering::Relaxed) == 1 {
            let _audio_mid =
                changes.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
        }

        let (offer, _pending) = changes.apply().ok_or(RtcError::FailedToCreateOffer)?;

        // Return SDP string in standard format
        Ok(offer.to_sdp_string())
    }

    pub fn add_subscriber(&self, subscriber_id: String, subscriber: Arc<Subscriber>) {
        self.subscribers.insert(subscriber_id, subscriber);

        // Notify subscriber about available tracks
        for track_entry in self.tracks.iter() {
            let _track = track_entry.value();
            // TODO: Send track information to subscriber
        }
    }

    pub fn remove_subscriber(&self, subscriber_id: &str) {
        self.subscribers.remove(subscriber_id);
    }

    pub fn get_subscribers(&self) -> Vec<Arc<Subscriber>> {
        self.subscribers
            .iter()
            .map(|entry| Arc::clone(entry.value()))
            .collect()
    }

    pub fn set_video_enabled(&self, enabled: bool) {
        self.is_video_enabled
            .store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
        // TODO: Update RTC session to enable/disable video
    }

    pub fn set_audio_enabled(&self, enabled: bool) {
        self.is_audio_enabled
            .store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
        // TODO: Update RTC session to enable/disable audio
    }

    pub fn set_e2ee_enabled(&self, enabled: bool) {
        self.is_e2ee_enabled
            .store(if enabled { 1 } else { 0 }, Ordering::Relaxed);
    }

    pub fn close(&self) {
        self.cancel_token.cancel();
        self.subscribers.clear();
        self.tracks.clear();
    }

    fn run_event_loop<J>(self: Arc<Self>, rx: Receiver<Event>, joined_handler: J)
    where
        J: JoinedHandler + Clone,
    {
        while let Ok(event) = rx.recv() {
            self.handle_rtc_event(event, joined_handler.clone());
            if self.cancel_token.is_cancelled() {
                break;
            }
        }
    }

    fn handle_rtc_event<J>(&self, event: Event, joined_handler: J)
    where
        J: JoinedHandler,
    {
        match event {
            Event::Connected => {
                tracing::info!("Publisher {} connected", self.participant_id);
                joined_handler.handle_joined(true);
            }
            Event::IceConnectionStateChange(state) => {
                tracing::debug!("ICE connection state changed: {:?}", state);
                if matches!(state, IceConnectionState::Disconnected) {
                    tracing::info!("Publisher {} disconnected", self.participant_id);
                }
            }

            Event::MediaAdded(media) => {
                tracing::info!("Media added: {:?}", media);
                // Track kind for incoming mid
                self.incoming_mid_kind.insert(media.mid, media.kind);
                // TODO: Create track and notify subscribers
            }
            Event::MediaData(data) => {
                // Ask for keyframe if stream is not contiguous (helps new subscribers)
                self.request_keyframe_throttled(&data);

                // Forward media data to subscribers
                self.forward_media_to_subscribers(data);
            }
            Event::RtpPacket(packet) => {
                // Forward RTP packet to subscribers
                tracing::debug!("Received RTP packet: {:?}", packet);
            }
            _ => {
                tracing::debug!("Unhandled RTC event: {:?}", event);
            }
        }
    }

    fn forward_media_to_subscribers(&self, media_data: MediaData) {
        if media_data.rid.is_some() && media_data.rid != Some(Rid::from("h")) {
            return;
        }

        // For each subscriber, write the incoming media into its corresponding send-only mid
        for entry in self.subscribers.iter() {
            let subscriber = entry.value();

            let Some(kind) = self
                .incoming_mid_kind
                .get(&media_data.mid)
                .map(|v| *v.value())
            else {
                continue;
            };

            let Some(mid_out) = subscriber.get_send_mid(kind) else {
                continue;
            };
            let mut rtc = subscriber.rtc.write();
            if let Some(writer) = rtc.writer(mid_out) {
                if let Some(pt) = writer.match_params(media_data.params) {
                    // Errors will disconnect the subscriber's rtc; we just log and continue
                    if let Err(e) = writer.write(
                        pt,
                        media_data.network_time,
                        media_data.time,
                        media_data.data.clone(),
                    ) {
                        tracing::warn!(
                            "Forward write failed for subscriber {}: {:?}",
                            subscriber.participant_id,
                            e
                        );
                    }
                }
            }
        }
    }

    fn request_keyframe_throttled(&self, data: &MediaData) {
        if data.contiguous {
            return;
        }
        let Some(kind) = self.incoming_mid_kind.get(&data.mid).map(|v| *v.value()) else {
            return;
        };
        if kind != MediaKind::Video {
            return;
        }

        let now = Instant::now();
        if let Some(last) = self.last_kf_req.get(&data.mid) {
            if now.duration_since(*last.value()) < Duration::from_secs(1) {
                return;
            }
        }
        self.last_kf_req.insert(data.mid, now);

        let mut rtc = self.rtc.write();
        if let Some(mut writer) = rtc.writer(data.mid) {
            let _ = writer.request_keyframe(data.rid, KeyframeRequestKind::Fir);
        }
    }
}
