use diesel::{
    ExpressionMethods, JoinOnDsl, NullableExpressionMethods, PgConnection, PgSortExpressionMethods,
    QueryDsl, RunQueryDsl, SelectableHelper, insert_into,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    update,
};
use salvo::async_trait;

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

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<Meeting, MeetingError>;

    async fn get_meeting_by_code(&self, meeting_code: i32) -> Result<Meeting, MeetingError>;

    async fn create_meeting(&self, meeting: NewMeeting<'_>) -> Result<Meeting, MeetingError>;

    async fn update_meeting(&self, meeting: Meeting) -> Result<Meeting, MeetingError>;
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

        let meeting_responses: Vec<MeetingResponse> = results
            .into_iter()
            .map(
                |(meeting, member, participant, message, created_by)| MeetingResponse {
                    meeting: meeting,
                    member: member,
                    participant: participant,
                    latest_message: message,
                    created_by: created_by,
                },
            )
            .collect();

        Ok(meeting_responses)
    }

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<Meeting, MeetingError> {
        let mut conn = self.get_conn()?;

        let meeting = meetings::table
            .filter(meetings::id.eq(meeting_id))
            .first::<Meeting>(&mut conn)
            .map_err(|_| MeetingError::MeetingNotFound(meeting_id))?;

        Ok(meeting)
    }

    async fn get_meeting_by_code(&self, meeting_code: i32) -> Result<Meeting, MeetingError> {
        let mut conn = self.get_conn()?;

        let meeting = meetings::table
            .filter(meetings::code.eq(meeting_code))
            .first::<Meeting>(&mut conn)
            .map_err(|_| MeetingError::MeetingNotFound(meeting_code))?;

        Ok(meeting)
    }

    async fn create_meeting(&self, meeting: NewMeeting<'_>) -> Result<Meeting, MeetingError> {
        let mut conn = self.get_conn()?;

        let new_meeting = insert_into(meetings::table)
            .values(&meeting)
            .returning(Meeting::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        Ok(new_meeting)
    }

    async fn update_meeting(&self, meeting: Meeting) -> Result<Meeting, MeetingError> {
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

        Ok(updated_meeting)
    }
}
