#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum TrackQuality {
    Low = 0,
    Medium = 1,
    High = 2,
}

impl TrackQuality {
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => TrackQuality::Low,
            1 => TrackQuality::Medium,
            2 => TrackQuality::High,
            _ => TrackQuality::Medium, // Default to Medium
        }
    }

    pub fn from_str(s: &str) -> Result<Self, &'static str> {
        match s {
            "low" | "l" => Ok(TrackQuality::Low),
            "medium" | "m" => Ok(TrackQuality::Medium),
            "high" | "h" => Ok(TrackQuality::High),
            _ => Err("Invalid quality string"),
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            TrackQuality::Low => "low",
            TrackQuality::Medium => "medium",
            TrackQuality::High => "high",
        }
    }
}

impl Default for TrackQuality {
    fn default() -> Self {
        TrackQuality::Medium
    }
}
