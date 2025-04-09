use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]

pub struct ParticipantHasLeftResponse {
    pub target_id: String,
}
