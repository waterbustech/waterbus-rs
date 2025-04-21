// @generated automatically by Diesel CLI.

diesel::table! {
    ccus (id) {
        id -> Int4,
        #[sql_name = "socketId"]
        socket_id -> Varchar,
        #[sql_name = "podName"]
        pod_name -> Varchar,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "userId"]
        user_id -> Nullable<Int4>,
    }
}

diesel::table! {
    meetings (id) {
        id -> Int4,
        title -> Varchar,
        password -> Varchar,
        avatar -> Nullable<Varchar>,
        #[sql_name = "latestMessageCreatedAt"]
        latest_message_created_at -> Nullable<Timestamp>,
        code -> Int4,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "updatedAt"]
        updated_at -> Timestamp,
        #[sql_name = "deletedAt"]
        deleted_at -> Nullable<Timestamp>,
        #[sql_name = "latestMessageId"]
        latest_message_id -> Nullable<Int4>,
        status -> Int4,
    }
}

diesel::table! {
    members (id) {
        id -> Int4,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "deletedAt"]
        deleted_at -> Nullable<Timestamp>,
        #[sql_name = "softDeletedAt"]
        soft_deleted_at -> Nullable<Timestamp>,
        #[sql_name = "userId"]
        user_id -> Nullable<Int4>,
        #[sql_name = "meetingId"]
        meeting_id -> Nullable<Int4>,
        role -> Int4,
        status -> Int4,
    }
}

diesel::table! {
    messages (id) {
        id -> Int4,
        data -> Varchar,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "updatedAt"]
        updated_at -> Timestamp,
        #[sql_name = "deletedAt"]
        deleted_at -> Nullable<Timestamp>,
        #[sql_name = "createdById"]
        created_by_id -> Nullable<Int4>,
        #[sql_name = "meetingId"]
        meeting_id -> Nullable<Int4>,
        #[sql_name = "type"]
        type_ -> Int4,
        status -> Int4,
    }
}

diesel::table! {
    participants (id) {
        id -> Int4,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "deletedAt"]
        deleted_at -> Nullable<Timestamp>,
        #[sql_name = "userId"]
        user_id -> Nullable<Int4>,
        #[sql_name = "meetingId"]
        meeting_id -> Nullable<Int4>,
        #[sql_name = "ccuId"]
        ccu_id -> Nullable<Int4>,
        status -> Int4,
    }
}

diesel::table! {
    #[sql_name = "record-tracks"]
    record_tracks (id) {
        id -> Int4,
        #[sql_name = "urlToVideo"]
        url_to_video -> Varchar,
        #[sql_name = "startTime"]
        start_time -> Varchar,
        #[sql_name = "endTime"]
        end_time -> Varchar,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "deletedAt"]
        deleted_at -> Nullable<Timestamp>,
        #[sql_name = "recordId"]
        record_id -> Nullable<Int4>,
        #[sql_name = "userId"]
        user_id -> Nullable<Int4>,
    }
}

diesel::table! {
    records (id) {
        id -> Int4,
        #[sql_name = "urlToVideo"]
        url_to_video -> Nullable<Varchar>,
        thumbnail -> Nullable<Varchar>,
        duration -> Int4,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "deletedAt"]
        deleted_at -> Nullable<Timestamp>,
        #[sql_name = "meetingId"]
        meeting_id -> Nullable<Int4>,
        #[sql_name = "createdById"]
        created_by_id -> Nullable<Int4>,
        status -> Int4,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        #[sql_name = "fullName"]
        full_name -> Nullable<Varchar>,
        #[sql_name = "userName"]
        user_name -> Varchar,
        bio -> Nullable<Varchar>,
        #[sql_name = "googleId"]
        google_id -> Nullable<Varchar>,
        #[sql_name = "customId"]
        custom_id -> Nullable<Varchar>,
        avatar -> Nullable<Varchar>,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "updatedAt"]
        updated_at -> Timestamp,
        #[sql_name = "deletedAt"]
        deleted_at -> Nullable<Timestamp>,
        #[sql_name = "lastSeenAt"]
        last_seen_at -> Nullable<Timestamp>,
    }
}

diesel::table! {
    #[sql_name = "white-boards"]
    white_boards (id) {
        id -> Int4,
        paints -> Text,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "deletedAt"]
        deleted_at -> Nullable<Timestamp>,
        #[sql_name = "meetingId"]
        meeting_id -> Nullable<Int4>,
    }
}

diesel::joinable!(ccus -> users (user_id));
diesel::joinable!(members -> meetings (meeting_id));
diesel::joinable!(members -> users (user_id));
diesel::joinable!(messages -> users (created_by_id));
diesel::joinable!(participants -> ccus (ccu_id));
diesel::joinable!(participants -> meetings (meeting_id));
diesel::joinable!(participants -> users (user_id));
diesel::joinable!(record_tracks -> records (record_id));
diesel::joinable!(record_tracks -> users (user_id));
diesel::joinable!(records -> meetings (meeting_id));
diesel::joinable!(records -> users (created_by_id));
diesel::joinable!(white_boards -> meetings (meeting_id));

diesel::allow_tables_to_appear_in_same_query!(
    ccus,
    meetings,
    members,
    messages,
    participants,
    record_tracks,
    records,
    users,
    white_boards,
);
