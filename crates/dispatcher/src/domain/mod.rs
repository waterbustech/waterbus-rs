use waterbus_proto::{
    NewUserJoinedRequest, PublisherCandidateRequest, SubscriberCandidateRequest,
    SubscriberRenegotiateRequest,
};

pub enum DispatcherCallback {
    NewUserJoined(NewUserJoinedRequest),
    SubscriberRenegotiate(SubscriberRenegotiateRequest),
    PublisherCandidate(PublisherCandidateRequest),
    SubscriberCandidate(SubscriberCandidateRequest),
    NodeTerminated(String),
}
