use crate::errors::RtcError;

pub struct SdpUtils;

impl SdpUtils {
    /// Parse SDP string and extract basic information
    pub fn parse_sdp_info(sdp: &str) -> Result<SdpInfo, RtcError> {
        let mut info = SdpInfo::default();
        
        for line in sdp.lines() {
            let line = line.trim();
            
            if line.starts_with("v=") {
                info.version = line[2..].parse().unwrap_or(0);
            } else if line.starts_with("o=") {
                // Parse origin line: o=<username> <sess-id> <sess-version> <nettype> <addrtype> <unicast-address>
                let parts: Vec<&str> = line[2..].split_whitespace().collect();
                if parts.len() >= 6 {
                    info.session_id = parts[1].to_string();
                    info.session_version = parts[2].to_string();
                    info.origin_address = parts[5].to_string();
                }
            } else if line.starts_with("s=") {
                info.session_name = line[2..].to_string();
            } else if line.starts_with("c=") {
                // Parse connection line: c=<nettype> <addrtype> <connection-address>
                let parts: Vec<&str> = line[2..].split_whitespace().collect();
                if parts.len() >= 3 {
                    info.connection_address = parts[2].to_string();
                }
            } else if line.starts_with("m=") {
                // Parse media line: m=<media> <port> <proto> <fmt>
                let parts: Vec<&str> = line[2..].split_whitespace().collect();
                if parts.len() >= 4 {
                    let media_type = parts[0].to_string();
                    let port: u16 = parts[1].parse().unwrap_or(0);
                    let protocol = parts[2].to_string();
                    
                    info.media_descriptions.push(MediaDescription {
                        media_type,
                        port,
                        protocol,
                        formats: parts[3..].iter().map(|s| s.to_string()).collect(),
                    });
                }
            } else if line.starts_with("a=ice-ufrag:") {
                info.ice_ufrag = Some(line[12..].to_string());
            } else if line.starts_with("a=ice-pwd:") {
                info.ice_pwd = Some(line[10..].to_string());
            } else if line.starts_with("a=fingerprint:") {
                info.fingerprint = Some(line[14..].to_string());
            }
        }
        
        Ok(info)
    }

    /// Validate SDP format
    pub fn validate_sdp(sdp: &str) -> Result<(), RtcError> {
        if sdp.is_empty() {
            return Err(RtcError::InvalidSdp);
        }

        // Check for required SDP lines
        let has_version = sdp.contains("v=");
        let has_origin = sdp.contains("o=");
        let has_session = sdp.contains("s=");
        
        if !has_version || !has_origin || !has_session {
            return Err(RtcError::InvalidSdp);
        }

        Ok(())
    }

    /// Extract ICE candidates from SDP
    pub fn extract_ice_candidates(sdp: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        
        for line in sdp.lines() {
            let line = line.trim();
            if line.starts_with("a=candidate:") {
                candidates.push(line[12..].to_string());
            }
        }
        
        candidates
    }

    /// Extract media types from SDP
    pub fn extract_media_types(sdp: &str) -> Vec<String> {
        let mut media_types = Vec::new();
        
        for line in sdp.lines() {
            let line = line.trim();
            if line.starts_with("m=") {
                let parts: Vec<&str> = line[2..].split_whitespace().collect();
                if !parts.is_empty() {
                    media_types.push(parts[0].to_string());
                }
            }
        }
        
        media_types
    }

    /// Check if SDP contains video
    pub fn has_video(sdp: &str) -> bool {
        Self::extract_media_types(sdp).contains(&"video".to_string())
    }

    /// Check if SDP contains audio
    pub fn has_audio(sdp: &str) -> bool {
        Self::extract_media_types(sdp).contains(&"audio".to_string())
    }

    /// Modify SDP to enable/disable media types
    pub fn modify_media_enabled(sdp: &str, enable_video: bool, enable_audio: bool) -> String {
        let mut modified_lines = Vec::new();
        let mut skip_video_attributes = false;
        let mut skip_audio_attributes = false;
        
        for line in sdp.lines() {
            let line = line.trim();
            
            if line.starts_with("m=video") {
                if enable_video {
                    modified_lines.push(line.to_string());
                    skip_video_attributes = false;
                } else {
                    // Disable video by setting port to 0
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let disabled_line = format!("m=video 0 {}", parts[2..].join(" "));
                        modified_lines.push(disabled_line);
                    }
                    skip_video_attributes = true;
                }
            } else if line.starts_with("m=audio") {
                if enable_audio {
                    modified_lines.push(line.to_string());
                    skip_audio_attributes = false;
                } else {
                    // Disable audio by setting port to 0
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let disabled_line = format!("m=audio 0 {}", parts[2..].join(" "));
                        modified_lines.push(disabled_line);
                    }
                    skip_audio_attributes = true;
                }
            } else if line.starts_with("m=") {
                // Reset skip flags for other media types
                skip_video_attributes = false;
                skip_audio_attributes = false;
                modified_lines.push(line.to_string());
            } else if skip_video_attributes || skip_audio_attributes {
                // Skip attributes for disabled media
                if line.starts_with("a=") {
                    continue;
                }
                modified_lines.push(line.to_string());
            } else {
                modified_lines.push(line.to_string());
            }
        }
        
        modified_lines.join("\r\n")
    }
}

#[derive(Debug, Default)]
pub struct SdpInfo {
    pub version: u32,
    pub session_id: String,
    pub session_version: String,
    pub session_name: String,
    pub origin_address: String,
    pub connection_address: String,
    pub media_descriptions: Vec<MediaDescription>,
    pub ice_ufrag: Option<String>,
    pub ice_pwd: Option<String>,
    pub fingerprint: Option<String>,
}

#[derive(Debug)]
pub struct MediaDescription {
    pub media_type: String,
    pub port: u16,
    pub protocol: String,
    pub formats: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_sdp() {
        let valid_sdp = "v=0\r\no=- 123 456 IN IP4 127.0.0.1\r\ns=session\r\n";
        assert!(SdpUtils::validate_sdp(valid_sdp).is_ok());

        let invalid_sdp = "invalid";
        assert!(SdpUtils::validate_sdp(invalid_sdp).is_err());
    }

    #[test]
    fn test_extract_media_types() {
        let sdp = "v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\n";
        let media_types = SdpUtils::extract_media_types(sdp);
        assert_eq!(media_types, vec!["video", "audio"]);
    }

    #[test]
    fn test_has_video_audio() {
        let sdp = "v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\n";
        assert!(SdpUtils::has_video(sdp));
        assert!(SdpUtils::has_audio(sdp));

        let audio_only_sdp = "v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\n";
        assert!(!SdpUtils::has_video(audio_only_sdp));
        assert!(SdpUtils::has_audio(audio_only_sdp));
    }
}
