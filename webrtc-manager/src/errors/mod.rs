use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebRTCError {
    #[error("Failed to add track")]
    FailedToAddTrack,

    #[error("Failed to create offer")]
    FailedToCreateOffer,

    #[error("Failed to create answer")]
    FailedToCreateAnswer,

    #[error("Failed to create pc")]
    FailedToCreatePeer,

    #[error("Failed to add transceiver")]
    FailedToAddTransceiver,

    #[error("Failed to set sdp")]
    FailedToSetSdp,

    #[error("Failed to get sdp")]
    FailedToGetSdp, 

    #[error("Failed to add candidate")]
    FailedToAddCandidate,

    #[error("Peer not found")]
    PeerNotFound,

    #[error("Participant not found")]
    ParticipantNotFound,
}
