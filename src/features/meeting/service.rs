#![allow(unused)]

use crate::core::dtos::meeting::create_meeting_dto::CreateMeetingDto;
use crate::core::dtos::meeting::update_meeting_dto::UpdateMeetingDto;
use crate::core::entities::models::{
    Meeting, MembersRoleEnum, MembersStatusEnum, NewMeeting, NewMember,
};
use crate::core::types::errors::meeting_error::MeetingError;
use crate::core::types::res::meeting_response::MeetingResponse;
use crate::features::meeting::repository::{MeetingRepository, MeetingRepositoryImpl};
use chrono::Utc;
use salvo::async_trait;

#[async_trait]
pub trait MeetingService {
    async fn create_meeting(
        &self,
        data: CreateMeetingDto,
        user_id: i32,
    ) -> Result<Meeting, MeetingError>;

    async fn update_meeting(
        &self,
        data: UpdateMeetingDto,
        user_id: i32,
    ) -> Result<Meeting, MeetingError>;

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError>;

    async fn get_meeting(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError>;

    async fn leave_meeting(&self, meeting_id: i32, user_id: i32) -> Result<Meeting, MeetingError>;

    async fn join_meeting_without_password(
        &self,
        join_meeting_id: i32,
    ) -> Result<Meeting, MeetingError>;

    async fn join_with_password(
        &self,
        join_meeting_id: i32,
        password: &str,
    ) -> Result<Meeting, MeetingError>;

    async fn add_member(&self, host_id: i32, user_id: i32) -> Result<Meeting, MeetingError>;

    async fn remove_member(&self, host_id: i32, user_id: i32) -> Result<Meeting, MeetingError>;

    async fn accept_invitation(
        &self,
        meeting_id: i32,
        user_id: i32,
    ) -> Result<Meeting, MeetingError>;

    async fn archived_meeting(
        &self,
        meeting_id: i32,
        user_id: i32,
    ) -> Result<Meeting, MeetingError>;
}

pub struct MeetingServiceImpl {
    repository: MeetingRepositoryImpl,
}

impl MeetingServiceImpl {
    pub fn new(repository: MeetingRepositoryImpl) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl MeetingService for MeetingServiceImpl {
    async fn create_meeting(
        &self,
        data: CreateMeetingDto,
        user_id: i32,
    ) -> Result<Meeting, MeetingError> {
        let create_meeting_dto = data.clone();
        let now = Utc::now().naive_utc();

        let new_meeting = NewMeeting {
            title: &*data.title,
            password: &*data.password,
            createdAt: now,
            updatedAt: now,
        };

        let new_meeting = self.repository.create_meeting(new_meeting).await.unwrap();

        let new_member = NewMember {
            meetingId: &new_meeting.id,
            userId: Some(user_id),
            status: MembersStatusEnum::Joined,
            role: MembersRoleEnum::Host,
            createdAt: now,
        };

        let new_member = self.repository.create_member(new_member).await.unwrap();

        todo!()
    }

    async fn update_meeting(
        &self,
        data: UpdateMeetingDto,
        user_id: i32,
    ) -> Result<Meeting, MeetingError> {
        let update_meeting_dto = data.clone();
        let meeting = self
            .repository
            .get_meeting_by_code(update_meeting_dto.code)
            .await
            .unwrap();

        // Check whether user_id is host or not

        // Update new meeting metadata

        // let member = meeting
        todo!()
    }

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError> {
        let meeting = self.repository.get_meeting_by_id(meeting_id).await?;

        Ok(meeting)
    }

    async fn get_meeting(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError> {
        let meeting = self.repository.get_meeting_by_id(meeting_id).await?;

        Ok(meeting)
    }

    async fn leave_meeting(&self, meeting_id: i32, user_id: i32) -> Result<Meeting, MeetingError> {
        todo!()
    }

    async fn join_meeting_without_password(
        &self,
        join_meeting_id: i32,
    ) -> Result<Meeting, MeetingError> {
        todo!()
    }

    async fn join_with_password(
        &self,
        join_meeting_id: i32,
        password: &str,
    ) -> Result<Meeting, MeetingError> {
        todo!()
    }

    async fn add_member(&self, host_id: i32, user_id: i32) -> Result<Meeting, MeetingError> {
        todo!()
    }

    async fn remove_member(&self, host_id: i32, user_id: i32) -> Result<Meeting, MeetingError> {
        todo!()
    }

    async fn accept_invitation(
        &self,
        meeting_id: i32,
        user_id: i32,
    ) -> Result<Meeting, MeetingError> {
        todo!()
    }

    async fn archived_meeting(
        &self,
        meeting_id: i32,
        user_id: i32,
    ) -> Result<Meeting, MeetingError> {
        todo!()
    }
}
