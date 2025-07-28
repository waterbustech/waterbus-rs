use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};

use parking_lot::RwLock;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use webrtc::{
    data_channel::RTCDataChannel, peer_connection::RTCPeerConnection,
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
};

use crate::models::{connection_type::ConnectionType, data_channel_msg::TrackSubscribedMessage};

use super::media::Media;

pub struct Publisher {
    pub media: Arc<RwLock<Media>>,
    pub peer_connection: Arc<RTCPeerConnection>,
    pub connection_type: AtomicU8,
    pub cancel_token: CancellationToken,
    pub data_channel: Option<Arc<RTCDataChannel>>,
    pub track_event_receiver: Option<mpsc::UnboundedReceiver<TrackSubscribedMessage>>,
}

impl Publisher {
    pub async fn new(
        media: Arc<RwLock<Media>>,
        peer_connection: Arc<RTCPeerConnection>,
        connection_type: ConnectionType,
    ) -> Arc<Self> {
        let publisher = Arc::new(Self {
            media,
            peer_connection,
            connection_type: AtomicU8::new(connection_type.into()),
            cancel_token: CancellationToken::new(),
            data_channel: None,
            track_event_receiver: None,
        });

        let publisher_clone = Arc::clone(&publisher);

        let publisher_mut = unsafe {
            let ptr = Arc::as_ptr(&publisher_clone) as *mut Publisher;
            &mut *ptr
        };

        let _ = publisher_mut.create_data_channel().await;
        publisher_mut
            .setup_media_communication(Arc::downgrade(&publisher_clone))
            .await;
        publisher_mut.start_data_channel_handler().await;

        // Set up keyframe request callback
        {
            let publisher_weak = Arc::downgrade(&publisher);
            let mut media = publisher.media.write();
            media.keyframe_request_callback = Some(Arc::new(move |ssrc: u32| {
                if let Some(publisher) = publisher_weak.upgrade() {
                    publisher.send_rtcp_pli_once(ssrc);
                }
            }));
        }

        publisher
    }

    #[inline]
    pub fn send_rtcp_pli(&self, media_ssrc: u32) {
        let pc2 = Arc::downgrade(&self.peer_connection);
        let cancel = self.cancel_token.clone();

        tokio::spawn(async move {
            let mut result = Result::<usize, anyhow::Error>::Ok(0);
            while result.is_ok() {
                let timeout = tokio::time::sleep(Duration::from_secs(3));
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

    #[inline]
    pub fn send_rtcp_pli_once(&self, media_ssrc: u32) {
        let pc2 = Arc::downgrade(&self.peer_connection);

        tokio::spawn(async move {
            if let Some(pc) = pc2.upgrade() {
                let _ = pc
                    .write_rtcp(&[Box::new(PictureLossIndication {
                        sender_ssrc: 0,
                        media_ssrc,
                    })])
                    .await;
            }
        });
    }

    #[inline]
    pub fn set_connection_type(&self, connection_type: ConnectionType) {
        self.connection_type
            .store(connection_type.into(), Ordering::Relaxed);
    }

    #[inline]
    pub fn get_connection_type(&self) -> ConnectionType {
        self.connection_type.load(Ordering::Relaxed).into()
    }

    #[inline]
    pub fn close(&self) {
        let pc = self.peer_connection.clone();
        let media = self.media.clone();
        self.cancel_token.cancel();

        tokio::spawn(async move {
            let _ = pc.close().await;
            drop(pc);
            // Stop media
            let media = media.write();
            media.stop();
        });
    }
}
