use serde::{Deserialize, Deserializer};

use crate::models::quality::TrackQuality;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackQualityRequest {
    pub track_id: String,
    #[serde(deserialize_with = "deserialize_quality")]
    pub quality: TrackQuality,
}

fn deserialize_quality<'de, D>(deserializer: D) -> Result<TrackQuality, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u8::deserialize(deserializer)?;
    Ok(TrackQuality::from_u8(value))
}
