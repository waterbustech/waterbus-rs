use diesel::{
    BelongingToDsl, Connection, ExpressionMethods, GroupedBy, JoinOnDsl, NullableExpressionMethods,
    PgConnection, QueryDsl, RunQueryDsl, SelectableHelper,
    dsl::delete,
    insert_into,
    r2d2::{ConnectionManager, Pool, PooledConnection},
    update,
};
use salvo::async_trait;
use tracing::warn;

use chrono::NaiveDateTime;

use crate::core::{
    database::schema::{members, messages, participants, rooms, users},
    entities::models::{
        Member, MembersRoleEnum, Message, NewRoom, Participant, Room, RoomStatusEnum, User,
    },
    types::{
        errors::{general::GeneralError, room_error::RoomError},
        responses::{
            message_response::MessageResponse,
            room_response::{ParticipantResponse, RoomResponse},
        },
    },
};
use crate::core::{
    entities::models::{NewMember, NewParticipant},
    types::responses::room_response::MemberResponse,
};

#[async_trait]
pub trait RoomRepository: Send + Sync {
    async fn find_all(
        &self,
        user_id: i32,
        room_status: RoomStatusEnum,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<RoomResponse>, RoomError>;

    async fn exists_code(&self, room_code: &str) -> Result<bool, RoomError>;

    async fn get_room_by_id(&self, room_id: i32) -> Result<RoomResponse, RoomError>;

    async fn get_room_by_code(&self, room_code: &str) -> Result<RoomResponse, RoomError>;

    async fn create_room(&self, room: NewRoom<'_>) -> Result<RoomResponse, RoomError>;

    async fn create_room_with_member(
        &self,
        room: NewRoom<'_>,
        user: User,
        created_at: NaiveDateTime,
    ) -> Result<RoomResponse, RoomError>;

    async fn update_room(&self, room: Room) -> Result<RoomResponse, RoomError>;

    async fn get_member_by_id(&self, member_id: i32) -> Result<MemberResponse, RoomError>;

    async fn create_member(&self, member: NewMember<'_>) -> Result<MemberResponse, RoomError>;

    async fn update_member(&self, member: Member) -> Result<MemberResponse, RoomError>;

    async fn delete_member_by_id(&self, member_id: i32) -> Result<(), RoomError>;

    async fn get_participant_by_id(
        &self,
        participant_id: i32,
    ) -> Result<ParticipantResponse, RoomError>;

    async fn create_participant(
        &self,
        participant: NewParticipant<'_>,
    ) -> Result<ParticipantResponse, RoomError>;

    async fn update_participant(
        &self,
        participant: Participant,
    ) -> Result<ParticipantResponse, RoomError>;

    async fn delete_participant_by_id(&self, participant_id: i32) -> Result<(), RoomError>;

    async fn delete_participants_by_node(&self, node_id: &str) -> Result<(), RoomError>;
}

#[derive(Debug, Clone)]
pub struct RoomRepositoryImpl {
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl RoomRepositoryImpl {
    pub fn new(pool: Pool<ConnectionManager<PgConnection>>) -> Self {
        Self { pool }
    }

    fn get_conn(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, GeneralError> {
        self.pool.get().map_err(|_| GeneralError::DbConnectionError)
    }
}

#[async_trait]
impl RoomRepository for RoomRepositoryImpl {
    async fn find_all(
        &self,
        user_id: i32,
        room_status: RoomStatusEnum,
        skip: i64,
        limit: i64,
    ) -> Result<Vec<RoomResponse>, RoomError> {
        let mut conn = self.get_conn()?;

        let room_status: i16 = room_status.into();

        let users_for_message = diesel::alias!(users as users_for_message);

        let rooms_with_latest = rooms::table
            .inner_join(members::table.on(rooms::id.eq(members::room_id)))
            .inner_join(users::table.on(members::user_id.eq(users::id)))
            .filter(rooms::status.eq(room_status))
            .filter(users::id.eq(user_id))
            .left_join(messages::table.on(rooms::latest_message_id.eq(messages::id.nullable())))
            .left_join(
                users_for_message
                    .on(messages::created_by_id.eq(users_for_message.field(users::id))),
            )
            .select((
                Room::as_select(),
                Option::<Message>::as_select(),
                users_for_message
                    .fields((
                        users::id,
                        users::full_name,
                        users::user_name,
                        users::bio,
                        users::external_id,
                        users::avatar,
                        users::created_at,
                        users::updated_at,
                        users::deleted_at,
                        users::last_seen_at,
                    ))
                    .nullable(),
            ))
            .order(rooms::latest_message_created_at.desc())
            .distinct_on(rooms::latest_message_created_at)
            .offset(skip)
            .limit(limit)
            .load::<(Room, Option<Message>, Option<User>)>(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Failed to find rooms".to_string()))?;

        if rooms_with_latest.is_empty() {
            return Ok(vec![]);
        }

        let rooms_only = rooms_with_latest
            .iter()
            .map(|(m, _, _)| m.clone())
            .collect::<Vec<_>>();

        let participants_with_users = Participant::belonging_to(&rooms_only)
            .inner_join(users::table.on(users::id.eq(participants::user_id)))
            .select((Participant::as_select(), Option::<User>::as_select()))
            .load::<(Participant, Option<User>)>(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Failed to get participants".into()))?;

        let members_with_users = Member::belonging_to(&rooms_only)
            .inner_join(users::table.on(users::id.eq(members::user_id)))
            .select((Member::as_select(), Option::<User>::as_select()))
            .load::<(Member, Option<User>)>(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Failed to get members".into()))?;

        let participant_grouped: Vec<Vec<(Participant, Option<User>)>> =
            participants_with_users.grouped_by(&rooms_only);

        let member_grouped: Vec<Vec<(Member, Option<User>)>> =
            members_with_users.grouped_by(&rooms_only);

        let room_responses = rooms_with_latest
            .into_iter()
            .zip(member_grouped)
            .zip(participant_grouped)
            .map(|((tuple, members), participants)| {
                let (room, latest_message, message_user) = tuple;

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
                    room: None,
                });

                RoomResponse {
                    room,
                    members,
                    participants,
                    latest_message,
                }
            })
            .collect::<Vec<_>>();

        Ok(room_responses)
    }

    async fn exists_code(&self, room_code: &str) -> Result<bool, RoomError> {
        let mut conn = self.get_conn()?;

        use self::rooms::dsl::*;

        match rooms
            .filter(code.eq(room_code))
            .select(id)
            .first::<i32>(&mut conn)
        {
            Ok(_) => Ok(true),
            Err(diesel::result::Error::NotFound) => Ok(false),
            Err(e) => Err(RoomError::UnexpectedError(format!(
                "DB error checking room code: {e}"
            ))),
        }
    }

    async fn get_room_by_id(&self, room_id: i32) -> Result<RoomResponse, RoomError> {
        let mut conn = self.get_conn()?;

        let rooms = rooms::table
            .filter(rooms::id.eq(room_id))
            .select(Room::as_select())
            .load::<Room>(&mut conn)
            .map_err(|_| RoomError::RoomNotFound(room_id))?;

        let participants_with_users = Participant::belonging_to(&rooms)
            .inner_join(users::table.on(users::id.eq(participants::user_id)))
            .select((Participant::as_select(), Option::<User>::as_select()))
            .load::<(Participant, Option<User>)>(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Failed to get participants".into()))?;

        let members_with_users = Member::belonging_to(&rooms)
            .inner_join(users::table.on(users::id.eq(members::user_id)))
            .select((Member::as_select(), Option::<User>::as_select()))
            .load::<(Member, Option<User>)>(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Failed to get members".into()))?;

        let participant_grouped: Vec<Vec<(Participant, Option<User>)>> =
            participants_with_users.grouped_by(&rooms);

        let member_grouped: Vec<Vec<(Member, Option<User>)>> =
            members_with_users.grouped_by(&rooms);

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

        let room = rooms
            .into_iter()
            .next()
            .ok_or(RoomError::RoomNotFound(room_id))?;

        let response = RoomResponse {
            room,
            members: member_responses,
            participants: participant_responses,
            latest_message: None,
        };

        Ok(response)
    }

    async fn get_room_by_code(&self, room_code: &str) -> Result<RoomResponse, RoomError> {
        let mut conn = self.get_conn()?;

        let rooms = rooms::table
            .filter(rooms::code.eq(room_code))
            .select(Room::as_select())
            .load::<Room>(&mut conn)
            .map_err(|_| RoomError::RoomCodeNotFound(room_code.to_string()))?;

        let participants_with_users = Participant::belonging_to(&rooms)
            .inner_join(users::table.on(users::id.eq(participants::user_id)))
            .select((Participant::as_select(), Option::<User>::as_select()))
            .load::<(Participant, Option<User>)>(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Failed to get participants".into()))?;

        let members_with_users = Member::belonging_to(&rooms)
            .inner_join(users::table.on(users::id.eq(members::user_id)))
            .select((Member::as_select(), Option::<User>::as_select()))
            .load::<(Member, Option<User>)>(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Failed to get members".into()))?;

        let participant_grouped: Vec<Vec<(Participant, Option<User>)>> =
            participants_with_users.grouped_by(&rooms);

        let member_grouped: Vec<Vec<(Member, Option<User>)>> =
            members_with_users.grouped_by(&rooms);

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

        let room = rooms
            .into_iter()
            .next()
            .ok_or(RoomError::RoomCodeNotFound(room_code.to_string()))?;

        let response = RoomResponse {
            room,
            members: member_responses,
            participants: participant_responses,
            latest_message: None,
        };

        Ok(response)
    }

    async fn create_room(&self, room: NewRoom<'_>) -> Result<RoomResponse, RoomError> {
        let mut conn = self.get_conn()?;

        let new_room = insert_into(rooms::table)
            .values(&room)
            .returning(Room::as_select())
            .get_result(&mut conn)
            .map_err(|err| RoomError::UnexpectedError(err.to_string()))?;

        let room_response = RoomResponse {
            room: new_room,
            members: Vec::new(),
            participants: Vec::new(),
            latest_message: None,
        };

        Ok(room_response)
    }

    async fn create_room_with_member(
        &self,
        room: NewRoom<'_>,
        user: User,
        created_at: NaiveDateTime,
    ) -> Result<RoomResponse, RoomError> {
        let mut conn = self.get_conn()?;

        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            let new_room = insert_into(rooms::table)
                .values(&room)
                .returning(Room::as_select())
                .get_result(conn)?;

            let new_member = NewMember {
                room_id: &new_room.id,
                user_id: Some(user.id),
                role: MembersRoleEnum::Owner.into(),
                created_at,
            };

            let new_member = insert_into(members::table)
                .values(&new_member)
                .returning(Member::as_select())
                .get_result(conn)?;

            let response = RoomResponse {
                room: new_room,
                members: vec![MemberResponse {
                    member: new_member,
                    user: Some(user),
                }],
                participants: vec![],
                latest_message: None,
            };

            Ok(response)
        })
        .map_err(|err| RoomError::UnexpectedError(err.to_string()))
    }

    async fn update_room(&self, room: Room) -> Result<RoomResponse, RoomError> {
        let mut conn = self.get_conn()?;

        let updated_room = update(rooms::table)
            .filter(rooms::id.eq(room.id))
            .set((
                rooms::title.eq(room.title),
                rooms::avatar.eq(room.avatar),
                rooms::password.eq(room.password),
                rooms::latest_message_created_at.eq(room.latest_message_created_at),
                rooms::latest_message_id.eq(room.latest_message_id),
                rooms::status.eq(room.status),
            ))
            .returning(Room::as_select())
            .get_result(&mut conn)
            .map_err(|err| RoomError::UnexpectedError(err.to_string()))?;

        let room_response = self.get_room_by_id(updated_room.id).await?;

        Ok(room_response)
    }

    async fn get_member_by_id(&self, member_id: i32) -> Result<MemberResponse, RoomError> {
        let mut conn = self.get_conn()?;

        let result = members::table
            .filter(members::id.eq(member_id))
            .left_join(users::table.on(members::user_id.nullable().eq(users::id.nullable())))
            .select((Member::as_select(), Option::<User>::as_select()))
            .load::<(Member, Option<User>)>(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Member not found".to_string()))?;

        if result.is_empty() {
            return Err(RoomError::UnexpectedError("Member not found".to_string()));
        }

        match result.into_iter().next() {
            Some((member, user)) => Ok(MemberResponse { member, user }),
            None => Err(RoomError::UnexpectedError("Member not found".to_string())),
        }
    }

    async fn create_member(&self, member: NewMember<'_>) -> Result<MemberResponse, RoomError> {
        let mut conn = self.get_conn()?;
        let new_member = insert_into(members::table)
            .values(&member)
            .returning(Member::as_select())
            .get_result(&mut conn)
            .map_err(|err| RoomError::UnexpectedError(err.to_string()))?;

        self.get_member_by_id(new_member.id).await
    }

    async fn update_member(&self, member: Member) -> Result<MemberResponse, RoomError> {
        let mut conn = self.get_conn()?;

        let updated_member = update(members::table)
            .filter(members::id.eq(member.id))
            .set(members::soft_deleted_at.eq(member.soft_deleted_at))
            .returning(Member::as_select())
            .get_result(&mut conn)
            .map_err(|err| RoomError::UnexpectedError(err.to_string()))?;

        self.get_member_by_id(updated_member.id).await
    }

    async fn delete_member_by_id(&self, member_id: i32) -> Result<(), RoomError> {
        let mut conn = self.get_conn()?;

        delete(members::table)
            .filter(members::id.eq(member_id))
            .execute(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Failed to delete member".to_string()))?;

        Ok(())
    }

    async fn get_participant_by_id(
        &self,
        participant_id: i32,
    ) -> Result<ParticipantResponse, RoomError> {
        let mut conn = self.get_conn()?;

        let result = participants::table
            .filter(participants::id.eq(participant_id))
            .left_join(users::table.on(participants::user_id.nullable().eq(users::id.nullable())))
            .select((Participant::as_select(), Option::<User>::as_select()))
            .load::<(Participant, Option<User>)>(&mut conn)
            .map_err(|_| RoomError::UnexpectedError("Participant not found".to_string()))?;

        if result.is_empty() {
            return Err(RoomError::UnexpectedError(
                "Participant not found".to_string(),
            ));
        }

        match result.into_iter().next() {
            Some((participant, user)) => Ok(ParticipantResponse { participant, user }),
            None => Err(RoomError::UnexpectedError(
                "Participant not found".to_string(),
            )),
        }
    }

    async fn create_participant(
        &self,
        participant: NewParticipant<'_>,
    ) -> Result<ParticipantResponse, RoomError> {
        let mut conn = self.get_conn()?;

        let new_participant = insert_into(participants::table)
            .values(&participant)
            .returning(Participant::as_select())
            .get_result(&mut conn)
            .map_err(|err| RoomError::UnexpectedError(err.to_string()))?;

        self.get_participant_by_id(new_participant.id).await
    }

    async fn update_participant(
        &self,
        participant: Participant,
    ) -> Result<ParticipantResponse, RoomError> {
        let mut conn = self.get_conn()?;

        let updated_participant = update(participants::table)
            .filter(participants::id.eq(participant.id))
            .set((
                participants::status.eq(participant.status),
                participants::node_id.eq(participant.node_id),
            ))
            .returning(Participant::as_select())
            .get_result(&mut conn)
            .map_err(|err| RoomError::UnexpectedError(err.to_string()))?;

        self.get_participant_by_id(updated_participant.id).await
    }

    async fn delete_participant_by_id(&self, participant_id: i32) -> Result<(), RoomError> {
        let mut conn = self.get_conn()?;

        let deleted_rows = delete(participants::table)
            .filter(participants::id.eq(participant_id))
            .execute(&mut conn)
            .map_err(|err| {
                warn!("err: {:?}", err);
                RoomError::UnexpectedError("Failed to delete participant".to_string())
            })?;

        if deleted_rows == 0 {
            return Err(RoomError::UnexpectedError(
                "No participant found to delete".to_string(),
            ));
        }

        let participant = self.get_participant_by_id(participant_id).await;

        if let Ok(participant) = participant {
            warn!("Participant found: {:?}", participant);
        }

        Ok(())
    }

    async fn delete_participants_by_node(&self, node_id: &str) -> Result<(), RoomError> {
        let mut conn = self.get_conn()?;

        let deleted_rows = delete(participants::table)
            .filter(participants::node_id.eq(node_id))
            .execute(&mut conn)
            .map_err(|err| {
                warn!(
                    "Failed to delete participants for node {}: {:?}",
                    node_id, err
                );
                RoomError::UnexpectedError("Failed to delete participants by node".into())
            })?;

        if deleted_rows == 0 {
            warn!("No participants found for node_id: {}", node_id);
        }

        Ok(())
    }
}
