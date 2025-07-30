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
}

impl Room {
    pub fn new(config: RtcManagerConfig) -> Self {
        // Use public IP from config if available, otherwise fallback to auto-detected
        let host_addr = if !config.public_ip.is_empty() {
            config.public_ip.parse()
                .unwrap_or_else(|_| {
                    warn!(event = "invalid_public_ip", public_ip = %config.public_ip);
                    select_host_address()
                })
        } else {
            select_host_address()
        };
        
        let udp_socket =
            UdpSocket::bind(format!("{host_addr}:0")).expect("binding a random UDP port");
        let addr = udp_socket.local_addr().expect("a local socket address");
        info!(event = "udp_bound", port = %addr, public_ip = %config.public_ip);

        Self {
            publishers: Arc::new(DashMap::new()),
            udp_socket,
        }
    }

    pub fn join_room(
        &self,
        params: JoinRoomParams,
        _room_id: &str,
    ) -> Result<Option<JoinRoomResponse>, WebRTCError> {
        let participant_id = params.participant_id.clone();
        info!(
            event = "join_room_requested",
            participant_id = %participant_id,
            connection_type = ?params.connection_type
        );

        // Create RTC instance following str0m pattern
        let mut rtc = Rtc::builder()
            // .set_ice_lite(true)
            .build();
        info!(
            event = "rtc_instance_created",
            participant_id = %participant_id
        );

        // Add the shared UDP socket as a host candidate
        let local_addr = self.udp_socket.local_addr().unwrap();
        let candidate = Candidate::host(local_addr, "udp").expect("a host candidate");
        rtc.add_local_candidate(candidate.clone()).unwrap();
        let candidate_json = serde_json::to_string(&candidate).unwrap();
        info!(event = "host_candidate_added", candidate = %candidate_json);

        // Create publisher with the new str0m-based architecture
        let publisher = Publisher::new(rtc, params.clone());
        info!(event = "publisher_created", participant_id = %participant_id);

        self._add_publisher(&participant_id, &publisher);
        info!(
            event = "publisher_added",
            participant_id = %participant_id
        );

        // Handle P2P vs SFU logic like webrtc-manager
        if params.connection_type == ConnectionType::P2P {
            info!(
                event = "p2p_mode_caching_sdp",
                participant_id = %participant_id
            );
            // For P2P, cache the SDP and return None (no immediate response)
            let mut media = publisher.media.write();
            media.cache_sdp(params.sdp.clone());

            // Execute callback for P2P
            tokio::spawn(async move {
                (params.callback)(false).await;
            });

            info!(event = "p2p_sdp_cached");
            Ok(None)
        } else {
            info!(
                event = "sfu_mode_creating_answer",
                participant_id = %participant_id
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
                candidate: candidate_json,
                is_recording: false,
            };

            tokio::spawn(async move {
                (params.callback)(false).await;
            });

            info!(
                event = "sfu_join_successful",
                participant_id = %participant_id
            );
            Ok(Some(response))
        }
    }

