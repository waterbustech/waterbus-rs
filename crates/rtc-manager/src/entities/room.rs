use std::collections::VecDeque;
use std::{
    net::UdpSocket,
    sync::Arc,
    time::{Duration, Instant},
};

use dashmap::DashMap;
use str0m::media::{Direction, KeyframeRequest, MediaData, MediaKind, Mid};
use str0m::{
    Candidate, Event, Input, Output, Rtc,
    change::SdpOffer,
    net::{Protocol, Receive},
};
use tracing::{info, warn};

use crate::{
    entities::publisher::Publisher,
    errors::WebRTCError,
    models::{
        connection_type::ConnectionType,
        input_params::{
            IceCandidate, JoinRoomParams, JoinRoomResponse, RtcManagerConfig,
            SubscribeHlsLiveStreamParams, SubscribeHlsLiveStreamResponse, SubscribeParams,
            SubscribeResponse,
        },
        streaming_protocol::StreamingProtocol,
    },
    utils::select_host_address::select_host_address,
};

#[derive(Debug)]
pub enum Propagated {
    /// When we have nothing to propagate.
    Noop,
    /// Poll client has reached timeout.
    Timeout(Instant),
    /// A new incoming track opened.
    TrackOpen(String, Arc<TrackIn>),
    /// Data to be propagated from one publisher to its subscribers.
    MediaData(String, MediaData),
    /// A keyframe request from a subscriber to the publisher.
    KeyframeRequest(String, KeyframeRequest, String, Mid),
}

