#![allow(unused)]

use crate::core::dtos::meeting::create_meeting_dto::CreateMeetingDto;
use crate::core::dtos::meeting::update_meeting_dto::{self, UpdateMeetingDto};
use crate::core::dtos::pagination_dto::{self, PaginationDto};
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

    async fn get_meetings_by_status(
        &self,
        member_status: i32,
        meeting_status: i32,
        user_id: i32,
        pagination_dto: PaginationDto,
    ) -> Result<Vec<MeetingResponse>, MeetingError>;

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
            created_at: now,
            updated_at: now,
        };

        let mut new_meeting = self.repository.create_meeting(new_meeting).await.unwrap();

        let new_member = NewMember {
            meeting_id: &new_meeting.meeting.id,
            user_id: Some(user_id),
            status: MembersStatusEnum::Joined as i32,
            role: MembersRoleEnum::Host as i32,
            created_at: now,
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
            member.member.user_id == Some(user_id)
                && member.member.role == MembersRoleEnum::Host as i32
        });

        if !is_host {
            return Err(MeetingError::YouDontHavePermissions);
        }

        // Update new meeting metadata
        let mut meeting = meeting.meeting;

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

    async fn get_meetings_by_status(
        &self,
        member_status: i32,
        meeting_status: i32,
        user_id: i32,
        pagination_dto: PaginationDto,
    ) -> Result<Vec<MeetingResponse>, MeetingError> {
        let member_status =
            MembersStatusEnum::try_from(member_status).unwrap_or(MembersStatusEnum::Joined);
        let meeting_status =
            MeetingsStatusEnum::try_from(meeting_status).unwrap_or(MeetingsStatusEnum::Active);

        let pagination_dto = pagination_dto.clone();
        let meetings = self
            .repository
            .find_all(
                user_id,
                member_status,
                MeetingsStatusEnum::Active,
                pagination_dto.skip,
                pagination_dto.limit,
            )
            .await?;

        Ok(meetings)
    }

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError> {
        let meeting = self.repository.get_meeting_by_id(meeting_id).await?;

        Ok(meeting)
    }

    async fn get_meeting_by_code(
        &self,
        meeting_code: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let meeting = self.repository.get_meeting_by_code(meeting_code).await?;

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
