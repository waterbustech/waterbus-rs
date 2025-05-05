#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(u8)]
pub enum TrackQuality {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
}

impl TrackQuality {
    pub fn from_str(s: &str) -> TrackQuality {
        match s {
            "q" => TrackQuality::Low,
            "h" => TrackQuality::Medium,
            "f" => TrackQuality::High,
            _ => TrackQuality::None,
        }
    }

    pub fn from_u8(value: u8) -> TrackQuality {
        match value {
            1 => TrackQuality::Low,
            2 => TrackQuality::Medium,
            3 => TrackQuality::High,
            _ => TrackQuality::None,
        }
    }

    pub fn as_u8(&self) -> u8 {
        self.clone() as u8
    }
}
