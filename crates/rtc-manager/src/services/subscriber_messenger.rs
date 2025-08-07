use crate::{
    entities::subscriber::Subscriber,
    models::{
        quality::TrackQuality,
        track_quality_request::TrackQualityRequest,
    },
};

impl Subscriber {
    pub async fn request_quality_change(&self, track_id: String, quality: TrackQuality) {
        // Subscriber requests quality change from its publisher
        // This is handled by the publisher, not the media entity
        
        let _request = TrackQualityRequest::new(track_id.clone(), quality);
        
        tracing::debug!("Subscriber {} requesting quality change for track {}: {:?}", 
                       self.participant_id, track_id, quality);
        
        // Update local preferred quality
        self.set_preferred_quality(quality);
        
        // TODO: Send quality request to publisher
        // In the new architecture, the publisher will handle this request
        // and control the quality sent to this subscriber
    }

    pub async fn handle_track_update(&self, track_id: String, available: bool) {
        if available {
            tracing::debug!("Subscriber {} received track update: {} is now available", 
                           self.participant_id, track_id);
        } else {
            tracing::debug!("Subscriber {} received track update: {} is no longer available", 
                           self.participant_id, track_id);
            
            // Remove track if it's no longer available
            self.remove_track(&track_id);
        }
    }

    pub async fn send_feedback_to_publisher(&self, track_id: String, feedback_type: FeedbackType) {
        tracing::debug!("Subscriber {} sending feedback to publisher for track {}: {:?}", 
                       self.participant_id, track_id, feedback_type);
        
        // TODO: Send feedback to publisher
        // This could be keyframe requests, quality adaptation feedback, etc.
    }
}

#[derive(Debug, Clone)]
pub enum FeedbackType {
    KeyframeRequest,
    QualityDecrease,
    QualityIncrease,
    NetworkCongestion,
    NetworkImprovement,
}
