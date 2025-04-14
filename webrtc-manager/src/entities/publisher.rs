use std::{sync::Arc, time::Duration};

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use webrtc::{
    peer_connection::RTCPeerConnection,
    rtcp::payload_feedbacks::{
        picture_loss_indication::PictureLossIndication,
        receiver_estimated_maximum_bitrate::ReceiverEstimatedMaximumBitrate,
    },
};

use super::media::Media;

#[derive(Debug)]
pub enum PreferredQuality {
    Low,
    Medium,
    High,
}

#[derive(Debug)]
pub struct Publisher {
    pub media: Arc<Mutex<Media>>,
    pub peer_connection: Arc<RTCPeerConnection>,
    cancel_token: CancellationToken,
}

impl Publisher {
    pub fn new(media: Arc<Mutex<Media>>, peer_connection: Arc<RTCPeerConnection>) -> Self {
        let this = Self {
            media,
            peer_connection,
            cancel_token: CancellationToken::new(),
        };

        this
    }

    pub fn send_rtcp_pli(&self, media_ssrc: u32) {
        let pc2 = Arc::downgrade(&self.peer_connection);
        let cancel = self.cancel_token.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("[RTCP] Send PLI cancelled");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(3)) => {
                        if let Some(pc) = pc2.upgrade() {
                            if let Err(e) = pc.write_rtcp(&[Box::new(PictureLossIndication {
                                sender_ssrc: 0,
                                media_ssrc,
                            })]).await {
                                warn!("[PLI Sender] Error sending PLI: {:?}", e);
                            }
                        } else {
                            break;
                        }
                    }
                }
            }
        });
    }

    pub async fn close(&self) {
        self.cancel_token.cancel();
        let _ = self.peer_connection.close().await;

        // Stop media
        let media = self.media.lock().await;
        media.stop();
    }

    pub async fn calculate_remb(&self) {
        let receivers = self.peer_connection.get_receivers().await;

        for receiver in receivers {
            let result = receiver.read_rtcp().await;
            if let Ok((packets, _attrs)) = result {
                for pkt in packets {
                    if let Some(remb) = pkt
                        .as_any()
                        .downcast_ref::<ReceiverEstimatedMaximumBitrate>()
                    {
                        println!("📡 Received REMB: bitrate = {}", remb.bitrate);
                        println!("   SSRCs: {:?}", remb.ssrcs);
                        println!("   Sender SSRC: {}", remb.sender_ssrc);
                    }
                }
            } else if let Err(err) = result {
                eprintln!("Failed to read RTCP: {:?}", err);
            }
        }
    }
}
