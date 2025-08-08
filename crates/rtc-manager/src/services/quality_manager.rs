use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::{
    models::quality::TrackQuality,
    errors::RtcError,
};

/// Manages quality adaptation based on Transport-CC feedback
#[derive(Debug, Clone)]
pub struct QualityManager {
    /// Current quality level
    current_quality: Arc<AtomicU8>,
    /// Target quality level based on network conditions
    target_quality: Arc<AtomicU8>,
    /// Network statistics for quality decisions
    network_stats: Arc<RwLock<NetworkConditions>>,
    /// Last quality change timestamp
    last_quality_change: Arc<RwLock<Instant>>,
    /// Minimum time between quality changes
    quality_change_cooldown: Duration,
}

#[derive(Debug, Clone)]
pub struct NetworkConditions {
    /// Available bandwidth in bits per second
    pub available_bandwidth: u64,
    /// Round trip time in milliseconds
    pub rtt_ms: f64,
    /// Packet loss percentage (0.0 - 1.0)
    pub packet_loss: f64,
    /// Jitter in milliseconds
    pub jitter_ms: f64,
    /// Last update timestamp
    pub last_update: Instant,
}

impl Default for NetworkConditions {
    fn default() -> Self {
        Self {
            available_bandwidth: 1_000_000, // 1 Mbps default
            rtt_ms: 50.0,
            packet_loss: 0.0,
            jitter_ms: 10.0,
            last_update: Instant::now(),
        }
    }
}

/// Quality thresholds for different network conditions
#[derive(Debug, Clone)]
pub struct QualityThresholds {
    /// Minimum bandwidth for high quality (bits per second)
    pub high_quality_min_bandwidth: u64,
    /// Minimum bandwidth for medium quality (bits per second)
    pub medium_quality_min_bandwidth: u64,
    /// Maximum RTT for high quality (milliseconds)
    pub high_quality_max_rtt: f64,
    /// Maximum packet loss for high quality (percentage)
    pub high_quality_max_loss: f64,
    /// Maximum RTT for medium quality (milliseconds)
    pub medium_quality_max_rtt: f64,
    /// Maximum packet loss for medium quality (percentage)
    pub medium_quality_max_loss: f64,
}

impl Default for QualityThresholds {
    fn default() -> Self {
        Self {
            high_quality_min_bandwidth: 2_000_000,  // 2 Mbps
            medium_quality_min_bandwidth: 500_000,  // 500 Kbps
            high_quality_max_rtt: 100.0,           // 100ms
            high_quality_max_loss: 0.01,           // 1%
            medium_quality_max_rtt: 200.0,         // 200ms
            medium_quality_max_loss: 0.05,         // 5%
        }
    }
}

impl QualityManager {
    /// Create a new quality manager
    pub fn new() -> Self {
        Self {
            current_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            target_quality: Arc::new(AtomicU8::new(TrackQuality::Medium.as_u8())),
            network_stats: Arc::new(RwLock::new(NetworkConditions::default())),
            last_quality_change: Arc::new(RwLock::new(Instant::now())),
            quality_change_cooldown: Duration::from_secs(2), // 2 second cooldown
        }
    }

    /// Update network conditions from Transport-CC feedback
    pub fn update_network_conditions(&self, conditions: NetworkConditions) {
        {
            let mut stats = self.network_stats.write();
            *stats = conditions;
        }

        // Determine target quality based on network conditions
        let target_quality = self.calculate_target_quality();
        self.target_quality.store(target_quality.as_u8(), Ordering::Relaxed);

        // Apply quality change if needed and cooldown has passed
        self.apply_quality_change_if_needed();
    }

    /// Calculate target quality based on current network conditions
    fn calculate_target_quality(&self) -> TrackQuality {
        let stats = self.network_stats.read();
        let thresholds = QualityThresholds::default();

        // Check if conditions are good enough for high quality
        if stats.available_bandwidth >= thresholds.high_quality_min_bandwidth
            && stats.rtt_ms <= thresholds.high_quality_max_rtt
            && stats.packet_loss <= thresholds.high_quality_max_loss
        {
            return TrackQuality::High;
        }

        // Check if conditions are good enough for medium quality
        if stats.available_bandwidth >= thresholds.medium_quality_min_bandwidth
            && stats.rtt_ms <= thresholds.medium_quality_max_rtt
            && stats.packet_loss <= thresholds.medium_quality_max_loss
        {
            return TrackQuality::Medium;
        }

        // Default to low quality
        TrackQuality::Low
    }

