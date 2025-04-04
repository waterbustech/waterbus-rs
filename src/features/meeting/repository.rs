#![allow(unused)]

use std::collections::HashMap;

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

        let meeting_status = meeting_status as i32;
        let member_status = member_status as i32;

        // Step 1: Get relevant meeting IDs
        let meeting_ids = meetings::table
            .inner_join(members::table.on(meetings::id.nullable().eq(members::meeting_id)))
            .inner_join(users::table.on(members::user_id.eq(users::id.nullable())))
            .filter(meetings::status.eq(meeting_status))
            .filter(members::status.eq(member_status))
            .filter(users::id.eq(user_id))
            .select(meetings::id)
            .load::<i32>(&mut conn)
            .map_err(|_| MeetingError::UnexpectedError("Failed to find meeting_ids".to_string()))?;

        if meeting_ids.is_empty() {
            return Ok(vec![]);
        }

        // Step 2: Aliased users table for joins
        let (users_for_member, users_for_participant, users_for_message) = diesel::alias!(
            users as users_for_member,
            users as users_for_participant,
            users as users_for_message
        );

        // Step 3: Load all related meeting data
        let results =
            meetings::table
                .filter(meetings::id.eq_any(meeting_ids))
                .left_join(members::table.on(meetings::id.nullable().eq(members::meeting_id)))
                .left_join(
                    users_for_member
                        .on(members::user_id.eq(users_for_member.field(users::id).nullable())),
                )
                .left_join(
                    participants::table.on(meetings::id.nullable().eq(participants::meeting_id)),
                )
                .left_join(users_for_participant.on(
                    participants::user_id.eq(users_for_participant.field(users::id).nullable()),
                ))
                .left_join(
                    messages::table.on(meetings::latest_message_id.eq(messages::id.nullable())),
                )
                .left_join(
                    users_for_message
                        .on(messages::created_by_id
                            .eq(users_for_message.field(users::id).nullable())),
                )
                .select((
                    Meeting::as_select(),
                    Option::<Member>::as_select(),
                    Option::<Participant>::as_select(),
                    Option::<Message>::as_select(),
                    users_for_message
                        .fields((
                            users::id,
                            users::full_name,
                            users::user_name,
                            users::bio,
                            users::google_id,
                            users::github_id,
                            users::apple_id,
                            users::avatar,
                            users::created_at,
                            users::updated_at,
                            users::deleted_at,
                            users::last_seen_at,
                        ))
                        .nullable(),
                    users_for_member
                        .fields((
                            users::id,
                            users::full_name,
                            users::user_name,
                            users::bio,
                            users::google_id,
                            users::github_id,
                            users::apple_id,
                            users::avatar,
                            users::created_at,
                            users::updated_at,
                            users::deleted_at,
                            users::last_seen_at,
                        ))
                        .nullable(),
                    users_for_participant
                        .fields((
                            users::id,
                            users::full_name,
                            users::user_name,
                            users::bio,
                            users::google_id,
                            users::github_id,
                            users::apple_id,
                            users::avatar,
                            users::created_at,
                            users::updated_at,
                            users::deleted_at,
                            users::last_seen_at,
                        ))
                        .nullable(),
                ))
                .order(messages::created_at.desc().nulls_last())
                .offset(skip)
                .limit(limit)
                .load::<(
                    Meeting,
                    Option<Member>,
                    Option<Participant>,
                    Option<Message>,
                    Option<User>, // message created_by
                    Option<User>, // member user
                    Option<User>, // participant user
                )>(&mut conn)
                .map_err(|_| {
                    MeetingError::UnexpectedError("Failed to load meeting data".to_string())
                })?;

        // Step 4: Aggregate results into meeting responses
        let mut meeting_responses_map: HashMap<i32, MeetingResponse> = HashMap::new();

        for (meeting, member, participant, message, created_by, member_user, participant_user) in
            results
        {
            let mut latest_message = None;
            if let Some(message) = message {
                latest_message = Some(MessageResponse {
                    message: message,
                    meeting: None,
                    created_by,
                });
            }

            let entry = meeting_responses_map
                .entry(meeting.id)
                .or_insert(MeetingResponse {
                    meeting,
                    latest_message,
                    members: Vec::new(),
                    participants: Vec::new(),
                });

            if let Some(mem) = member {
                entry.members.push(MemberResponse {
                    member: mem,
                    user: member_user.clone(),
                });
            }

            if let Some(part) = participant {
                entry.participants.push(ParticipantResponse {
                    participant: part,
                    user: participant_user.clone(),
                });
            }
        }

        // Step 5: Convert map into Vec
        let meeting_responses = meeting_responses_map
            .into_values()
            .collect::<Vec<MeetingResponse>>();

        Ok(meeting_responses)
    }

    async fn get_meeting_by_id(&self, meeting_id: i32) -> Result<MeetingResponse, MeetingError> {
        let mut conn = self.get_conn()?;

        let (users_for_member, users_for_participant) =
            diesel::alias!(users as users_for_member, users as users_for_participant);

        let result = meetings::table
            .filter(meetings::id.eq(meeting_id))
            .left_join(members::table.on(meetings::id.nullable().eq(members::meeting_id)))
            .left_join(
                users_for_member.on(members::user_id
                    .nullable()
                    .eq(users_for_member.field(users::id).nullable())),
            )
            .left_join(participants::table.on(meetings::id.nullable().eq(participants::meeting_id)))
            .left_join(
                users_for_participant.on(participants::user_id
                    .nullable()
                    .eq(users_for_participant.field(users::id).nullable())),
            )
            .select((
                Meeting::as_select(),
                Option::<Member>::as_select(),
                Option::<Participant>::as_select(),
                users_for_member
                    .fields((
                        users::id,
                        users::full_name,
                        users::user_name,
                        users::bio,
                        users::google_id,
                        users::github_id,
                        users::apple_id,
                        users::avatar,
                        users::created_at,
                        users::updated_at,
                        users::deleted_at,
                        users::last_seen_at,
                    ))
                    .nullable(),
                users_for_participant
                    .fields((
                        users::id,
                        users::full_name,
                        users::user_name,
                        users::bio,
                        users::google_id,
                        users::github_id,
                        users::apple_id,
                        users::avatar,
                        users::created_at,
                        users::updated_at,
                        users::deleted_at,
                        users::last_seen_at,
                    ))
                    .nullable(),
            ))
            .load::<(
                Meeting,
                Option<Member>,
                Option<Participant>,
                Option<User>,
                Option<User>,
            )>(&mut conn)
            .map_err(|_| MeetingError::MeetingNotFound(meeting_id))?;

        // Process the query result into a structured response
        let (meeting, participants, members) = result.into_iter().fold(
            (None, Vec::new(), Vec::new()),
            |(mut meeting, mut participants, mut members), (m, mem, p, user1, user2)| {
                // Assign meeting if it's not already set
                if meeting.is_none() {
                    meeting = Some(m);
                }

                // Map users to participants and members
                if let Some(user) = user1 {
                    // Map user1 (for members) to a Member object
                    if let Some(mem) = mem {
                        members.push(MemberResponse {
                            member: mem,
                            user: Some(user.clone()),
                        });
                    }
                }

                if let Some(user) = user2 {
                    // Map user2 (for participants) to a Participant object
                    if let Some(p) = p {
                        participants.push(ParticipantResponse {
                            user: Some(user.clone()),
                            participant: p,
                        });
                    }
                }

                (meeting, participants, members)
            },
        );

        // Construct and return the MeetingResponse
        if let Some(meeting) = meeting {
            let meeting_response = MeetingResponse {
                meeting,
                participants,
                members,
                latest_message: None,
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

        let (users_for_member, users_for_participant) =
            diesel::alias!(users as users_for_member, users as users_for_participant);

        let result = meetings::table
            .filter(meetings::code.eq(meeting_code))
            .left_join(members::table.on(meetings::id.nullable().eq(members::meeting_id)))
            .left_join(
                users_for_member.on(members::user_id
                    .nullable()
                    .eq(users_for_member.field(users::id).nullable())),
            )
            .left_join(participants::table.on(meetings::id.nullable().eq(participants::meeting_id)))
            .left_join(
                users_for_participant.on(participants::user_id
                    .nullable()
                    .eq(users_for_participant.field(users::id).nullable())),
            )
            .select((
                Meeting::as_select(),
                Option::<Member>::as_select(),
                Option::<Participant>::as_select(),
                users_for_member
                    .fields((
                        users::id,
                        users::full_name,
                        users::user_name,
                        users::bio,
                        users::google_id,
                        users::github_id,
                        users::apple_id,
                        users::avatar,
                        users::created_at,
                        users::updated_at,
                        users::deleted_at,
                        users::last_seen_at,
                    ))
                    .nullable(),
                users_for_participant
                    .fields((
                        users::id,
                        users::full_name,
                        users::user_name,
                        users::bio,
                        users::google_id,
                        users::github_id,
                        users::apple_id,
                        users::avatar,
                        users::created_at,
                        users::updated_at,
                        users::deleted_at,
                        users::last_seen_at,
                    ))
                    .nullable(),
            ))
            .load::<(
                Meeting,
                Option<Member>,
                Option<Participant>,
                Option<User>,
                Option<User>,
            )>(&mut conn)
            .map_err(|_| MeetingError::MeetingNotFound(meeting_code))?;

        // Process the query result into a structured response
        let (meeting, participants, members) = result.into_iter().fold(
            (None, Vec::new(), Vec::new()),
            |(mut meeting, mut participants, mut members), (m, mem, p, user1, user2)| {
                // Assign meeting if it's not already set
                if meeting.is_none() {
                    meeting = Some(m);
                }

                // Map users to participants and members
                if let Some(user) = user1 {
                    // Map user1 (for members) to a Member object
                    if let Some(mem) = mem {
                        members.push(MemberResponse {
                            member: mem,
                            user: Some(user.clone()),
                        });
                    }
                }

                if let Some(user) = user2 {
                    // Map user2 (for participants) to a Participant object
                    if let Some(p) = p {
                        participants.push(ParticipantResponse {
                            user: Some(user.clone()),
                            participant: p,
                        });
                    }
                }

                (meeting, participants, members)
            },
        );

        // Construct and return the MeetingResponse
        if let Some(meeting) = meeting {
            let meeting_response = MeetingResponse {
                meeting,
                participants,
                members,
                latest_message: None,
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
            .map_err(|err| MeetingError::UnexpectedError("Member not found".to_string()))?;

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
