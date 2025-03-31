// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "meetings_status_enum"))]
    pub struct MeetingsStatusEnum;

    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "members_role_enum"))]
    pub struct MembersRoleEnum;

    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "members_status_enum"))]
    pub struct MembersStatusEnum;

    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "messages_status_enum"))]
    pub struct MessagesStatusEnum;

    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "messages_type_enum"))]
    pub struct MessagesTypeEnum;

    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "participants_status_enum"))]
    pub struct ParticipantsStatusEnum;

    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "records_status_enum"))]
    pub struct RecordsStatusEnum;
}

diesel::table! {
    ccus (id) {
        id -> Int4,
        socketId -> Varchar,
        podName -> Varchar,
        createdAt -> Timestamp,
        userId -> Nullable<Int4>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::MeetingsStatusEnum;

    meetings (id) {
        id -> Int4,
        title -> Varchar,
        password -> Varchar,
        avatar -> Nullable<Varchar>,
        status -> MeetingsStatusEnum,
        latestMessageCreatedAt -> Nullable<Timestamp>,
        code -> Int4,
        createdAt -> Timestamp,
        updatedAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        latestMessageId -> Nullable<Int4>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::MembersRoleEnum;
    use super::sql_types::MembersStatusEnum;

    members (id) {
        id -> Int4,
        role -> MembersRoleEnum,
        status -> MembersStatusEnum,
        createdAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        softDeletedAt -> Nullable<Timestamp>,
        userId -> Nullable<Int4>,
        meetingId -> Nullable<Int4>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::MessagesTypeEnum;
    use super::sql_types::MessagesStatusEnum;

    messages (id) {
        id -> Int4,
        data -> Varchar,
        #[sql_name = "type"]
        type_ -> MessagesTypeEnum,
        status -> MessagesStatusEnum,
        createdAt -> Timestamp,
        updatedAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        createdById -> Nullable<Int4>,
        meetingId -> Nullable<Int4>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::ParticipantsStatusEnum;

    participants (id) {
        id -> Int4,
        status -> ParticipantsStatusEnum,
        createdAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        userId -> Nullable<Int4>,
        meetingId -> Nullable<Int4>,
        ccuId -> Nullable<Int4>,
    }
}

diesel::table! {
    #[sql_name = "record-tracks"]
    record_tracks (id) {
        id -> Int4,
        urlToVideo -> Varchar,
        startTime -> Varchar,
        endTime -> Varchar,
        createdAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        recordId -> Nullable<Int4>,
        userId -> Nullable<Int4>,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::RecordsStatusEnum;

    records (id) {
        id -> Int4,
        urlToVideo -> Nullable<Varchar>,
        thumbnail -> Nullable<Varchar>,
        duration -> Int4,
        status -> RecordsStatusEnum,
        createdAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        meetingId -> Nullable<Int4>,
        createdById -> Nullable<Int4>,
    }
}

diesel::table! {
    sessions (id) {
        id -> Int4,
        createdAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        userId -> Nullable<Int4>,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        fullName -> Nullable<Varchar>,
        userName -> Varchar,
        bio -> Nullable<Varchar>,
        googleId -> Nullable<Varchar>,
        githubId -> Nullable<Varchar>,
        appleId -> Nullable<Varchar>,
        avatar -> Nullable<Varchar>,
        createdAt -> Timestamp,
        updatedAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        lastSeenAt -> Nullable<Timestamp>,
    }
}

diesel::table! {
    #[sql_name = "white-boards"]
    white_boards (id) {
        id -> Int4,
        paints -> Text,
        createdAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        meetingId -> Nullable<Int4>,
    }
}

diesel::joinable!(ccus -> users (userId));
diesel::joinable!(members -> meetings (meetingId));
diesel::joinable!(members -> users (userId));
diesel::joinable!(messages -> users (createdById));
diesel::joinable!(participants -> ccus (ccuId));
diesel::joinable!(participants -> meetings (meetingId));
diesel::joinable!(participants -> users (userId));
diesel::joinable!(record_tracks -> records (recordId));
diesel::joinable!(record_tracks -> users (userId));
diesel::joinable!(records -> meetings (meetingId));
diesel::joinable!(records -> users (createdById));
diesel::joinable!(sessions -> users (userId));
diesel::joinable!(white_boards -> meetings (meetingId));

diesel::allow_tables_to_appear_in_same_query!(
    ccus,
    meetings,
    members,
    messages,
    participants,
    record_tracks,
    records,
    sessions,
    users,
    white_boards,
);
