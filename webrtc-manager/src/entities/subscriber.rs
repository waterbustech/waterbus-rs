use std::sync::Arc;

use webrtc::peer_connection::RTCPeerConnection;

pub struct Subscriber {
    pub peer_connection: Arc<RTCPeerConnection>,
}