    /// Apply quality change if target differs from current and cooldown has passed
    fn apply_quality_change_if_needed(&self) {
        let current = TrackQuality::from_u8(self.current_quality.load(Ordering::Relaxed));
        let target = TrackQuality::from_u8(self.target_quality.load(Ordering::Relaxed));

        if current != target {
            let now = Instant::now();
            let mut last_change = self.last_quality_change.write();

            if now.duration_since(*last_change) >= self.quality_change_cooldown {
                self.current_quality.store(target.as_u8(), Ordering::Relaxed);
                *last_change = now;

                info!("Quality changed from {:?} to {:?}", current, target);
            } else {
                debug!("Quality change from {:?} to {:?} delayed due to cooldown", current, target);
            }
        }
    }

    /// Get current quality level
    pub fn get_current_quality(&self) -> TrackQuality {
        TrackQuality::from_u8(self.current_quality.load(Ordering::Relaxed))
    }

    /// Get target quality level
    pub fn get_target_quality(&self) -> TrackQuality {
        TrackQuality::from_u8(self.target_quality.load(Ordering::Relaxed))
    }

    /// Get current network conditions
    pub fn get_network_conditions(&self) -> NetworkConditions {
        self.network_stats.read().clone()
    }

    /// Force a quality level (for manual override)
    pub fn set_quality(&self, quality: TrackQuality) {
        self.current_quality.store(quality.as_u8(), Ordering::Relaxed);
        self.target_quality.store(quality.as_u8(), Ordering::Relaxed);
        *self.last_quality_change.write() = Instant::now();

        info!("Quality manually set to {:?}", quality);
    }

    /// Check if quality change is needed
    pub fn needs_quality_change(&self) -> bool {
        let current = self.current_quality.load(Ordering::Relaxed);
        let target = self.target_quality.load(Ordering::Relaxed);
        current != target
    }

    /// Get quality change recommendation
    pub fn get_quality_recommendation(&self) -> Option<QualityChangeRecommendation> {
        if self.needs_quality_change() {
            let current = TrackQuality::from_u8(self.current_quality.load(Ordering::Relaxed));
            let target = TrackQuality::from_u8(self.target_quality.load(Ordering::Relaxed));
            let conditions = self.get_network_conditions();

            Some(QualityChangeRecommendation {
                from_quality: current,
                to_quality: target,
                reason: self.get_change_reason(&conditions),
                network_conditions: conditions,
            })
        } else {
            None
        }
    }

    /// Get the reason for quality change
    fn get_change_reason(&self, conditions: &NetworkConditions) -> QualityChangeReason {
        let thresholds = QualityThresholds::default();

        if conditions.available_bandwidth < thresholds.medium_quality_min_bandwidth {
            QualityChangeReason::LowBandwidth
        } else if conditions.rtt_ms > thresholds.medium_quality_max_rtt {
            QualityChangeReason::HighLatency
        } else if conditions.packet_loss > thresholds.medium_quality_max_loss {
            QualityChangeReason::PacketLoss
        } else if conditions.jitter_ms > 50.0 {
            QualityChangeReason::HighJitter
        } else {
            QualityChangeReason::ImprovedConditions
        }
    }
}

#[derive(Debug, Clone)]
pub struct QualityChangeRecommendation {
    pub from_quality: TrackQuality,
    pub to_quality: TrackQuality,
    pub reason: QualityChangeReason,
    pub network_conditions: NetworkConditions,
}

#[derive(Debug, Clone)]
pub enum QualityChangeReason {
    LowBandwidth,
    HighLatency,
    PacketLoss,
    HighJitter,
    ImprovedConditions,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_manager_creation() {
        let manager = QualityManager::new();
        assert_eq!(manager.get_current_quality(), TrackQuality::Medium);
    }

    #[test]
    fn test_quality_calculation_high_quality() {
        let manager = QualityManager::new();
        
        let conditions = NetworkConditions {
            available_bandwidth: 3_000_000, // 3 Mbps
            rtt_ms: 50.0,
            packet_loss: 0.005, // 0.5%
            jitter_ms: 5.0,
            last_update: Instant::now(),
        };

        manager.update_network_conditions(conditions);
        assert_eq!(manager.get_target_quality(), TrackQuality::High);
    }

    #[test]
    fn test_quality_calculation_low_quality() {
        let manager = QualityManager::new();
        
        let conditions = NetworkConditions {
            available_bandwidth: 200_000, // 200 Kbps
            rtt_ms: 300.0,
            packet_loss: 0.1, // 10%
            jitter_ms: 100.0,
            last_update: Instant::now(),
        };

        manager.update_network_conditions(conditions);
        assert_eq!(manager.get_target_quality(), TrackQuality::Low);
    }
}