    pub fn subscribe(&self, params: SubscribeParams) -> Result<SubscribeResponse, WebRTCError> {
        let target_id = &params.target_id;
        let _participant_id = &params.participant_id;

        info!(
            event = "subscribe_requested",
            target_id = %target_id,
            participant_id = %_participant_id
        );

        // Get the target publisher
        let publisher = self._get_publisher(target_id)?;
        info!(event = "target_publisher_found", target_id = %target_id);

        // Check if we have cached SDP for P2P (like webrtc-manager)
        let cached_sdp = {
            let media = publisher.media.read();
            media.get_sdp()
        };

        if let Some(sdp) = cached_sdp {
            info!(event = "cached_p2p_sdp_found", target_id = %target_id);
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
                event = "no_cached_sdp_creating_sfu_subscriber",
                target_id = %target_id
            );
            // For SFU, create a new subscriber
            let connection_type = publisher.get_connection_type();

            if connection_type == ConnectionType::P2P {
                info!(event = "target_p2p_no_cached_sdp", target_id = %target_id);
                return Err(WebRTCError::PeerNotFound);
            }

            // Create a new RTC instance for the subscriber
            let mut subscriber_rtc = Rtc::builder().set_ice_lite(true).build();

            // Add the shared UDP socket as a host candidate
            let addr = self.udp_socket.local_addr().unwrap();
            let candidate = Candidate::host(addr, "udp").expect("a host candidate");
            subscriber_rtc.add_local_candidate(candidate).unwrap();
            info!(event = "subscriber_host_candidate_added", addr = %addr);

            // Create SDP offer for the subscriber BEFORE passing RTC to subscriber
            let (offer, _pending) = {
                let mut change = subscriber_rtc.sdp_api();

                // Add media for each track in the publisher
                let media = publisher.media.read();
                let track_count = media.tracks.len();
                info!(
                    event = "adding_tracks_to_subscriber",
                    track_count = track_count
                );

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
            info!(event = "subscriber_created", target_id = %target_id);

            // Get media state for response
            let media_state = publisher.get_media_state();

            let offer_json = serde_json::to_string(&offer).unwrap();
            info!(event = "sfu_offer_created", offer_length = offer_json.len());

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
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        // Get the publisher
        let publisher = self._get_publisher(participant_id)?;

        // Parse the ICE candidate string to create a str0m Candidate
        let str0m_candidate = Candidate::from_sdp_string(&candidate.candidate).map_err(|e| {
            warn!(event = "ice_candidate_parse_failed", error = %e);
            WebRTCError::InvalidIceCandidate
        })?;

        // Add the remote candidate to the RTC instance
        {
            let mut rtc = publisher.rtc.write();
            rtc.add_remote_candidate(str0m_candidate);
        }

        info!(
            event = "publisher_ice_candidate_added",
            participant_id = %participant_id
        );
        Ok(())
    }

    pub fn add_subscriber_candidate(
        &self,
        target_id: &str,
        participant_id: &str,
        candidate: IceCandidate,
    ) -> Result<(), WebRTCError> {
        // Get the target publisher
        let publisher = self._get_publisher(target_id)?;

        // Get the subscriber from the publisher
        let subscriber = publisher.get_subscriber(participant_id)?;

        // Parse the ICE candidate string to create a str0m Candidate
        let str0m_candidate = Candidate::from_sdp_string(&candidate.candidate).map_err(|e| {
            warn!(event = "ice_candidate_parse_failed", error = %e);
            WebRTCError::InvalidIceCandidate
        })?;

        // Add the remote candidate to the subscriber's RTC instance
        {
            let mut rtc = subscriber.rtc.write();
            rtc.add_remote_candidate(str0m_candidate);
        }

        info!(
            event = "subscriber_ice_candidate_added",
            participant_id = %participant_id,
            target_id = %target_id
        );
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
        info!(event = "udp_loop_started");
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
                    event = "disconnected_publishers_cleaned",
                    cleaned_count = before_count - after_count
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
                info!(event = "propagating_event", event_type = ?p);
                self.propagate(&p);
                continue;
            }

            // The read timeout is not allowed to be 0. In case it is 0, we set 1 millisecond.
            let duration = (timeout - Instant::now()).max(Duration::from_millis(1));

            self.udp_socket
                .set_read_timeout(Some(duration))
                .expect("setting socket read timeout");

            // Handle socket input and route to correct publisher
            if let Some(input) = self.read_socket_input(&mut buf) {
                info!(event = "udp_input_received");
                
                // Find the publisher that accepts this input
                let mut handled = false;
                for publisher_entry in self.publishers.iter() {
                    let rtc = publisher_entry.value().rtc.read();
                    if rtc.accepts(&input) {
                        drop(rtc); // Release read lock before handling input
                        publisher_entry.value().handle_input(input);
                        handled = true;
                        info!(
                            event = "udp_input_routed_to_publisher",
                            participant_id = %publisher_entry.key()
                        );
                        break;
                    }
                }
                
                if !handled {
                    warn!(event = "udp_input_not_handled");
                }
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

        info!(
            event = "propagating_to_publishers",
            publisher_count = self.publishers.len()
        );

        for publisher in self.publishers.iter_mut() {
            if publisher.key() == publisher_id {
                // Do not propagate to originating publisher.
                continue;
            }

            match propagated {
                Propagated::TrackOpen(_, track_in) => {
                    info!(
                        event = "track_open_propagated",
                        target_publisher = %publisher.key()
                    );
                    publisher.value().handle_track_open(track_in.clone());
                }
                Propagated::MediaData(origin_id, data) => {
                    info!(
                        event = "media_data_propagated",
                        origin_id = %origin_id,
                        target_publisher = %publisher.key()
                    );
                    publisher.value().handle_media_data_out(origin_id, data);
                }
                Propagated::KeyframeRequest(_, req, origin_id, mid_in) => {
                    // Only the origin publisher handles the keyframe request.
                    if publisher.key() == origin_id {
                        info!(
                            event = "keyframe_request_propagated",
                            origin_id = %origin_id
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
                                info!(event = "media_data_from_publisher", participant_id = %publisher.participant_id);
                                // Propagate media data to subscribers
                                Propagated::MediaData(publisher.participant_id.clone(), media_data)
                            }
                            Event::KeyframeRequest(req) => {
                                info!(
                                    event = "keyframe_request_from_publisher",
                                    participant_id = %publisher.participant_id
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
                                    event = "media_added",
                                    participant_id = %publisher.participant_id,
                                    mid = ?e.mid,
                                    kind = ?e.kind
                                );
                                // Handle new media tracks
                                let track_in = Arc::new(TrackIn {
                                    origin: publisher.participant_id.clone(),
                                    mid: e.mid,
                                    kind: e.kind,
                                });
                                Propagated::TrackOpen(publisher.participant_id.clone(), track_in)
                            }

                            Event::IceConnectionStateChange(state) => {
                                info!(
                                    event = "ice_connection_state_change",
                                    participant_id = %publisher.participant_id,
                                    new_state = ?state
                                );
                                Propagated::Noop
                            }
                            _ => Propagated::Noop,
                        }
                    }
                    Output::Transmit(transmit) => {
                        // Transmit data on the socket
                        if let Err(e) = socket.send_to(&transmit.contents, transmit.destination) {
                            warn!(event = "transmit_failed", error = ?e);
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
