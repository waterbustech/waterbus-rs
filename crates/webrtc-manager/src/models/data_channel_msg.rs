use crate::models::quality::TrackQuality;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackSubscribedMessage {
    pub track_id: String,
    pub subscribed_count: u32,
    #[serde(
        serialize_with = "serialize_quality",
        deserialize_with = "deserialize_quality"
    )]
    pub quality: Option<TrackQuality>,
    pub timestamp: u64,
}

fn serialize_quality<S>(quality: &Option<TrackQuality>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mapped = quality.as_ref().map(|q| q.as_u8());
    serializer.serialize_some(&mapped)
}

fn deserialize_quality<'de, D>(deserializer: D) -> Result<Option<TrackQuality>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<u8>::deserialize(deserializer)?;
    Ok(opt.map(TrackQuality::from_u8))
}
