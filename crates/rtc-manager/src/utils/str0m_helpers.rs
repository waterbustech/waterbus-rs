use std::net::SocketAddr;
use str0m::{Rtc, Candidate, net::Protocol};
use crate::{errors::RtcError, models::params::RtcManagerConfigs};

pub struct Str0mHelper;

impl Str0mHelper {
    /// Create a new str0m RTC instance with the given configuration
    pub fn create_rtc_instance(configs: &RtcManagerConfigs) -> Result<Rtc, RtcError> {
        // Configure ICE settings
        if !configs.public_ip.is_empty() {
            // TODO: Set up NAT 1:1 mapping if needed
            tracing::debug!("Using public IP: {}", configs.public_ip);
        }

        let rtc = Rtc::builder().build();
        
        Ok(rtc)
    }

    /// Add local candidates to the RTC instance based on configuration
    pub fn add_local_candidates(rtc: &mut Rtc, configs: &RtcManagerConfigs) -> Result<(), RtcError> {
        // Add UDP candidates for the port range
        for port in configs.port_min..=configs.port_max {
            let addr = if configs.public_ip.is_empty() {
                format!("0.0.0.0:{}", port)
            } else {
                format!("{}:{}", configs.public_ip, port)
            };
            
            if let Ok(socket_addr) = addr.parse::<SocketAddr>() {
                if let Ok(candidate) = Candidate::host(socket_addr, Protocol::Udp) {
                    rtc.add_local_candidate(candidate);
                }
            }
        }
        
        Ok(())
    }

    /// Convert a WebRTC SDP string to str0m format
    pub fn parse_sdp_offer(sdp: &str) -> Result<str0m::change::SdpOffer, RtcError> {
        str0m::change::SdpOffer::from_sdp_string(sdp)
            .map_err(|_| RtcError::FailedToSetSdp)
    }

    /// Convert str0m SDP answer to string format
    pub fn serialize_sdp_answer(answer: &str0m::change::SdpAnswer) -> String {
        answer.to_sdp_string()
    }

    /// Convert str0m SDP offer to string format
    pub fn serialize_sdp_offer(offer: &str0m::change::SdpOffer) -> String {
        offer.to_sdp_string()
    }

    /// Create a basic media configuration for str0m
    pub fn create_media_config(
        enable_video: bool,
        enable_audio: bool,
    ) -> Vec<(str0m::media::MediaKind, str0m::media::Direction)> {
        let mut media_configs = Vec::new();
        
        if enable_video {
            media_configs.push((
                str0m::media::MediaKind::Video,
                str0m::media::Direction::SendRecv,
            ));
        }
        
        if enable_audio {
            media_configs.push((
                str0m::media::MediaKind::Audio,
                str0m::media::Direction::SendRecv,
            ));
        }
        
        media_configs
    }

    /// Extract media information from str0m media event
    pub fn extract_media_info(media: &str0m::media::Media) -> (String, str0m::media::MediaKind) {
        let mid = media.mid().to_string();
        let kind = media.kind();
        (mid, kind)
    }

    /// Check if str0m RTC instance is connected
    pub fn is_connected(_rtc: &Rtc) -> bool {
        // TODO: Implement connection state check
        // This would require accessing internal state of str0m
        true // Placeholder
    }

    /// Get connection statistics from str0m
    pub fn get_connection_stats(_rtc: &Rtc) -> ConnectionStats {
        // TODO: Extract actual stats from str0m
        ConnectionStats::default()
    }
}

#[derive(Debug, Default)]
pub struct ConnectionStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub round_trip_time: f64,
    pub jitter: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_rtc_instance() {
        let configs = RtcManagerConfigs {
            public_ip: "127.0.0.1".to_string(),
            port_min: 10000,
            port_max: 10010,
        };

        let result = Str0mHelper::create_rtc_instance(&configs);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_media_config() {
        let config = Str0mHelper::create_media_config(true, true);
        assert_eq!(config.len(), 2);
        
        let config = Str0mHelper::create_media_config(true, false);
        assert_eq!(config.len(), 1);
        
        let config = Str0mHelper::create_media_config(false, false);
        assert_eq!(config.len(), 0);
    }
}
