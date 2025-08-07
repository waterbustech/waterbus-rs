#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    SFU = 0,
    P2P = 1,
}

impl From<u8> for ConnectionType {
    fn from(value: u8) -> Self {
        match value {
            0 => ConnectionType::SFU,
            1 => ConnectionType::P2P,
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
            0 => ConnectionType::SFU,
            1 => ConnectionType::P2P,
            _ => ConnectionType::SFU, // Default to SFU
        }
    }
}

impl From<ConnectionType> for i32 {
    fn from(connection_type: ConnectionType) -> Self {
        connection_type as i32
    }
}
