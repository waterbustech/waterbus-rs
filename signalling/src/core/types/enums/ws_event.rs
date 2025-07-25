#[derive(Debug)]
pub enum WsEvent {
    RoomPublish,
    RoomSubscribe,
    RoomSubscribeHlsLiveStream,
    RoomAnswerSubscriber,
    RoomLeave,
    RoomReconnect,
    RoomMigrate,

    RoomPublisherRenegotiation,
    RoomSubscriberRenegotiation,

    RoomPublisherCandidate,
    RoomSubscriberCandidate,

    RoomNewParticipant,
    RoomParticipantLeft,

    RoomVideoEnabled,
    RoomCameraType,
    RoomAudioEnabled,
    RoomScreenSharing,
    RoomHandRaising,
    RoomSubtitleTrack,

    ChatSend,
    ChatUpdate,
    ChatDelete,

    SystemDestroy,

    Connection,
    Disconnect,
}

impl WsEvent {
    pub fn to_str(&self) -> &str {
        match self {
            WsEvent::RoomPublish => "room.publish",
            WsEvent::RoomSubscribe => "room.subscribe",
            WsEvent::RoomSubscribeHlsLiveStream => "room.subscribe_hls_live_stream",
            WsEvent::RoomAnswerSubscriber => "room.answer_subscriber",
            WsEvent::RoomLeave => "room.leave",
            WsEvent::RoomReconnect => "room.reconnect",
            WsEvent::RoomMigrate => "room.migrate",

            WsEvent::RoomPublisherRenegotiation => "room.publisher_renegotiation",
            WsEvent::RoomSubscriberRenegotiation => "room.subscriber_renegotiation",

            WsEvent::RoomPublisherCandidate => "room.publisher_candidate",
            WsEvent::RoomSubscriberCandidate => "room.subscriber_candidate",

            WsEvent::RoomNewParticipant => "room.new_participant",
            WsEvent::RoomParticipantLeft => "room.participant_left",

            WsEvent::RoomVideoEnabled => "room.video_enabled",
            WsEvent::RoomCameraType => "room.camera_type",
            WsEvent::RoomAudioEnabled => "room.audio_enabled",
            WsEvent::RoomScreenSharing => "room.screen_sharing",
            WsEvent::RoomHandRaising => "room.hand_raising",
            WsEvent::RoomSubtitleTrack => "room.subscribe_subtitle",

            WsEvent::ChatSend => "chat.send",
            WsEvent::ChatUpdate => "chat.update",
            WsEvent::ChatDelete => "chat.delete",

            WsEvent::SystemDestroy => "system.destroy",

            WsEvent::Connection => "connection",
            WsEvent::Disconnect => "disconnect",
        }
    }
}
