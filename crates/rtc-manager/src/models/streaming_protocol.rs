#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingProtocol {
    SFU = 0,
    HLS = 1,
    MOQ = 2,
}

impl From<u8> for StreamingProtocol {
    fn from(value: u8) -> Self {
        match value {
            0 => StreamingProtocol::SFU,
            1 => StreamingProtocol::HLS,
            2 => StreamingProtocol::MOQ,
            _ => StreamingProtocol::SFU, // Default to SFU
        }
    }
}

impl From<StreamingProtocol> for u8 {
    fn from(protocol: StreamingProtocol) -> Self {
        protocol as u8
    }
}

impl From<i32> for StreamingProtocol {
    fn from(value: i32) -> Self {
        match value {
            0 => StreamingProtocol::SFU,
            1 => StreamingProtocol::HLS,
            2 => StreamingProtocol::MOQ,
            _ => StreamingProtocol::SFU, // Default to SFU
        }
    }
}

impl From<StreamingProtocol> for i32 {
    fn from(protocol: StreamingProtocol) -> Self {
        protocol as i32
    }
}
