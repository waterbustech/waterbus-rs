#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ConnectionType {
    P2P = 0,
    SFU = 1,
}

impl From<u8> for ConnectionType {
    fn from(val: u8) -> Self {
        match val {
            1 => ConnectionType::SFU,
            _ => ConnectionType::P2P,
        }
    }
}

impl From<ConnectionType> for u8 {
    fn from(ct: ConnectionType) -> Self {
        ct as u8
    }
}
