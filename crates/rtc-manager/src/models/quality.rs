use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum TrackQuality {
    None = 0,
    Low = 1,
    Medium = 2,
    High = 3,
}

impl FromStr for TrackQuality {
    type Err = ();

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "q" => TrackQuality::Low,
            "h" => TrackQuality::Medium,
            "f" => TrackQuality::High,
            _ => TrackQuality::Medium,
        })
    }
}

impl TrackQuality {
    #[inline]
    pub fn from_u8(value: u8) -> TrackQuality {
        match value {
            1 => TrackQuality::Low,
            2 => TrackQuality::Medium,
            3 => TrackQuality::High,
            _ => TrackQuality::None,
        }
    }

    #[inline]
    pub fn as_u8(&self) -> u8 {
        self.clone() as u8
    }
}
