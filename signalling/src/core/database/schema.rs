// @generated automatically by Diesel CLI.

diesel::table! {
    members (id) {
        id -> Int4,
        created_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        soft_deleted_at -> Nullable<Timestamp>,
        user_id -> Int4,
        room_id -> Int4,
        role -> Int2,
    }
}

diesel::table! {
    messages (id) {
        id -> Int4,
        data -> Text,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        created_by_id -> Int4,
        room_id -> Int4,
        #[sql_name = "type"]
        type_ -> Int2,
        status -> Int2,
    }
}

diesel::table! {
    participants (id) {
        id -> Int4,
        created_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        user_id -> Int4,
        room_id -> Int4,
        #[max_length = 100]
        node_id -> Nullable<Varchar>,
        status -> Int2,
    }
}

diesel::table! {
    rooms (id) {
        id -> Int4,
        #[max_length = 255]
        title -> Varchar,
        #[max_length = 255]
        password -> Nullable<Varchar>,
        #[max_length = 500]
        avatar -> Nullable<Varchar>,
        latest_message_created_at -> Nullable<Timestamp>,
        #[max_length = 20]
        code -> Varchar,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        latest_message_id -> Nullable<Int4>,
        status -> Int2,
        #[sql_name = "type"]
        type_ -> Int2,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        #[max_length = 255]
        full_name -> Nullable<Varchar>,
        #[max_length = 50]
        user_name -> Varchar,
        bio -> Nullable<Text>,
        #[max_length = 100]
        external_id -> Varchar,
        #[max_length = 500]
        avatar -> Nullable<Varchar>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        deleted_at -> Nullable<Timestamp>,
        last_seen_at -> Nullable<Timestamp>,
    }
}

diesel::joinable!(members -> rooms (room_id));
diesel::joinable!(members -> users (user_id));
diesel::joinable!(messages -> rooms (room_id));
diesel::joinable!(messages -> users (created_by_id));
diesel::joinable!(participants -> rooms (room_id));
diesel::joinable!(participants -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    members,
    messages,
    participants,
    rooms,
    users,
);
