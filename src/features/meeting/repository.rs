use salvo::async_trait;

use crate::core::{entities::models::Meeting, types::errors::meeting_error::MeetingError};

#[async_trait]
pub trait MeetingRepository: Send + Sync {
    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<Meeting, MeetingError>;

    async fn get_meeting_by_code(&self, meeting_code: i32) -> Result<Meeting, MeetingError>;

    async fn create_meeting(&self, meeting: Meeting) -> Result<Meeting, MeetingError>;

    async fn update_meeting(&self, meeting: Meeting) -> Result<Meeting, MeetingError>;

    async fn delete_meeting_by_id(&self, meeting_id: i32) -> Result<Meeting, MeetingError>;
}
