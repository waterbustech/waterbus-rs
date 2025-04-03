// @generated automatically by Diesel CLI.

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
    meetings (id) {
        id -> Int4,
        title -> Varchar,
        password -> Varchar,
        avatar -> Nullable<Varchar>,
        latestMessageCreatedAt -> Nullable<Timestamp>,
        code -> Int4,
        createdAt -> Timestamp,
        updatedAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        latestMessageId -> Nullable<Int4>,
        status -> Int4,
    }
}

diesel::table! {
    members (id) {
        id -> Int4,
        createdAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        softDeletedAt -> Nullable<Timestamp>,
        userId -> Nullable<Int4>,
        meetingId -> Nullable<Int4>,
        role -> Int4,
        status -> Int4,
    }
}

diesel::table! {
    messages (id) {
        id -> Int4,
        data -> Varchar,
        createdAt -> Timestamp,
        updatedAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        createdById -> Nullable<Int4>,
        meetingId -> Nullable<Int4>,
        #[sql_name = "type"]
        type_ -> Int4,
        status -> Int4,
    }
}

diesel::table! {
    participants (id) {
        id -> Int4,
        createdAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        userId -> Nullable<Int4>,
        meetingId -> Nullable<Int4>,
        ccuId -> Nullable<Int4>,
        status -> Int4,
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
    records (id) {
        id -> Int4,
        urlToVideo -> Nullable<Varchar>,
        thumbnail -> Nullable<Varchar>,
        duration -> Int4,
        createdAt -> Timestamp,
        deletedAt -> Nullable<Timestamp>,
        meetingId -> Nullable<Int4>,
        createdById -> Nullable<Int4>,
        status -> Int4,
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
