DROP TRIGGER IF EXISTS trigger_update_room_latest_message ON messages;
DROP TRIGGER IF EXISTS trigger_users_updated_at ON users;
DROP TRIGGER IF EXISTS trigger_rooms_updated_at ON rooms;
DROP TRIGGER IF EXISTS trigger_messages_updated_at ON messages;

DROP FUNCTION IF EXISTS update_room_latest_message();
DROP FUNCTION IF EXISTS update_updated_at_column();

DROP INDEX IF EXISTS idx_users_user_name;
DROP INDEX IF EXISTS idx_users_external_id;
DROP INDEX IF EXISTS idx_users_last_seen;
DROP INDEX IF EXISTS idx_users_created_at;
DROP INDEX IF EXISTS idx_users_deleted_at;

DROP INDEX IF EXISTS idx_rooms_code;
DROP INDEX IF EXISTS idx_rooms_latest_message;
DROP INDEX IF EXISTS idx_rooms_status_type;
DROP INDEX IF EXISTS idx_rooms_created_at;
DROP INDEX IF EXISTS idx_rooms_deleted_at;

DROP INDEX IF EXISTS idx_members_user_room;
DROP INDEX IF EXISTS idx_members_user_id;
DROP INDEX IF EXISTS idx_members_room_id;
DROP INDEX IF EXISTS idx_members_role;
DROP INDEX IF EXISTS idx_members_created_at;
DROP INDEX IF EXISTS idx_members_deleted_at;

DROP INDEX IF EXISTS idx_messages_room_created;
DROP INDEX IF EXISTS idx_messages_created_by;
DROP INDEX IF EXISTS idx_messages_type_status;
DROP INDEX IF EXISTS idx_messages_created_at;
DROP INDEX IF EXISTS idx_messages_room_id;
DROP INDEX IF EXISTS idx_messages_deleted_at;

DROP INDEX IF EXISTS idx_participants_room_id;
DROP INDEX IF EXISTS idx_participants_user_id;
DROP INDEX IF EXISTS idx_participants_node_id;
DROP INDEX IF EXISTS idx_participants_status;
DROP INDEX IF EXISTS idx_participants_created_at;
DROP INDEX IF EXISTS idx_participants_deleted_at;

DROP TABLE IF EXISTS participants;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS members;
DROP TABLE IF EXISTS rooms;
DROP TABLE IF EXISTS users;
