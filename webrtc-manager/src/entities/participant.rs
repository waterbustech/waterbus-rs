use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use webrtc::{
    peer_connection::RTCPeerConnection,
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
};

use super::media::Media;

#[derive(Debug)]
pub struct Participant {
    pub media: Arc<Mutex<Media>>,
    pub peer_connection: Arc<RTCPeerConnection>,
}

impl Participant {
    pub fn new(media: Arc<Mutex<Media>>, peer_connection: Arc<RTCPeerConnection>) -> Self {
        Self {
            media: media,
            peer_connection: peer_connection,
        }
    }

    pub fn send_rtcp_pli(&self, media_ssrc: u32) {
        let pc2 = Arc::downgrade(&self.peer_connection);

        tokio::spawn(async move {
            let mut result = Result::<usize, anyhow::Error>::Ok(0);
            while result.is_ok() {
                let timeout = tokio::time::sleep(Duration::from_secs(3));
                tokio::pin!(timeout);

                tokio::select! {
                    _ = timeout.as_mut() => {
                        if let Some(pc) = pc2.upgrade() {
                            result = pc.write_rtcp(&[Box::new(PictureLossIndication {
                                sender_ssrc: 0,
                                media_ssrc,
                            })]).await.map_err(Into::into);
                        } else {
                            break;
                        }
                    }
                };
            }
        });
    }
}
