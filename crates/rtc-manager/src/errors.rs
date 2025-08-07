use thiserror::Error;

#[derive(Error, Debug)]
pub enum RtcError {
    #[error("Failed to create peer connection")]
    FailedToCreatePeer,

    #[error("Failed to create offer")]
    FailedToCreateOffer,

    #[error("Failed to create answer")]
    FailedToCreateAnswer,

    #[error("Failed to set SDP")]
    FailedToSetSdp,

    #[error("Peer not found")]
    PeerNotFound,

    #[error("Room not found")]
    RoomNotFound,

    #[error("Publisher not found")]
    PublisherNotFound,

    #[error("Subscriber not found")]
    SubscriberNotFound,

    #[error("Track not found")]
    TrackNotFound,

    #[error("Media not found")]
    MediaNotFound,

    #[error("Invalid SDP")]
    InvalidSdp,

    #[error("Invalid ICE candidate")]
    InvalidIceCandidate,

    #[error("Connection failed")]
    ConnectionFailed,

    #[error("Data channel error: {0}")]
    DataChannelError(String),

    #[error("RTP error: {0}")]
    RtpError(String),

    #[error("str0m error: {0}")]
    Str0mError(#[from] str0m::RtcError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Generic error: {0}")]
    Generic(String),
}

impl From<anyhow::Error> for RtcError {
    fn from(err: anyhow::Error) -> Self {
        RtcError::Generic(err.to_string())
    }
}
