use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRoomDto {
    pub sdp: String,
    pub room_id: String,
    pub participant_id: String,
    pub is_video_enabled: bool,
    pub is_audio_enabled: bool,
    pub is_e2ee_enabled: bool,
    pub total_tracks: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeDto {
    pub target_id: String,
    pub room_id: String,
    pub participant_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSubscribeDto {
    pub target_id: String,
    pub sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublisherRenegotiationDto {
    pub sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDto {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_m_line_index: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriberCandidateDto {
    pub target_id: String,
    pub candidate: CandidateDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetEnabledDto {
    pub is_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetScreenSharingDto {
    pub is_sharing: bool,
    pub screen_track_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCameraTypeDto {
    #[serde(rename = "type")]
    pub type_: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetHandRaisingDto {
    pub is_raising: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartWhiteBoardDto {
    pub room_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanWhiteBoardDto {
    pub room_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PaintType {
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "polygon")]
    Polygon,
    #[serde(rename = "circle")]
    Circle,
    #[serde(rename = "square")]
    Square,
    #[serde(rename = "line")]
    Line,
    #[serde(rename = "eraser")]
    Eraser,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OffsetModel {
    pub dx: f64,
    pub dy: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PaintModel {
    pub color: String,
    pub offsets: Vec<OffsetModel>,
    pub width: f64,
    #[serde(rename = "poligonSides")]
    pub polygon_sides: u32,
    #[serde(rename = "isFilled")]
    pub is_filled: bool,
    #[serde(rename = "type")]
    pub paint_type: PaintType,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WhiteBoardAction {
    #[serde(rename = "add")]
    Add,
    #[serde(rename = "remove")]
    Remove,
    #[serde(rename = "update")]
    Update,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateWhiteBoardDto {
    #[serde(rename = "roomId")]
    pub room_id: String,
    pub action: WhiteBoardAction,
    pub paints: Vec<PaintModel>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MsgDto {
    pub my_msg: String,
}
