#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    P2P = 0,
    SFU = 1,
}

impl From<u8> for ConnectionType {
    fn from(value: u8) -> Self {
        match value {
            0 => ConnectionType::P2P,
            1 => ConnectionType::SFU,
            _ => ConnectionType::SFU, // Default to SFU
        }
    }
}

impl From<ConnectionType> for u8 {
    fn from(connection_type: ConnectionType) -> Self {
        connection_type as u8
    }
}

impl From<i32> for ConnectionType {
    fn from(value: i32) -> Self {
        match value {
            0 => ConnectionType::P2P,
            1 => ConnectionType::SFU,
            _ => ConnectionType::SFU, // Default to SFU
        }
    }
}

impl From<ConnectionType> for i32 {
    fn from(connection_type: ConnectionType) -> Self {
        connection_type as i32
    }
}