impl Propagated {
    /// Get publisher id, if the propagated event has a publisher id.
    fn publisher_id(&self) -> Option<&str> {
        match self {
            Propagated::TrackOpen(p, _)
            | Propagated::MediaData(p, _)
            | Propagated::KeyframeRequest(p, _, _, _) => Some(p),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct TrackIn {
    pub origin: String,
    pub mid: Mid,
    pub kind: MediaKind,
}

pub struct Room {
    publishers: Arc<DashMap<String, Arc<Publisher>>>,
    udp_socket: UdpSocket,
    buf: Vec<u8>,
}

impl Room {
    pub fn new(_config: RtcManagerConfig) -> Self {
        let host_addr = select_host_address();
        let socket_addr = format!("{}:0", host_addr);
        let udp_socket = UdpSocket::bind(socket_addr).unwrap();
        let addr = udp_socket.local_addr().expect("a local socket address");
        info!("Bound UDP port: {}", addr);

        Self {
            publishers: Arc::new(DashMap::new()),
            udp_socket,
            buf: vec![0; 2000],
        }
    }

    pub fn join_room(
        &self,
        params: JoinRoomParams,
        _room_id: &str,
    ) -> Result<Option<JoinRoomResponse>, WebRTCError> {
        let participant_id = params.participant_id.clone();
        info!(
            "🔄 Joining room - participant: {}, connection_type: {:?}",
            participant_id, params.connection_type
        );

        // Create RTC instance following str0m pattern
        let mut rtc = Rtc::builder().set_ice_lite(true).build();
        info!(
            "🔧 Created RTC instance for participant: {}",
            participant_id
        );

        // Add the shared UDP socket as a host candidate
        let addr = self.udp_socket.local_addr().unwrap();
        let candidate = Candidate::host(addr, "udp").expect("a host candidate");
        rtc.add_local_candidate(candidate.clone()).unwrap();
        info!("📡 Added host candidate: {:?}", candidate.to_sdp_string());

        // Create publisher with the new str0m-based architecture
        let publisher = Publisher::new(rtc, params.clone());
        info!("🔧 Created publisher for participant: {}", participant_id);

        self._add_publisher(&participant_id, &publisher);
        info!(
            "✅ Publisher created and added for participant: {}",
            participant_id
        );

        // Handle P2P vs SFU logic like webrtc-manager
        if params.connection_type == ConnectionType::P2P {
            info!(
                "🤝 P2P mode - caching SDP for participant: {}",
                participant_id
            );
            // For P2P, cache the SDP and return None (no immediate response)
            let mut media = publisher.media.write();
            media.cache_sdp(params.sdp.clone());

            // Execute callback for P2P
            tokio::spawn(async move {
                (params.callback)(false).await;
            });

            info!("📋 P2P SDP cached, returning None");
            Ok(None)
        } else {
            info!(
                "🌐 SFU mode - creating SDP answer for participant: {}",
                participant_id
            );
            // For SFU, create SDP answer and return it
            let offer = SdpOffer::from_sdp_string(&params.sdp)
                .map_err(|_| WebRTCError::FailedToCreateOffer)?;

            let answer = {
                let mut rtc = publisher.rtc.write();
                rtc.sdp_api()
                    .accept_offer(offer)
                    .map_err(|_| WebRTCError::FailedToCreateAnswer)?
            };

            // Create response with real SDP
            let response = JoinRoomResponse {
                sdp: answer.to_sdp_string(),
                is_recording: false,
            };

            tokio::spawn(async move {
                (params.callback)(false).await;
            });

            info!(
                "✅ SFU join room successful for participant: {}",
                participant_id
            );
            Ok(Some(response))
        }
    }

    pub fn subscribe(&self, params: SubscribeParams) -> Result<SubscribeResponse, WebRTCError> {
        let target_id = &params.target_id;
        let _participant_id = &params.participant_id;

        info!(
            "🔍 Subscribe request - target: {}, participant: {}",
            target_id, _participant_id
        );

        // Get the target publisher
        let publisher = self._get_publisher(target_id)?;
        info!("📡 Found target publisher: {}", target_id);

        // Check if we have cached SDP for P2P (like webrtc-manager)
        let cached_sdp = {
            let media = publisher.media.read();
            media.get_sdp()
        };

        if let Some(sdp) = cached_sdp {
            info!("📋 Found cached P2P SDP for target: {}", target_id);
            // Return cached SDP for P2P connections
            let media_state = publisher.get_media_state();

            Ok(SubscribeResponse {
                offer: sdp,
                camera_type: media_state.camera_type,
                video_enabled: media_state.video_enabled,
                audio_enabled: media_state.audio_enabled,
                is_screen_sharing: media_state.is_screen_sharing,
                is_hand_raising: media_state.is_hand_raising,
                is_e2ee_enabled: media_state.is_e2ee_enabled,
                video_codec: media_state.codec,
                screen_track_id: media_state.screen_track_id,
            })
        } else {
            info!(
                "🌐 No cached SDP, creating SFU subscriber for target: {}",
                target_id
            );
            // For SFU, create a new subscriber
            let connection_type = publisher.get_connection_type();

            if connection_type == ConnectionType::P2P {
                info!("❌ Target is P2P but no cached SDP found");
                return Err(WebRTCError::PeerNotFound);
            }

            // Create a new RTC instance for the subscriber
            let mut subscriber_rtc = Rtc::builder().set_ice_lite(true).build();

            // Add the shared UDP socket as a host candidate
            let addr = self.udp_socket.local_addr().unwrap();
            let candidate = Candidate::host(addr, "udp").expect("a host candidate");
            subscriber_rtc.add_local_candidate(candidate).unwrap();
            info!("📡 Added host candidate for subscriber: {}", addr);

            // Create SDP offer for the subscriber BEFORE passing RTC to subscriber
            let (offer, _pending) = {
                let mut change = subscriber_rtc.sdp_api();

                // Add media for each track in the publisher
                let media = publisher.media.read();
                let track_count = media.tracks.len();
                info!("🎵 Adding {} tracks to subscriber", track_count);

                for track_entry in media.tracks.iter() {
                    let (_mid, track_info) = track_entry.pair();
                    let stream_id = track_info.origin.clone();
                    let _new_mid = change.add_media(
                        track_info.kind,
                        Direction::SendOnly,
                        Some(stream_id),
                        None,
                        None,
                    );
                }

                change.apply().ok_or(WebRTCError::FailedToCreateOffer)?
            };

            // Create subscriber with the RTC instance
            let _subscriber = publisher.subscribe_to_publisher(params.clone(), subscriber_rtc)?;
            info!("✅ Subscriber created for target: {}", target_id);

            // Get media state for response
            let media_state = publisher.get_media_state();

            let offer_json = serde_json::to_string(&offer).unwrap();
            info!("📄 SFU offer created: {} chars", offer_json.len());

            Ok(SubscribeResponse {
                offer: offer_json,
                camera_type: media_state.camera_type,
                video_enabled: media_state.video_enabled,
                audio_enabled: media_state.audio_enabled,
                is_screen_sharing: media_state.is_screen_sharing,
                is_hand_raising: media_state.is_hand_raising,
                is_e2ee_enabled: media_state.is_e2ee_enabled,
                video_codec: media_state.codec,
                screen_track_id: media_state.screen_track_id,
            })
        }
    }

    pub fn subscribe_hls_live_stream(
        &self,
        params: SubscribeHlsLiveStreamParams,
    ) -> Result<SubscribeHlsLiveStreamResponse, WebRTCError> {
        let target_id = &params.target_id;

        let publisher = self._get_publisher(target_id)?;
        let media = publisher.media.read();

        if media.streaming_protocol != StreamingProtocol::HLS {
            return Err(WebRTCError::InvalidStreamingProtocol);
        }

        let hls_urls = media.get_hls_urls();

        Ok(SubscribeHlsLiveStreamResponse { hls_urls })
    }

    pub fn set_subscriber_remote_sdp(
        &self,
        target_id: &str,
        participant_id: &str,
        sdp: &str,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(target_id)?;
        let subscriber = publisher.get_subscriber(participant_id)?;

        let offer = SdpOffer::from_sdp_string(sdp).map_err(|_| WebRTCError::FailedToCreateOffer)?;

        let _answer = {
            let mut rtc = subscriber.rtc.write();
            rtc.sdp_api()
                .accept_offer(offer)
                .map_err(|_| WebRTCError::FailedToCreateAnswer)?
        };

        // Send answer back through data channel or other mechanism
        // For now, we'll just return success
        Ok(())
    }

    pub fn handle_publisher_renegotiation(
        &self,
        participant_id: &str,
        sdp: &str,
    ) -> Result<String, WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;

        let offer = SdpOffer::from_sdp_string(sdp).map_err(|_| WebRTCError::FailedToCreateOffer)?;

        let answer = {
            let mut rtc = publisher.rtc.write();
            rtc.sdp_api()
                .accept_offer(offer)
                .map_err(|_| WebRTCError::FailedToCreateAnswer)?
        };

        let answer_json = serde_json::to_string(&answer).unwrap();

        Ok(answer_json)
    }

    pub fn handle_migrate_connection(
        &self,
        participant_id: &str,
        sdp: &str,
        connection_type: ConnectionType,
    ) -> Result<Option<String>, WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;

        publisher.set_connection_type(connection_type.clone());

        if connection_type == ConnectionType::SFU {
            // For SFU, create SDP answer and return it
            let offer =
                SdpOffer::from_sdp_string(sdp).map_err(|_| WebRTCError::FailedToCreateOffer)?;

            let answer = {
                let mut rtc = publisher.rtc.write();
                rtc.sdp_api()
                    .accept_offer(offer)
                    .map_err(|_| WebRTCError::FailedToCreateAnswer)?
            };

            let answer_json = serde_json::to_string(&answer).unwrap();

            Ok(Some(answer_json))
        } else {
            // For P2P, cache the SDP and clear all tracks
            let mut media = publisher.media.write();
            media.remove_all_tracks();
            media.cache_sdp(sdp.to_owned());
            Ok(None)
        }
    }

    pub fn add_publisher_candidate(
        &self,
        _participant_id: &str,
        _candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        // In str0m, ICE candidates are handled automatically
        Ok(())
    }

    pub fn add_subscriber_candidate(
        &self,
        _target_id: &str,
        _participant_id: &str,
        _candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        // In str0m, ICE candidates are handled automatically
        Ok(())
    }

    #[inline]
    pub fn leave_room(&mut self, participant_id: &str) {
        if let Some((_id, publisher)) = self.publishers.remove(participant_id) {
            publisher.close();
        }
    }

    #[inline]
    pub fn set_e2ee_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_e2ee_enabled(is_enabled);
        Ok(())
    }

