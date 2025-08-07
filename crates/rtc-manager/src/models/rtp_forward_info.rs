use bytes::Bytes;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct RtpForwardInfo {
    pub data: Bytes,
    pub timestamp: Instant,
    pub ssrc: u32,
    pub sequence_number: u16,
    pub payload_type: u8,
    pub marker: bool,
}

impl RtpForwardInfo {
    pub fn new(
        data: Bytes,
        ssrc: u32,
        sequence_number: u16,
        payload_type: u8,
        marker: bool,
    ) -> Self {
        Self {
            data,
            timestamp: Instant::now(),
            ssrc,
            sequence_number,
            payload_type,
            marker,
        }
    }
}
