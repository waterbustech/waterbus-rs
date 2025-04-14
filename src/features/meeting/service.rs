#![allow(unused)]

use crate::core::dtos::meeting;
use crate::core::dtos::meeting::create_meeting_dto::CreateMeetingDto;
use crate::core::dtos::meeting::update_meeting_dto::{self, UpdateMeetingDto};
use crate::core::dtos::pagination_dto::{self, PaginationDto};
use crate::core::entities::models::{
    Meeting, MeetingsStatusEnum, MembersRoleEnum, MembersStatusEnum, NewMeeting, NewMember,
    NewParticipant, ParticipantsStatusEnum,
};
use crate::core::types::errors::meeting_error::MeetingError;
use crate::core::types::res::meeting_response::MeetingResponse;
use crate::core::utils::bcrypt_utils::{hash_password, verify_password};
use crate::core::utils::id_utils::generate_meeting_code;
use crate::features::meeting::repository::{MeetingRepository, MeetingRepositoryImpl};
use crate::features::user::repository::{UserRepository, UserRepositoryImpl};
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

    async fn leave_meeting(
        &self,
        meeting_code: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError>;

    async fn join_meeting_without_password(
        &self,
        user_id: i32,
        meeting_code: i32,
    ) -> Result<MeetingResponse, MeetingError>;

    async fn join_with_password(
        &self,
        user_id: i32,
        meeting_code: i32,
        password: &str,
    ) -> Result<MeetingResponse, MeetingError>;

    async fn add_member(
        &self,
        code: i32,
        host_id: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError>;

    async fn remove_member(
        &self,
        code: i32,
        host_id: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError>;

    async fn accept_invitation(
        &self,
        meeting_id: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError>;

    async fn archived_meeting(
        &self,
        meeting_code: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError>;
}

pub struct MeetingServiceImpl {
    meeting_repository: MeetingRepositoryImpl,
    user_repository: UserRepositoryImpl,
}

impl MeetingServiceImpl {
    pub fn new(
        meeting_repository: MeetingRepositoryImpl,
        user_repository: UserRepositoryImpl,
    ) -> Self {
        Self {
            meeting_repository,
            user_repository,
        }
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
            latest_message_created_at: now,
        };

        let mut new_meeting = self
            .meeting_repository
            .create_meeting(new_meeting)
            .await
            .unwrap();

        let new_member = NewMember {
            meeting_id: &new_meeting.meeting.id,
            user_id: Some(user_id),
            status: MembersStatusEnum::Joined as i32,
            role: MembersRoleEnum::Host as i32,
            created_at: now,
        };

        let new_member = self
            .meeting_repository
            .create_member(new_member)
            .await
            .unwrap();

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
            .meeting_repository
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

        let updated_meeting = self.meeting_repository.update_meeting(meeting).await?;

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
            .meeting_repository
            .find_all(
                user_id,
                member_status,
                meeting_status,
                pagination_dto.skip,
                pagination_dto.limit,
            )
            .await?;

        Ok(meetings)
    }

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError> {
        let meeting = self
            .meeting_repository
            .get_meeting_by_id(meeting_id)
            .await?;

        Ok(meeting)
    }

    async fn get_meeting_by_code(
        &self,
        meeting_code: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let meeting = self
            .meeting_repository
            .get_meeting_by_code(meeting_code)
            .await?;

        Ok(meeting)
    }

    async fn leave_meeting(
        &self,
        meeting_code: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let mut meeting = self
            .meeting_repository
            .get_meeting_by_code(meeting_code)
            .await?;

        let index_of_member = meeting
            .members
            .iter()
            .position(|member| member.member.user_id == Some(user_id))
            .ok_or_else(|| MeetingError::UnexpectedError("Member not found".into()))?;

        let member = meeting.members[index_of_member].member.clone();

        if member.role == MembersRoleEnum::Host as i32 {
            return Err(MeetingError::UnexpectedError("Host not allowed to leave the room. You can archive chats if the room no longer active.".into()));
        }

        self.meeting_repository
            .delete_member_by_id(member.id)
            .await?;

        meeting
            .members
            .retain(|member| member.member.user_id != Some(user_id));

        Ok(meeting)
    }

    async fn join_meeting_without_password(
        &self,
        user_id: i32,
        meeting_code: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let user = self
            .user_repository
            .get_user_by_id(user_id)
            .await
            .map_err(|err| MeetingError::UnexpectedError("User not found".into()))?;

        let mut meeting = self
            .meeting_repository
            .get_meeting_by_code(meeting_code)
            .await?;

        let is_member = meeting.members.iter().any(|member| {
            member.member.user_id == Some(user_id)
                && member.member.status != MembersStatusEnum::Inviting as i32
        });

        if !is_member {
            return Err(MeetingError::UnexpectedError(
                "User is not member in the meeting".into(),
            ));
        }

        let now = Utc::now().naive_utc();

        let participant = NewParticipant {
            user_id: Some(user_id),
            meeting_id: &meeting.meeting.id,
            status: ParticipantsStatusEnum::Active as i32,
            created_at: now,
            ccu_id: None,
        };

        let participant = self
            .meeting_repository
            .create_participant(participant)
            .await?;

        meeting
            .participants
            .retain(|p| p.participant.ccu_id != None);

        meeting.participants.push(participant);

        Ok(meeting)
    }

    async fn join_with_password(
        &self,
        user_id: i32,
        meeting_code: i32,
        password: &str,
    ) -> Result<MeetingResponse, MeetingError> {
        let user = self
            .user_repository
            .get_user_by_id(user_id)
            .await
            .map_err(|err| MeetingError::UnexpectedError("User not found".into()))?;

        let mut meeting = self
            .meeting_repository
            .get_meeting_by_code(meeting_code)
            .await?;

        let is_password_correct = verify_password(password, &meeting.meeting.password);

        if !is_password_correct {
            return Err(MeetingError::PasswordIncorrect);
        }

        let now = Utc::now().naive_utc();

        let participant = NewParticipant {
            user_id: Some(user_id),
            meeting_id: &meeting.meeting.id,
            status: ParticipantsStatusEnum::Active as i32,
            created_at: now,
            ccu_id: None,
        };

        let participant = self
            .meeting_repository
            .create_participant(participant)
            .await?;

        meeting
            .participants
            .retain(|p| p.participant.ccu_id != None);

        meeting.participants.push(participant);

        Ok(meeting)
    }

    async fn add_member(
        &self,
        code: i32,
        host_id: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let mut meeting = self.meeting_repository.get_meeting_by_code(code).await?;

        let is_member = meeting
            .members
            .iter()
            .any(|member| member.member.user_id == Some(user_id));

        if is_member {
            return Err(MeetingError::UnexpectedError(
                "User already in the meeting".to_string(),
            ));
        }

        let is_host = meeting.members.iter().any(|member| {
            member.member.user_id == Some(host_id)
                && member.member.role == MembersRoleEnum::Host as i32
        });

        if !is_host {
            return Err(MeetingError::YouDontHavePermissions);
        }

        let user = self
            .user_repository
            .get_user_by_id(user_id)
            .await
            .map_err(|err| MeetingError::UnexpectedError("User not found".to_string()));

        let now = Utc::now().naive_utc();

        let new_member = NewMember {
            user_id: Some(user_id),
            meeting_id: &meeting.meeting.id,
            created_at: now,
            status: MembersStatusEnum::Inviting as i32,
            role: MembersRoleEnum::Attendee as i32,
        };

        let new_member = self.meeting_repository.create_member(new_member).await?;

        meeting.members.push(new_member);

        Ok(meeting)
    }

    async fn remove_member(
        &self,
        code: i32,
        host_id: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let mut meeting = self.meeting_repository.get_meeting_by_code(code).await?;

        let index_of_member = meeting
            .members
            .iter()
            .position(|member| member.member.user_id == Some(user_id))
            .ok_or_else(|| MeetingError::UnexpectedError("Member not found".into()))?;

        let is_host = meeting.members.iter().any(|member| {
            member.member.user_id == Some(host_id)
                && member.member.role == MembersRoleEnum::Host as i32
        });

        if !is_host {
            return Err(MeetingError::YouDontHavePermissions);
        }

        let member_id = meeting.members[index_of_member].member.id;

        self.meeting_repository
            .delete_member_by_id(member_id)
            .await?;

        meeting
            .members
            .retain(|member| member.member.user_id != Some(user_id));

        Ok(meeting)
    }

    async fn accept_invitation(
        &self,
        meeting_id: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let mut meeting = self
            .meeting_repository
            .get_meeting_by_id(meeting_id)
            .await?;

        let index_of_member = meeting
            .members
            .iter()
            .position(|member| {
                member.member.user_id == Some(user_id)
                    && member.member.status == MembersStatusEnum::Inviting as i32
            })
            .ok_or_else(|| MeetingError::UnexpectedError("Member not found".into()))?;

        let mut member = meeting.members[index_of_member].member.clone();

        member.status == MembersStatusEnum::Joined as i32;

        let member = self.meeting_repository.update_member(member).await?;

        meeting.members[index_of_member] = member;

        Ok(meeting)
    }

    async fn archived_meeting(
        &self,
        meeting_code: i32,
        user_id: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let meeting = self
            .meeting_repository
            .get_meeting_by_code(meeting_code)
            .await?;

        let index_of_member = meeting
            .members
            .iter()
            .position(|member| member.member.user_id == Some(user_id))
            .ok_or_else(|| MeetingError::UnexpectedError("Member not found".into()))?;

        let member = meeting.members[index_of_member].member.clone();

        if member.role != MembersRoleEnum::Host as i32 {
            return Err(MeetingError::YouDontHavePermissions);
        }

        let mut meeting = meeting.meeting;

        meeting.status = MeetingsStatusEnum::Archived as i32;

        let meeting = self.meeting_repository.update_meeting(meeting).await?;

        Ok(meeting)
    }
}
