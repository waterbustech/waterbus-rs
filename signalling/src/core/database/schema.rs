// @generated automatically by Diesel CLI.

diesel::table! {
    rooms (id) {
        id -> Int4,
        title -> Varchar,
        password -> Nullable<Varchar>,
        avatar -> Nullable<Varchar>,
        #[sql_name = "latestMessageCreatedAt"]
        latest_message_created_at -> Nullable<Timestamp>,
        code -> Varchar,
        #[sql_name = "createdAt"]
        created_at -> Timestamp,
        #[sql_name = "updatedAt"]
        updated_at -> Timestamp,
        #[sql_name = "deletedAt"]
        deleted_at -> Nullable<Timestamp>,
        #[sql_name = "latestMessageId"]
        latest_message_id -> Nullable<Int4>,
        status -> Int4,
        #[sql_name = "type"]
        type_ -> Int4,
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
        #[sql_name = "roomId"]
        room_id -> Nullable<Int4>,
        role -> Int4,
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
        #[sql_name = "roomId"]
        room_id -> Nullable<Int4>,
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
        #[sql_name = "roomId"]
        room_id -> Nullable<Int4>,
        #[sql_name = "nodeId"]
        node_id -> Nullable<Varchar>,
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
        #[sql_name = "externalId"]
        external_id -> Varchar,
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

diesel::joinable!(members -> rooms (room_id));
diesel::joinable!(members -> users (user_id));
diesel::joinable!(messages -> users (created_by_id));
diesel::joinable!(participants -> rooms (room_id));
diesel::joinable!(participants -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(rooms, members, messages, participants, users,);
