use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use webrtc::{
    peer_connection::RTCPeerConnection,
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
};

use crate::models::connection_type::ConnectionType;

use super::media::Media;

#[derive(Debug)]
pub struct Publisher {
    pub media: Arc<RwLock<Media>>,
    pub peer_connection: Arc<RTCPeerConnection>,
    pub connection_type: AtomicU8,
    cancel_token: CancellationToken,
}

impl Publisher {
    pub fn new(
        media: Arc<RwLock<Media>>,
        peer_connection: Arc<RTCPeerConnection>,
        connection_type: ConnectionType,
    ) -> Self {
        let this = Self {
            media,
            peer_connection,
            connection_type: AtomicU8::new(connection_type.into()),
            cancel_token: CancellationToken::new(),
        };

        this
    }

    pub fn send_rtcp_pli(&self, media_ssrc: u32) {
        let pc2 = Arc::downgrade(&self.peer_connection);
        let cancel = self.cancel_token.clone();

        tokio::spawn(async move {
            let mut result = Result::<usize, anyhow::Error>::Ok(0);
            while result.is_ok() {
                let timeout = tokio::time::sleep(Duration::from_secs(1));
                tokio::pin!(timeout);

                tokio::select! {
                    _ = cancel.cancelled() => {
                        drop(pc2);
                        break;
                    }
                    _ = timeout.as_mut() =>{
                        if let Some(pc) = pc2.upgrade(){
                            result = pc.write_rtcp(&[Box::new(PictureLossIndication{
                                sender_ssrc: 0,
                                media_ssrc,
                            })]).await.map_err(Into::into);
                        }else{
                            break;
                        }
                    }
                };
            }
        });
    }

    pub fn set_connection_type(&self, connection_type: ConnectionType) {
        self.connection_type
            .store(connection_type.into(), Ordering::Relaxed);
    }

    pub fn get_connection_type(&self) -> ConnectionType {
        self.connection_type.load(Ordering::Relaxed).into()
    }

    pub fn close(&self) {
        let pc = self.peer_connection.clone();
        let media = self.media.clone();
        self.cancel_token.cancel();

        tokio::spawn(async move {
            let _ = pc.close().await;

            drop(pc);

            // Stop media
            let media = media.write().await;
            media.stop();
        });
    }
}
