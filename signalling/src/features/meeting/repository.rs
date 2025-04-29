use diesel::{
    BelongingToDsl, ExpressionMethods, GroupedBy, JoinOnDsl, NullableExpressionMethods,
    PgConnection, QueryDsl, RunQueryDsl, SelectableHelper,
    dsl::delete,
    insert_into,
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
        res::{
            meeting_response::{MeetingResponse, ParticipantResponse},
            message_response::MessageResponse,
        },
    },
};
use crate::core::{
    entities::models::{NewMember, NewParticipant},
    types::res::meeting_response::MemberResponse,
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

    async fn get_member_by_id(&self, member_id: i32) -> Result<MemberResponse, MeetingError>;

    async fn create_member(&self, member: NewMember<'_>) -> Result<MemberResponse, MeetingError>;

    async fn update_member(&self, member: Member) -> Result<MemberResponse, MeetingError>;

    async fn delete_member_by_id(&self, member_id: i32) -> Result<(), MeetingError>;

    async fn get_participant_by_id(
        &self,
        participant_id: i32,
    ) -> Result<ParticipantResponse, MeetingError>;

    async fn create_participant(
        &self,
        participant: NewParticipant<'_>,
    ) -> Result<ParticipantResponse, MeetingError>;

    async fn update_participant(
        &self,
        participant: Participant,
    ) -> Result<ParticipantResponse, MeetingError>;

    async fn delete_participant_by_id(&self, participant_id: i32) -> Result<(), MeetingError>;
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

        let meeting_status = meeting_status as i32;
        let member_status = member_status as i32;

        let users_for_message = diesel::alias!(users as users_for_message);

        let meetings_with_latest = meetings::table
            .inner_join(members::table.on(meetings::id.nullable().eq(members::meeting_id)))
            .inner_join(users::table.on(members::user_id.eq(users::id.nullable())))
            .filter(meetings::status.eq(meeting_status))
            .filter(members::status.eq(member_status))
            .filter(users::id.eq(user_id))
            .left_join(messages::table.on(meetings::latest_message_id.eq(messages::id.nullable())))
            .left_join(
                users_for_message
                    .on(messages::created_by_id.eq(users_for_message.field(users::id).nullable())),
            )
            .select((
                Meeting::as_select(),
                Option::<Message>::as_select(),
                users_for_message
                    .fields((
                        users::id,
                        users::full_name,
                        users::user_name,
                        users::bio,
                        users::google_id,
                        users::custom_id,
                        users::avatar,
                        users::created_at,
                        users::updated_at,
                        users::deleted_at,
                        users::last_seen_at,
                    ))
                    .nullable(),
            ))
            .order(meetings::latest_message_created_at.desc())
            .distinct_on(meetings::latest_message_created_at)
            .offset(skip)
            .limit(limit)
            .load::<(Meeting, Option<Message>, Option<User>)>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to find meetings".to_string()))?;

        if meetings_with_latest.is_empty() {
            return Ok(vec![]);
        }

        let meetings_only = meetings_with_latest
            .iter()
            .map(|(m, _, _)| m.clone())
            .collect::<Vec<_>>();

        let participants_with_users = Participant::belonging_to(&meetings_only)
            .inner_join(users::table.on(users::id.nullable().eq(participants::user_id)))
            .select((Participant::as_select(), Option::<User>::as_select()))
            .load::<(Participant, Option<User>)>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to get participants".into()))?;

        let members_with_users = Member::belonging_to(&meetings_only)
            .inner_join(users::table.on(users::id.nullable().eq(members::user_id)))
            .select((Member::as_select(), Option::<User>::as_select()))
            .load::<(Member, Option<User>)>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to get members".into()))?;

        let participant_grouped: Vec<Vec<(Participant, Option<User>)>> =
            participants_with_users.grouped_by(&meetings_only);

        let member_grouped: Vec<Vec<(Member, Option<User>)>> =
            members_with_users.grouped_by(&meetings_only);

        let meeting_responses = meetings_with_latest
            .into_iter()
            .zip(member_grouped)
            .zip(participant_grouped)
            .map(|((tuple, members), participants)| {
                let (meeting, latest_message, message_user) = tuple;

                let members = members
                    .into_iter()
                    .map(|(member, user)| MemberResponse { member, user })
                    .collect();

                let participants = participants
                    .into_iter()
                    .map(|(participant, user)| ParticipantResponse { participant, user })
                    .collect();

                let latest_message = latest_message.map(|message| MessageResponse {
                    message,
                    created_by: message_user,
                    meeting: None,
                });

                MeetingResponse {
                    meeting,
                    members,
                    participants,
                    latest_message,
                }
            })
            .collect::<Vec<_>>();

        Ok(meeting_responses)
    }

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let meetings = meetings::table
            .filter(meetings::id.eq(meeting_id))
            .select(Meeting::as_select())
            .load::<Meeting>(&mut conn)
            .map_err(|_| MeetingError::MeetingNotFound(meeting_id))?;

        let participants_with_users = Participant::belonging_to(&meetings)
            .inner_join(users::table.on(users::id.nullable().eq(participants::user_id)))
            .select((Participant::as_select(), Option::<User>::as_select()))
            .load::<(Participant, Option<User>)>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to get participants".into()))?;

        let members_with_users = Member::belonging_to(&meetings)
            .inner_join(users::table.on(users::id.nullable().eq(members::user_id)))
            .select((Member::as_select(), Option::<User>::as_select()))
            .load::<(Member, Option<User>)>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to get members".into()))?;

        let participant_grouped: Vec<Vec<(Participant, Option<User>)>> =
            participants_with_users.grouped_by(&meetings);

        let member_grouped: Vec<Vec<(Member, Option<User>)>> =
            members_with_users.grouped_by(&meetings);

        let participant_responses: Vec<ParticipantResponse> = participant_grouped
            .into_iter()
            .flatten()
            .map(|(participant, user)| ParticipantResponse { participant, user })
            .collect();

        let member_responses: Vec<MemberResponse> = member_grouped
            .into_iter()
            .flatten()
            .map(|(member, user)| MemberResponse { member, user })
            .collect();

        let meeting = meetings
            .into_iter()
            .next()
            .ok_or(MeetingError::MeetingNotFound(meeting_id))?;

        let response = MeetingResponse {
            meeting,
            members: member_responses,
            participants: participant_responses,
            latest_message: None,
        };

        Ok(response)
    }

    async fn get_meeting_by_code(
        &self,
        meeting_code: i32,
    ) -> Result<MeetingResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let meetings = meetings::table
            .filter(meetings::code.eq(meeting_code))
            .select(Meeting::as_select())
            .load::<Meeting>(&mut conn)
            .map_err(|_| MeetingError::MeetingNotFound(meeting_code))?;

        let participants_with_users = Participant::belonging_to(&meetings)
            .inner_join(users::table.on(users::id.nullable().eq(participants::user_id)))
            .select((Participant::as_select(), Option::<User>::as_select()))
            .load::<(Participant, Option<User>)>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to get participants".into()))?;

        let members_with_users = Member::belonging_to(&meetings)
            .inner_join(users::table.on(users::id.nullable().eq(members::user_id)))
            .select((Member::as_select(), Option::<User>::as_select()))
            .load::<(Member, Option<User>)>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to get members".into()))?;

        let participant_grouped: Vec<Vec<(Participant, Option<User>)>> =
            participants_with_users.grouped_by(&meetings);

        let member_grouped: Vec<Vec<(Member, Option<User>)>> =
            members_with_users.grouped_by(&meetings);

        let participant_responses: Vec<ParticipantResponse> = participant_grouped
            .into_iter()
            .flatten()
            .map(|(participant, user)| ParticipantResponse { participant, user })
            .collect();

        let member_responses: Vec<MemberResponse> = member_grouped
            .into_iter()
            .flatten()
            .map(|(member, user)| MemberResponse { member, user })
            .collect();

        let meeting = meetings
            .into_iter()
            .next()
            .ok_or(MeetingError::MeetingNotFound(meeting_code))?;

        let response = MeetingResponse {
            meeting,
            members: member_responses,
            participants: participant_responses,
            latest_message: None,
        };

        Ok(response)
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

        let meeting_response = MeetingResponse {
            meeting: new_meeting,
            members: Vec::new(),
            participants: Vec::new(),
            latest_message: None,
        };

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
                meetings::latest_message_created_at.eq(meeting.latest_message_created_at),
                meetings::latest_message_id.eq(meeting.latest_message_id),
                meetings::status.eq(meeting.status),
            ))
            .returning(Meeting::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        let meeting_response = self.get_meeting_by_id(updated_meeting.id).await?;

        Ok(meeting_response)
    }

    async fn get_member_by_id(&self, member_id: i32) -> Result<MemberResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let result = members::table
            .filter(members::id.eq(member_id))
            .left_join(users::table.on(members::user_id.nullable().eq(users::id.nullable())))
            .select((Member::as_select(), Option::<User>::as_select()))
            .load::<(Member, Option<User>)>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Member not found".to_string()))?;

        if result.is_empty() {
            return Err(MeetingError::UnexpectedError(
                "Member not found".to_string(),
            ));
        }

        match result.into_iter().next() {
            Some((member, user)) => Ok(MemberResponse { member, user }),
            None => Err(MeetingError::UnexpectedError(
                "Member not found".to_string(),
            )),
        }
    }

    async fn create_member(&self, member: NewMember<'_>) -> Result<MemberResponse, MeetingError> {
        let mut conn = self.get_conn()?;
        let new_member = insert_into(members::table)
            .values(&member)
            .returning(Member::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        let member = self.get_member_by_id(new_member.id).await;

        member
    }

    async fn update_member(&self, member: Member) -> Result<MemberResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let updated_member = update(members::table)
            .filter(members::id.eq(member.id))
            .set(members::status.eq(member.status))
            .returning(Member::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        let member = self.get_member_by_id(updated_member.id).await;

        member
    }

    async fn delete_member_by_id(&self, member_id: i32) -> Result<(), MeetingError> {
        let mut conn = self.get_conn()?;

        delete(members::table)
            .filter(members::id.eq(member_id))
            .execute(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to delete member".to_string()))?;

        Ok(())
    }

    async fn get_participant_by_id(
        &self,
        participant_id: i32,
    ) -> Result<ParticipantResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let result = participants::table
            .filter(participants::id.eq(participant_id))
            .left_join(users::table.on(participants::user_id.nullable().eq(users::id.nullable())))
            .select((Participant::as_select(), Option::<User>::as_select()))
            .load::<(Participant, Option<User>)>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Participant not found".to_string()))?;

        if result.is_empty() {
            return Err(MeetingError::UnexpectedError(
                "Participant not found".to_string(),
            ));
        }

        match result.into_iter().next() {
            Some((participant, user)) => Ok(ParticipantResponse { participant, user }),
            None => Err(MeetingError::UnexpectedError(
                "Participant not found".to_string(),
            )),
        }
    }

    async fn create_participant(
        &self,
        participant: NewParticipant<'_>,
    ) -> Result<ParticipantResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let new_participant = insert_into(participants::table)
            .values(&participant)
            .returning(Participant::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        let participant = self.get_participant_by_id(new_participant.id).await;

        participant
    }

    async fn update_participant(
        &self,
        participant: Participant,
    ) -> Result<ParticipantResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let updated_participant = update(participants::table)
            .filter(participants::id.eq(participant.id))
            .set((
                participants::status.eq(participant.status),
                participants::ccu_id.eq(participant.ccu_id),
            ))
            .returning(Participant::as_select())
            .get_result(&mut conn)
            .map_err(|err| MeetingError::UnexpectedError(err.to_string()))?;

        let participant = self.get_participant_by_id(updated_participant.id).await;

        participant
    }

    async fn delete_participant_by_id(&self, participant_id: i32) -> Result<(), MeetingError> {
        let mut conn = self.get_conn()?;

        // Perform the deletion
        let deleted_rows = delete(participants::table)
            .filter(participants::id.eq(participant_id))
            .execute(&mut conn)
            .map_err(|err| {
                println!("err: {:?}", err);
                MeetingError::UnexpectedError("Failed to delete participant".to_string())
            })?;

        // Check if any rows were actually deleted
        if deleted_rows == 0 {
            return Err(MeetingError::UnexpectedError(
                "No participant found to delete".to_string(),
            ));
        }

        let participant = self.get_participant_by_id(participant_id).await;

        match participant {
            Ok(participant) => {
                // Successfully found the participant, print or use the participant
                println!("Participant found: {:?}", participant);
            }
            Err(_) => {}
        }

        Ok(())
    }
}
