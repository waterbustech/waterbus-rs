use serde_repr::{Deserialize_repr, Serialize_repr};

#[repr(u8)]
#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr, PartialEq, Eq)]
pub enum StreamingProtocol {
    SFU = 0,
    HLS = 1,
    MOQ = 2,
}

impl From<u8> for StreamingProtocol {
    fn from(val: u8) -> Self {
        match val {
            1 => StreamingProtocol::HLS,
            2 => StreamingProtocol::MOQ,
            _ => StreamingProtocol::SFU,
        }
    }
}

impl From<StreamingProtocol> for u8 {
    fn from(sp: StreamingProtocol) -> Self {
        sp as u8
    }
}
