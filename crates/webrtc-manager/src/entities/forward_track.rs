use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

use kanal::{AsyncReceiver, ReceiveError, Receiver, Sender};
use tracing::{debug, warn};
use str0m::media::{MediaKind, Direction, Mid};

use crate::{
    models::{quality::TrackQuality, rtp_foward_info::RtpForwardInfo},
    utils::multicast_sender::MulticastSender,
    errors::WebRTCError,
};

/// ForwardTrack is a track that forwards media data to subscribers using str0m
pub struct ForwardTrack<M: MulticastSender> {
    pub track_id: String,
    pub participant_id: String,
    pub subscriber_id: String,
    pub kind: MediaKind,
    requested_quality: Arc<AtomicU8>,
    effective_quality: Arc<AtomicU8>,
    current_subscribed_quality: Arc<AtomicU8>,
    multicast: Arc<M>,
    quality_change_sender: Sender<TrackQuality>,
}

impl<M: MulticastSender + 'static> ForwardTrack<M> {
    /// Create a new ForwardTrack for str0m
    pub fn new(
        track_id: String,
        participant_id: String,
        subscriber_id: String,
        kind: MediaKind,
        multicast: Arc<M>,
    ) -> Result<Self, WebRTCError> {
        let (quality_sender, _quality_receiver) = kanal::unbounded();

        let forward_track = Self {
            track_id: track_id.clone(),
            participant_id,
            subscriber_id: subscriber_id.clone(),
            kind,
            requested_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            effective_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            current_subscribed_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            multicast,
            quality_change_sender: quality_sender,
        };

        Ok(forward_track)
    }

    /// Set the requested quality for this forward track
    pub async fn set_quality(&self, quality: TrackQuality) {
        let old_quality = TrackQuality::from_u8(self.requested_quality.load(Ordering::Relaxed));
        
        if old_quality != quality {
            self.requested_quality.store(quality.as_u8(), Ordering::Relaxed);
            
            // Notify about quality change
            if let Err(e) = self.quality_change_sender.send(quality).await {
                warn!("Failed to send quality change notification: {:?}", e);
            }
            
            debug!(
                "Quality change for track {} (subscriber {}): {:?} -> {:?}",
                self.track_id, self.subscriber_id, old_quality, quality
            );
        }
    }

    /// Set the effective quality (what's actually being sent)
    pub fn set_effective_quality(&self, quality: &TrackQuality) {
        self.effective_quality.store(quality.as_u8(), Ordering::Relaxed);
    }

    /// Get the requested quality
    pub fn get_requested_quality(&self) -> TrackQuality {
        TrackQuality::from_u8(self.requested_quality.load(Ordering::Relaxed))
    }

    /// Get the effective quality
    pub fn get_effective_quality(&self) -> TrackQuality {
        TrackQuality::from_u8(self.effective_quality.load(Ordering::Relaxed))
    }

    /// Send media data to this forward track
    pub fn send_data(&self, data: &[u8], timestamp: u64) -> Result<(), WebRTCError> {
        // In str0m, we would forward the data through the media writer
        // This is a simplified implementation
        debug!(
            "Forwarding {} bytes of {} data for track {} to subscriber {}",
            data.len(),
            match self.kind {
                MediaKind::Audio => "audio",
                MediaKind::Video => "video",
            },
            self.track_id,
            self.subscriber_id
        );
        
        Ok(())
    }

    /// Request a keyframe for video tracks
    pub fn request_keyframe(&self) -> Result<(), WebRTCError> {
        if self.kind == MediaKind::Video {
            debug!("Keyframe requested for video track {} (subscriber {})", 
                   self.track_id, self.subscriber_id);
            // In str0m, this would be handled through the media writer
        }
        Ok(())
    }

    /// Check if this forward track is for video
    pub fn is_video(&self) -> bool {
        self.kind == MediaKind::Video
    }

    /// Check if this forward track is for audio  
    pub fn is_audio(&self) -> bool {
        self.kind == MediaKind::Audio
    }

    /// Get track ID
    pub fn get_track_id(&self) -> &str {
        &self.track_id
    }

    /// Get participant ID
    pub fn get_participant_id(&self) -> &str {
        &self.participant_id
    }

    /// Get subscriber ID
    pub fn get_subscriber_id(&self) -> &str {
        &self.subscriber_id
    }

    /// Get media kind
    pub fn get_kind(&self) -> MediaKind {
        self.kind
    }

    /// Stop the forward track
    pub fn stop(&self) {
        debug!("Stopping forward track {} for subscriber {}", 
               self.track_id, self.subscriber_id);
    }

    /// Check if the forward track is active
    pub fn is_active(&self) -> bool {
        // Simple check - in a real implementation, this might check if the RTC connection is alive
        true
    }

    /// Update quality based on network conditions
    pub async fn update_quality_based_on_network(&self, packet_loss: f32, rtt: u32) {
        let current_quality = self.get_requested_quality();
        
        let new_quality = if packet_loss > 0.05 || rtt > 200 {
            // High packet loss or RTT - reduce quality
            match current_quality {
                TrackQuality::High => TrackQuality::Medium,
                TrackQuality::Medium => TrackQuality::Low,
                TrackQuality::Low => TrackQuality::Low,
            }
        } else if packet_loss < 0.01 && rtt < 50 {
            // Good network conditions - increase quality
            match current_quality {
                TrackQuality::Low => TrackQuality::Medium,
                TrackQuality::Medium => TrackQuality::High,
                TrackQuality::High => TrackQuality::High,
            }
        } else {
            current_quality
        };

        if new_quality != current_quality {
            self.set_quality(new_quality).await;
        }
    }
}
