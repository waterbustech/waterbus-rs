use std::collections::HashMap;

use diesel::{
    ExpressionMethods, JoinOnDsl, NullableExpressionMethods, PgConnection, PgSortExpressionMethods,
    QueryDsl, RunQueryDsl, SelectableHelper, insert_into,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    update,
};
use salvo::async_trait;

use crate::core::entities::models::{NewMember, NewParticipant};
use crate::core::{
    database::schema::{meetings, members, messages, participants, users},
    entities::models::{
        Meeting, MeetingsStatusEnum, Member, MembersStatusEnum, Message, NewMeeting, Participant,
        User,
    },
    types::{
        errors::{general::GeneralError, meeting_error::MeetingError},
        res::meeting_response::MeetingResponse,
    },
};

#[async_trait]
pub trait MeetingRepository: Send + Sync {
    async fn find_all(
        &self,
        user_id: i32,
        member_status: MembersStatusEnum,
        meeting_status: MeetingsStatusEnum,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MeetingResponse>, MeetingError>;

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError>;

    async fn get_meeting_by_code(&self, meeting_code: i32)
    -> Result<MeetingResponse, MeetingError>;

    async fn create_meeting(
        &self,
        meeting: NewMeeting<'_>,
    ) -> Result<MeetingResponse, MeetingError>;

    async fn update_meeting(&self, meeting: Meeting) -> Result<MeetingResponse, MeetingError>;

    async fn create_member(&self, member: NewMember<'_>) -> Result<Member, MeetingError>;

    async fn update_member(&self, member: Member) -> Result<Member, MeetingError>;

    async fn create_participant(
        &self,
        participant: NewParticipant<'_>,
    ) -> Result<Participant, MeetingError>;

    async fn update_participant(
        &self,
        participant: Participant,
    ) -> Result<Participant, MeetingError>;
}

#[derive(Debug, Clone)]
pub struct MeetingRepositoryImpl {
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl MeetingRepositoryImpl {
    pub fn new(pool: Pool<ConnectionManager<PgConnection>>) -> Self {
        Self { pool }
    }

    fn get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, GeneralError> {
        self.pool.get().map_err(|_| GeneralError::DbConnectionError)
    }
}

#[async_trait]
impl MeetingRepository for MeetingRepositoryImpl {
    async fn find_all(
        &self,
        user_id: i32,
        member_status: MembersStatusEnum,
        meeting_status: MeetingsStatusEnum,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<MeetingResponse>, MeetingError> {
        let mut conn = self.get_conn()?;

        let meeting_ids = meetings::table
            .inner_join(members::table.on(meetings::id.nullable().eq(members::meetingId)))
            .inner_join(users::table.on(members::userId.eq(users::id.nullable())))
            .filter(meetings::status.eq(meeting_status))
            .filter(members::status.eq(member_status))
            .filter(users::id.eq(user_id))
            .select(meetings::id)
            .load::<i32>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to find meeting_ids".to_string()))?;

        if meeting_ids.is_empty() {
            return Ok(vec![]);
        }

        let (users_for_member, users_for_message) =
            diesel::alias!(users as users_for_member, users as users_for_message);

        let results = meetings::table
            .filter(meetings::id.eq_any(meeting_ids))
            .left_join(members::table.on(meetings::id.nullable().eq(members::meetingId)))
            .left_join(
                users_for_member
                    .on(members::userId.eq(users_for_member.field(users::id).nullable())),
            )
            .left_join(participants::table.on(meetings::id.nullable().eq(participants::meetingId)))
            .left_join(messages::table.on(meetings::latestMessageId.eq(messages::id.nullable())))
            .left_join(
                users_for_message
                    .on(messages::createdById.eq(users_for_message.field(users::id).nullable())),
            )
            .select((
                meetings::all_columns,
                members::all_columns.nullable(),
                participants::all_columns.nullable(),
                messages::all_columns.nullable(),
                users_for_message
                    .fields((
                        users::id,
                        users::fullName,
                        users::userName,
                        users::bio,
                        users::googleId,
                        users::githubId,
                        users::appleId,
                        users::avatar,
                        users::createdAt,
                        users::updatedAt,
                        users::deletedAt,
                        users::lastSeenAt,
                    ))
                    .nullable(),
            ))
            .order(messages::createdAt.desc().nulls_last())
            .offset(skip)
            .limit(limit)
            .load::<(
                Meeting,
                Option<Member>,
                Option<Participant>,
                Option<Message>,
                Option<User>,
            )>(&mut conn)
            .map_err(|_| {
                MeetingError::UnexpectedError("Failed to load meeting data".to_string())
            })?;

        let mut meeting_responses_map: HashMap<i32, MeetingResponse> = HashMap::new();

        for (meeting, member, participant, message, created_by) in results {
            let meeting_response =
                meeting_responses_map
                    .entry(meeting.id)
                    .or_insert(MeetingResponse {
                        id: meeting.id,
                        title: meeting.title,
                        avatar: meeting.avatar,
                        status: meeting.status,
                        password: meeting.password,
                        latest_message_created_at: meeting.latestMessageCreatedAt,
                        code: meeting.code,
                        created_at: meeting.createdAt,
                        updated_at: meeting.updatedAt,
                        deleted_at: meeting.deletedAt,
                        members: Vec::new(),
                        participants: Vec::new(),
                        latest_message: message,
                        created_by,
                    });

            if let Some(member) = member {
                meeting_response.members.push(member);
            }

            if let Some(participant) = participant {
                meeting_response.participants.push(participant);
            }
        }

        let meeting_responses = meeting_responses_map
            .into_iter()
            .map(|(_, response)| response)
            .collect::<Vec<MeetingResponse>>();

        Ok(meeting_responses)
    }

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let result = meetings::table
            .filter(meetings::id.eq(meeting_id))
            .left_join(members::table.on(meetings::id.nullable().eq(members::meetingId)))
            .left_join(participants::table.on(meetings::id.nullable().eq(participants::meetingId)))
            .select((
                meetings::all_columns,
                participants::all_columns.nullable(),
                members::all_columns.nullable(),
            ))
            .load::<(Meeting, Option<Participant>, Option<Member>)>(&mut conn)
            .map_err(|_| MeetingError::MeetingNotFound(meeting_id))?;

        let (meeting, participants, members) = result.into_iter().fold(
            (None, Vec::new(), Vec::new()),
            |(mut meeting, mut participants, mut members), (m, p, mem)| {
                if meeting.is_none() {
                    meeting = Some(m);
                }
                if let Some(p) = p {
                    participants.push(p);
                }
                if let Some(mem) = mem {
                    members.push(mem);
                }
                (meeting, participants, members)
            },
        );

        if let Some(meeting) = meeting {
            let meeting_response = MeetingResponse {
                id: meeting.id,
                title: meeting.title,
                avatar: meeting.avatar,
                status: meeting.status,
                password: meeting.password,
                latest_message_created_at: meeting.latestMessageCreatedAt,
                code: meeting.code,
                created_at: meeting.createdAt,
                updated_at: meeting.updatedAt,
                deleted_at: meeting.deletedAt,
                members: members,
                participants: participants,
                latest_message: None,
                created_by: None,
            };

            Ok(meeting_response)
        } else {
            Err(MeetingError::MeetingNotFound(meeting_id))
        }
    }

    async fn get_meeting_by_code(
        &self,
        meeting_code: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let result = meetings::table
            .filter(meetings::code.eq(meeting_code))
            .left_join(members::table.on(meetings::id.nullable().eq(members::meetingId)))
            .left_join(participants::table.on(meetings::id.nullable().eq(participants::meetingId)))
            .select((
                meetings::all_columns,
                participants::all_columns.nullable(),
                members::all_columns.nullable(),
            ))
            .load::<(Meeting, Option<Participant>, Option<Member>)>(&mut conn)
            .map_err(|_| MeetingError::MeetingNotFound(meeting_code))?;

        let (meeting, participants, members) = result.into_iter().fold(
            (None, Vec::new(), Vec::new()),
            |(mut meeting, mut participants, mut members), (m, p, mem)| {
                if meeting.is_none() {
                    meeting = Some(m);
                }
                if let Some(p) = p {
                    participants.push(p);
                }
                if let Some(mem) = mem {
                    members.push(mem);
                }
                (meeting, participants, members)
            },
        );

        if let Some(meeting) = meeting {
            let meeting_response = MeetingResponse {
                id: meeting.id,
                title: meeting.title,
                avatar: meeting.avatar,
                status: meeting.status,
                password: meeting.password,
                latest_message_created_at: meeting.latestMessageCreatedAt,
                code: meeting.code,
                created_at: meeting.createdAt,
                updated_at: meeting.updatedAt,
                deleted_at: meeting.deletedAt,
                members: members,
                participants: participants,
                latest_message: None,
                created_by: None,
            };

            Ok(meeting_response)
        } else {
            Err(MeetingError::MeetingNotFound(meeting_code))
        }
    }

    async fn create_meeting(
        &self,
        meeting: NewMeeting<'_>,
    ) -> Result<MeetingResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let new_meeting = insert_into(meetings::table)
            .values(&meeting)
            .returning(Meeting::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        let meeting_response = self.get_meeting_by_id(new_meeting.id).await?;

        Ok(meeting_response)
    }

    async fn update_meeting(&self, meeting: Meeting) -> Result<MeetingResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let updated_meeting = update(meetings::table)
            .filter(meetings::id.eq(meeting.id))
            .set((
                meetings::title.eq(meeting.title),
                meetings::avatar.eq(meeting.avatar),
                meetings::password.eq(meeting.password),
            ))
            .returning(Meeting::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        let meeting_response = self.get_meeting_by_id(updated_meeting.id).await?;

        Ok(meeting_response)
    }

    async fn create_member(&self, member: NewMember<'_>) -> Result<Member, MeetingError> {
        let mut conn = self.get_conn()?;
        let new_member = insert_into(members::table)
            .values(&member)
            .returning(Member::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        Ok(new_member)
    }

    async fn update_member(&self, member: Member) -> Result<Member, MeetingError> {
        let mut conn = self.get_conn()?;

        let updated_member = update(members::table)
            .filter(members::id.eq(member.id))
            .set(members::status.eq(member.status))
            .returning(Member::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        Ok(updated_member)
    }

    async fn create_participant(
        &self,
        participant: NewParticipant<'_>,
    ) -> Result<Participant, MeetingError> {
        let mut conn = self.get_conn()?;

        let new_participant = insert_into(participants::table)
            .values(&participant)
            .returning(Participant::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        Ok(new_participant)
    }

    async fn update_participant(
        &self,
        participant: Participant,
    ) -> Result<Participant, MeetingError> {
        let mut conn = self.get_conn()?;

        let updated_participant = update(participants::table)
            .filter(participants::id.eq(participant.id))
            .set(participants::status.eq(participant.status))
            .returning(Participant::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        Ok(updated_participant)
    }
}
