use crate::models::rtc_dto::IceCandidate;

pub trait IceCandidateHandler: Send + Sync + 'static {
    fn handle_candidate(&self, candidate: IceCandidate);
}

pub trait JoinedHandler: Send + Sync + 'static {
    fn handle_joined(&self, is_migrate: bool);
}

pub trait RenegotiationHandler: Send + Sync + 'static {
    fn handle_renegotiation(&self, sdp: String);
}
