#![allow(unused)]

use crate::core::dtos::meeting::create_meeting_dto::CreateMeetingDto;
use crate::core::dtos::meeting::update_meeting_dto::{self, UpdateMeetingDto};
use crate::core::entities::models::{
    Meeting, MeetingsStatusEnum, MembersRoleEnum, MembersStatusEnum, NewMeeting, NewMember,
};
use crate::core::types::errors::meeting_error::MeetingError;
use crate::core::types::res::meeting_response::MeetingResponse;
use crate::core::utils::bcrypt_utils::hash_password;
use crate::core::utils::id_utils::generate_meeting_code;
use crate::features::meeting::repository::{MeetingRepository, MeetingRepositoryImpl};
use chrono::Utc;
use salvo::async_trait;

#[async_trait]
pub trait MeetingService {
    async fn create_meeting(
        &self,
        data: CreateMeetingDto,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError>;

    async fn update_meeting(
        &self,
        data: UpdateMeetingDto,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError>;

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError>;

    async fn get_meeting_by_code(&self, meeting_code: i32)
    -> Result<MeetingResponse, MeetingError>;

    async fn leave_meeting(&self, meeting_code: i32, user_id: i32)
    -> Result<Meeting, MeetingError>;

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
    ) -> Result<MeetingResponse, MeetingError> {
        let create_meeting_dto = data.clone();
        let now = Utc::now().naive_utc();

        let password_hashed = hash_password(&data.password);

        let new_meeting = NewMeeting {
            title: &*data.title,
            password: &password_hashed,
            code: &generate_meeting_code(),
            status: MeetingsStatusEnum::Active as i32,
            createdAt: now,
            updatedAt: now,
        };

        let mut new_meeting = self.repository.create_meeting(new_meeting).await.unwrap();

        let new_member = NewMember {
            meetingId: &new_meeting.id,
            userId: Some(user_id),
            status: MembersStatusEnum::Joined as i32,
            role: MembersRoleEnum::Host as i32,
            createdAt: now,
        };

        let new_member = self.repository.create_member(new_member).await.unwrap();

        new_meeting.members = Vec::from([new_member]);

        Ok(new_meeting)
    }

    async fn update_meeting(
        &self,
        data: UpdateMeetingDto,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let update_meeting_dto = data.clone();
        let meeting = self
            .repository
            .get_meeting_by_code(update_meeting_dto.code)
            .await
            .unwrap();

        // Check whether user_id is host or not
        let is_host = meeting.members.iter().any(|member| {
            member.userId == Some(user_id) && member.role == MembersRoleEnum::Host as i32
        });

        if !is_host {
            return Err(MeetingError::YouDontHavePermissions);
        }

        // Update new meeting metadata
        let mut meeting = Meeting {
            id: meeting.id,
            title: meeting.title,
            password: meeting.password,
            avatar: meeting.avatar,
            status: meeting.status,
            latestMessageCreatedAt: meeting.latest_message_created_at,
            code: meeting.code,
            createdAt: meeting.created_at,
            updatedAt: meeting.updated_at,
            deletedAt: meeting.deleted_at,
            latestMessageId: meeting.latest_message.as_ref().map(|msg| msg.id),
        };

        if let Some(title) = update_meeting_dto.title {
            meeting.title = title;
        }

        if let Some(password) = update_meeting_dto.password {
            let password_hashed = hash_password(&password);
            meeting.password = password_hashed;
        }

        if let Some(avatar) = update_meeting_dto.avatar {
            meeting.avatar = Some(avatar);
        }

        let updated_meeting = self.repository.update_meeting(meeting).await?;

        Ok(updated_meeting)
    }

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError> {
        let meeting = self.repository.get_meeting_by_id(meeting_id).await?;

        Ok(meeting)
    }

    async fn get_meeting_by_code(
        &self,
        meeting_code: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        println!("code: {}", meeting_code);
        let meeting = self.repository.get_meeting_by_code(meeting_code).await?;

        println!("meeting: {:?}", meeting);

        Ok(meeting)
    }

    async fn leave_meeting(
        &self,
        meeting_code: i32,
        user_id: i32,
    ) -> Result<Meeting, MeetingError> {
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