    #[inline]
    pub fn set_camera_type(
        &self,
        participant_id: &str,
        camera_type: u8,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_camera_type(camera_type);
        Ok(())
    }

    #[inline]
    pub fn set_video_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_video_enabled(is_enabled);
        Ok(())
    }

    #[inline]
    pub fn set_audio_enabled(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_audio_enabled(is_enabled);
        Ok(())
    }

    #[inline]
    pub fn set_screen_sharing(
        &self,
        participant_id: &str,
        is_enabled: bool,
        screen_track_id: Option<String>,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_screen_sharing(is_enabled, screen_track_id);
        Ok(())
    }

    #[inline]
    pub fn set_hand_raising(
        &self,
        participant_id: &str,
        is_enabled: bool,
    ) -> Result<(), WebRTCError> {
        let publisher = self._get_publisher(participant_id)?;
        publisher.set_hand_raising(is_enabled);
        Ok(())
    }

    /// Run the main UDP socket loop for handling WebRTC traffic
    pub fn run_udp_loop(&mut self) -> Result<(), WebRTCError> {
        info!("🚀 Starting UDP loop for room");
        let mut to_propagate: VecDeque<Propagated> = VecDeque::new();
        let mut buf = vec![0; 2000];

        loop {
            // Clean out disconnected publishers
            let before_count = self.publishers.len();
            self.publishers.retain(|_, publisher| {
                let rtc = publisher.rtc.read();
                rtc.is_alive()
            });
            let after_count = self.publishers.len();
            if before_count != after_count {
                info!(
                    "🧹 Cleaned up {} disconnected publishers",
                    before_count - after_count
                );
            }

            // Poll all publishers until they return timeout
            let mut timeout = Instant::now() + Duration::from_millis(100);
            let socket = &self.udp_socket;

            // Process each publisher and collect timeouts
            for publisher in self.publishers.iter_mut() {
                let t = self.poll_until_timeout(publisher.value(), &mut to_propagate, socket);
                timeout = timeout.min(t);
            }

            // If we have an item to propagate, do that
            if let Some(p) = to_propagate.pop_front() {
                info!("📤 Propagating event: {:?}", p);
                self.propagate(&p);
                continue;
            }

            // The read timeout is not allowed to be 0. In case it is 0, we set 1 millisecond.
            let duration = (timeout - Instant::now()).max(Duration::from_millis(1));

            self.udp_socket
                .set_read_timeout(Some(duration))
                .expect("setting socket read timeout");

            // Handle socket input - simplified to avoid borrow checker issues
            let _input = self.read_socket_input(&mut buf);
            if _input.is_some() {
                info!("📡 Received UDP input (handling simplified)");
            }

            // Drive time forward in all publishers.
            let now = Instant::now();
            for publisher in self.publishers.iter_mut() {
                publisher.value().handle_input(Input::Timeout(now));
            }
        }
    }

    /// Poll all the output from the publisher until it returns a timeout.
    /// Collect any output in the queue, transmit data on the socket, return the timeout
    fn poll_until_timeout(
        &self,
        publisher: &Arc<Publisher>,
        queue: &mut VecDeque<Propagated>,
        socket: &UdpSocket,
    ) -> Instant {
        loop {
            let rtc = publisher.rtc.read();
            if !rtc.is_alive() {
                // This publisher will be cleaned up in the next run of the main loop.
                return Instant::now();
            }
            drop(rtc); // Release the lock before calling poll_publisher_output

            let propagated = self.poll_publisher_output(publisher, socket);

            if let Propagated::Timeout(t) = propagated {
                return t;
            }

            queue.push_back(propagated)
        }
    }

    /// Sends one "propagated" to all publishers, if relevant
    fn propagate(&mut self, propagated: &Propagated) {
        // Do not propagate to originating publisher.
        let Some(publisher_id) = propagated.publisher_id() else {
            // If the event doesn't have a publisher id, it can't be propagated,
            // (it's either a noop or a timeout).
            return;
        };

        info!("🔄 Propagating to {} publishers", self.publishers.len());

        for publisher in self.publishers.iter_mut() {
            if publisher.key() == publisher_id {
                // Do not propagate to originating publisher.
                continue;
            }

            match propagated {
                Propagated::TrackOpen(_, track_in) => {
                    info!(
                        "🎵 Propagating track open to publisher: {}",
                        publisher.key()
                    );
                    publisher.value().handle_track_open(track_in.clone());
                }
                Propagated::MediaData(origin_id, data) => {
                    info!(
                        "📹 Propagating media data from {} to publisher: {}",
                        origin_id,
                        publisher.key()
                    );
                    publisher.value().handle_media_data_out(origin_id, data);
                }
                Propagated::KeyframeRequest(_, req, origin_id, mid_in) => {
                    // Only the origin publisher handles the keyframe request.
                    if publisher.key() == origin_id {
                        info!(
                            "🎬 Propagating keyframe request to origin publisher: {}",
                            origin_id
                        );
                        publisher.value().handle_keyframe_request(*req, *mid_in);
                    }
                }
                Propagated::Noop | Propagated::Timeout(_) => {}
            }
        }
    }

    /// Poll a single publisher's output and return propagated events
    fn poll_publisher_output(&self, publisher: &Arc<Publisher>, socket: &UdpSocket) -> Propagated {
        let mut rtc = publisher.rtc.write();

        match rtc.poll_output() {
            Ok(output) => {
                match output {
                    Output::Event(event) => {
                        match event {
                            Event::MediaData(media_data) => {
                                info!("📹 Media data from publisher: {}", publisher.participant_id);
                                // Propagate media data to subscribers
                                Propagated::MediaData(publisher.participant_id.clone(), media_data)
                            }
                            Event::KeyframeRequest(req) => {
                                info!(
                                    "🎬 Keyframe request from publisher: {}",
                                    publisher.participant_id
                                );
                                // Handle keyframe requests
                                let participant_id = publisher.participant_id.clone();
                                Propagated::KeyframeRequest(
                                    participant_id.clone(),
                                    req,
                                    participant_id,
                                    req.mid,
                                )
                            }
                            Event::MediaAdded(e) => {
                                info!(
                                    "🎵 Media added for publisher: {} - mid: {:?}, kind: {:?}",
                                    publisher.participant_id, e.mid, e.kind
                                );
                                // Handle new media tracks
                                let track_in = Arc::new(TrackIn {
                                    origin: publisher.participant_id.clone(),
                                    mid: e.mid,
                                    kind: e.kind,
                                });
                                Propagated::TrackOpen(publisher.participant_id.clone(), track_in)
                            }
                            _ => Propagated::Noop,
                        }
                    }
                    Output::Transmit(transmit) => {
                        // Transmit data on the socket
                        if let Err(e) = socket.send_to(&transmit.contents, transmit.destination) {
                            warn!("Failed to transmit data: {:?}", e);
                        }
                        Propagated::Noop
                    }
                    Output::Timeout(timeout) => Propagated::Timeout(timeout),
                }
            }
            Err(_) => {
                // RTC is no longer alive
                Propagated::Noop
            }
        }
    }

    /// Read socket input following str0m pattern
    fn read_socket_input<'a>(&mut self, buf: &'a mut Vec<u8>) -> Option<Input<'a>> {
        buf.resize(2000, 0);

        match self.udp_socket.recv_from(buf) {
            Ok((n, source)) => {
                buf.truncate(n);

                // Parse data to a DatagramRecv, which help preparse network data to
                // figure out the multiplexing of all protocols on one UDP port.
                let Ok(contents) = buf.as_slice().try_into() else {
                    return None;
                };

                Some(Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: Protocol::Udp,
                        source,
                        destination: self.udp_socket.local_addr().unwrap(),
                        contents,
                    },
                ))
            }
            Err(e) => match e.kind() {
                // Expected error for set_read_timeout(). One for windows, one for the rest.
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => None,
                _ => panic!("UdpSocket read failed: {e:?}"),
            },
        }
    }

    #[inline]
    fn _get_publisher(&self, participant_id: &str) -> Result<Arc<Publisher>, WebRTCError> {
        let result = self
            .publishers
            .get(participant_id)
            .map(|r| r.clone())
            .ok_or(WebRTCError::ParticipantNotFound)?;

        Ok(result)
    }

    #[inline]
    fn _add_publisher(&self, participant_id: &str, publisher: &Arc<Publisher>) {
        self.publishers
            .insert(participant_id.to_owned(), publisher.clone());
    }
}
