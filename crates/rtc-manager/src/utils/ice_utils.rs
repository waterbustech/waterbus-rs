use std::net::SocketAddr;
use str0m::{Candidate, net::Protocol};
use crate::{errors::RtcError, models::rtc_dto::IceCandidate};

pub struct IceUtils;

impl IceUtils {
    /// Convert WebRTC IceCandidate to str0m Candidate
    pub fn convert_to_str0m_candidate(ice_candidate: &IceCandidate) -> Result<Candidate, RtcError> {
        // Parse the candidate string
        // Format: "candidate:foundation component protocol priority address port typ type ..."
        let parts: Vec<&str> = ice_candidate.candidate.split_whitespace().collect();
        
        if parts.len() < 8 {
            return Err(RtcError::InvalidIceCandidate);
        }

        // Extract address and port
        let address = parts[4];
        let port: u16 = parts[5].parse()
            .map_err(|_| RtcError::InvalidIceCandidate)?;
        
        let socket_addr: SocketAddr = format!("{}:{}", address, port)
            .parse()
            .map_err(|_| RtcError::InvalidIceCandidate)?;

        // Extract protocol
        let protocol = match parts[2].to_lowercase().as_str() {
            "udp" => Protocol::Udp,
            "tcp" => Protocol::Tcp,
            _ => Protocol::Udp, // Default to UDP
        };

        // Extract candidate type
        let candidate_type = if parts.len() > 7 {
            parts[7]
        } else {
            "host"
        };

        // Create str0m candidate based on type
        match candidate_type {
            "host" => Candidate::host(socket_addr, protocol),
            "srflx" => {
                // Server reflexive candidate
                // For now, treat as host candidate
                Candidate::host(socket_addr, protocol)
            }
            "relay" => {
                // Relay candidate (TURN)
                // For now, treat as host candidate
                Candidate::host(socket_addr, protocol)
            }
            _ => Candidate::host(socket_addr, protocol),
        }
        .map_err(|_| RtcError::InvalidIceCandidate)
    }

    /// Convert str0m Candidate to WebRTC IceCandidate
    pub fn convert_from_str0m_candidate(candidate: &Candidate, sdp_mid: Option<String>, sdp_m_line_index: Option<u16>) -> IceCandidate {
        // This is a simplified conversion
        // In practice, you'd need to extract more information from the str0m candidate
        let candidate_string = format!(
            "candidate:1 1 {} {} {} {} typ host",
            match candidate.proto() {
                Protocol::Udp => "UDP",
                Protocol::Tcp => "TCP",
                Protocol::SslTcp => "TCP", // Treat as TCP
                Protocol::Tls => "TCP",    // Treat as TCP
            },
            1000, // Priority (simplified)
            candidate.addr().ip(),
            candidate.addr().port()
        );

        IceCandidate {
            candidate: candidate_string,
            sdp_mid,
            sdp_m_line_index,
        }
    }

    /// Parse ICE candidate string and extract information
    pub fn parse_candidate_string(candidate: &str) -> Result<CandidateInfo, RtcError> {
        let parts: Vec<&str> = candidate.split_whitespace().collect();
        
        if parts.len() < 8 {
            return Err(RtcError::InvalidIceCandidate);
        }

        let foundation = parts[0].to_string();
        let component: u32 = parts[1].parse()
            .map_err(|_| RtcError::InvalidIceCandidate)?;
        let protocol = parts[2].to_string();
        let priority: u32 = parts[3].parse()
            .map_err(|_| RtcError::InvalidIceCandidate)?;
        let address = parts[4].to_string();
        let port: u16 = parts[5].parse()
            .map_err(|_| RtcError::InvalidIceCandidate)?;
        let candidate_type = parts[7].to_string();

        Ok(CandidateInfo {
            foundation,
            component,
            protocol,
            priority,
            address,
            port,
            candidate_type,
        })
    }

    /// Generate a host candidate string
    pub fn generate_host_candidate(address: &str, port: u16, protocol: &str) -> String {
        format!(
            "candidate:1 1 {} 2130706431 {} {} typ host",
            protocol.to_uppercase(),
            address,
            port
        )
    }

    /// Check if candidate is a host candidate
    pub fn is_host_candidate(candidate: &str) -> bool {
        candidate.contains("typ host")
    }

    /// Check if candidate is a server reflexive candidate
    pub fn is_srflx_candidate(candidate: &str) -> bool {
        candidate.contains("typ srflx")
    }

    /// Check if candidate is a relay candidate
    pub fn is_relay_candidate(candidate: &str) -> bool {
        candidate.contains("typ relay")
    }

    /// Extract the transport protocol from candidate
    pub fn get_transport_protocol(candidate: &str) -> Option<String> {
        let parts: Vec<&str> = candidate.split_whitespace().collect();
        if parts.len() >= 3 {
            Some(parts[2].to_lowercase())
        } else {
            None
        }
    }

    /// Get candidate priority
    pub fn get_candidate_priority(candidate: &str) -> Option<u32> {
        let parts: Vec<&str> = candidate.split_whitespace().collect();
        if parts.len() >= 4 {
            parts[3].parse().ok()
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct CandidateInfo {
    pub foundation: String,
    pub component: u32,
    pub protocol: String,
    pub priority: u32,
    pub address: String,
    pub port: u16,
    pub candidate_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_candidate_string() {
        let candidate = "1 1 UDP 2130706431 192.168.1.100 54400 typ host";
        let info = IceUtils::parse_candidate_string(candidate).unwrap();
        
        assert_eq!(info.foundation, "1");
        assert_eq!(info.component, 1);
        assert_eq!(info.protocol, "UDP");
        assert_eq!(info.priority, 2130706431);
        assert_eq!(info.address, "192.168.1.100");
        assert_eq!(info.port, 54400);
        assert_eq!(info.candidate_type, "host");
    }

    #[test]
    fn test_generate_host_candidate() {
        let candidate = IceUtils::generate_host_candidate("192.168.1.100", 54400, "udp");
        assert!(candidate.contains("192.168.1.100"));
        assert!(candidate.contains("54400"));
        assert!(candidate.contains("UDP"));
        assert!(candidate.contains("typ host"));
    }

    #[test]
    fn test_candidate_type_checks() {
        let host_candidate = "candidate:1 1 UDP 2130706431 192.168.1.100 54400 typ host";
        let srflx_candidate = "candidate:2 1 UDP 1694498815 203.0.113.1 54400 typ srflx raddr 192.168.1.100 rport 54400";
        let relay_candidate = "candidate:3 1 UDP 16777215 203.0.113.2 54401 typ relay raddr 203.0.113.1 rport 54400";

        assert!(IceUtils::is_host_candidate(host_candidate));
        assert!(!IceUtils::is_host_candidate(srflx_candidate));
        assert!(!IceUtils::is_host_candidate(relay_candidate));

        assert!(!IceUtils::is_srflx_candidate(host_candidate));
        assert!(IceUtils::is_srflx_candidate(srflx_candidate));
        assert!(!IceUtils::is_srflx_candidate(relay_candidate));

        assert!(!IceUtils::is_relay_candidate(host_candidate));
        assert!(!IceUtils::is_relay_candidate(srflx_candidate));
        assert!(IceUtils::is_relay_candidate(relay_candidate));
    }
}
