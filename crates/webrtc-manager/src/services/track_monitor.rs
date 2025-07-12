use std::{collections::HashMap, str::FromStr, sync::atomic::Ordering};

use crate::{entities::track::Track, models::quality::TrackQuality};

#[derive(Debug, Clone)]
pub struct TrackSubscribed {
    pub is_simulcast: bool,
    pub layers: HashMap<TrackQuality, LayerInfo>,
}

#[derive(Debug, Clone)]
pub struct LayerInfo {
    pub quality: TrackQuality,
    pub subscriber_count: usize,
    pub subscribers: Vec<String>, // forward_track_ids
}

impl Track {
    /// Get information about which simulcast layers are being subscribed to
    pub fn get_track_subscribed(&self) -> TrackSubscribed {
        let is_simulcast = self.is_simulcast.load(Ordering::Relaxed);
        let mut layers: HashMap<TrackQuality, LayerInfo> = HashMap::new();

        if !is_simulcast {
            // If not simulcast, just return empty layers
            return TrackSubscribed {
                is_simulcast: false,
                layers,
            };
        }

        // Initialize all possible simulcast layers (f=Low, h=Medium, q=High)
        for quality in [TrackQuality::Low, TrackQuality::Medium, TrackQuality::High] {
            layers.insert(
                quality.clone(),
                LayerInfo {
                    quality: quality.clone(),
                    subscriber_count: 0,
                    subscribers: Vec::new(),
                },
            );
        }

        // Check each forward track to see what quality they desire
        for entry in self.forward_tracks.iter() {
            let forward_track_id = entry.key().clone();
            let forward_track = entry.value();

            let desired_quality = forward_track.get_desired_quality();

            // Skip if no quality is desired
            if desired_quality == TrackQuality::None {
                continue;
            }

            // Find which actual layer would be served based on acceptable_map
            let actual_served_quality = self.get_actual_served_quality(&desired_quality);

            if let Some(layer_info) = layers.get_mut(&actual_served_quality) {
                layer_info.subscriber_count += 1;
                layer_info.subscribers.push(forward_track_id);
            }
        }

        TrackSubscribed {
            is_simulcast: true,
            layers,
        }
    }

    /// Helper function to determine which actual layer would be served
    /// based on the acceptable_map and desired quality
    fn get_actual_served_quality(&self, desired_quality: &TrackQuality) -> TrackQuality {
        // Get available qualities from remote tracks
        let available_qualities: Vec<TrackQuality> = self
            .remote_tracks
            .iter()
            .map(|track| TrackQuality::from_str(track.rid()).unwrap())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // If the desired quality is available, return it
        if available_qualities.contains(desired_quality) {
            return desired_quality.clone();
        }

        // Otherwise, use fallback logic (similar to rebuild_acceptable_map)
        match desired_quality {
            TrackQuality::High => {
                // Try Medium first, then Low
                if available_qualities.contains(&TrackQuality::Medium) {
                    TrackQuality::Medium
                } else {
                    TrackQuality::Low // Default fallback
                }
            }
            TrackQuality::Medium => {
                // Try Low first, then High
                if available_qualities.contains(&TrackQuality::Low) {
                    TrackQuality::Low
                } else if available_qualities.contains(&TrackQuality::High) {
                    TrackQuality::High
                } else {
                    TrackQuality::Low // Default fallback
                }
            }
            TrackQuality::Low => {
                // Try Medium first, then High
                if available_qualities.contains(&TrackQuality::Medium) {
                    TrackQuality::Medium
                } else if available_qualities.contains(&TrackQuality::High) {
                    TrackQuality::High
                } else {
                    TrackQuality::Low // Default fallback
                }
            }
            _ => TrackQuality::Low, // Default for any other case
        }
    }

    /// Get a summary of subscriber counts for each layer
    pub fn get_layer_subscriber_summary(&self) -> HashMap<TrackQuality, usize> {
        let track_subscribed = self.get_track_subscribed();
        track_subscribed
            .layers
            .into_iter()
            .map(|(quality, info)| (quality, info.subscriber_count))
            .collect()
    }

    /// Check if any subscribers exist for simulcast layers
    pub fn has_simulcast_subscribers(&self) -> bool {
        if !self.is_simulcast.load(Ordering::Relaxed) {
            return false;
        }

        let track_subscribed = self.get_track_subscribed();
        track_subscribed
            .layers
            .values()
            .any(|info| info.subscriber_count > 0)
    }

    /// Get the most demanded quality layer
    pub fn get_most_demanded_quality(&self) -> Option<TrackQuality> {
        let track_subscribed = self.get_track_subscribed();

        track_subscribed
            .layers
            .into_iter()
            .filter(|(_, info)| info.subscriber_count > 0)
            .max_by_key(|(_, info)| info.subscriber_count)
            .map(|(quality, _)| quality)
    }
}
