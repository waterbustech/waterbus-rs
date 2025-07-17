CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    full_name VARCHAR(255),
    user_name VARCHAR(50) NOT NULL,
    bio TEXT,
    external_id VARCHAR(100) NOT NULL,
    avatar VARCHAR(500),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP,
    last_seen_at TIMESTAMP
);

CREATE TABLE rooms (
    id SERIAL PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    password VARCHAR(255),
    avatar VARCHAR(500),
    latest_message_created_at TIMESTAMP,
    code VARCHAR(20) NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP,
    latest_message_id INTEGER,
    status SMALLINT NOT NULL DEFAULT 1,
    type SMALLINT NOT NULL DEFAULT 1
);

CREATE TABLE members (
    id SERIAL PRIMARY KEY,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP,
    soft_deleted_at TIMESTAMP,
    user_id INTEGER NOT NULL,
    room_id INTEGER NOT NULL,
    role SMALLINT NOT NULL DEFAULT 1,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE CASCADE
);

CREATE TABLE messages (
    id SERIAL PRIMARY KEY,
    data TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP,
    created_by_id INTEGER NOT NULL,
    room_id INTEGER NOT NULL,
    type SMALLINT NOT NULL DEFAULT 1,
    status SMALLINT NOT NULL DEFAULT 1,
    FOREIGN KEY (created_by_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE CASCADE
);

CREATE TABLE participants (
    id SERIAL PRIMARY KEY,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP,
    user_id INTEGER NOT NULL,
    room_id INTEGER NOT NULL,
    node_id VARCHAR(100),
    status SMALLINT NOT NULL DEFAULT 1,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(id) ON DELETE CASCADE
);

-- Users table indexes
CREATE UNIQUE INDEX idx_users_user_name ON users(user_name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX idx_users_external_id ON users(external_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_users_last_seen ON users(last_seen_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_users_created_at ON users(created_at);
CREATE INDEX idx_users_deleted_at ON users(deleted_at) WHERE deleted_at IS NOT NULL;

-- Rooms table indexes
CREATE UNIQUE INDEX idx_rooms_code ON rooms(code) WHERE deleted_at IS NULL;
CREATE INDEX idx_rooms_latest_message ON rooms(latest_message_created_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_rooms_status_type ON rooms(status, type) WHERE deleted_at IS NULL;
CREATE INDEX idx_rooms_created_at ON rooms(created_at);
CREATE INDEX idx_rooms_deleted_at ON rooms(deleted_at) WHERE deleted_at IS NOT NULL;

-- Members table indexes
CREATE UNIQUE INDEX idx_members_user_room ON members(user_id, room_id) WHERE deleted_at IS NULL AND soft_deleted_at IS NULL;
CREATE INDEX idx_members_user_id ON members(user_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_members_room_id ON members(room_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_members_role ON members(role) WHERE deleted_at IS NULL;
CREATE INDEX idx_members_created_at ON members(created_at);
CREATE INDEX idx_members_deleted_at ON members(deleted_at) WHERE deleted_at IS NOT NULL;

-- Messages table indexes
CREATE INDEX idx_messages_room_created ON messages(room_id, created_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_messages_created_by ON messages(created_by_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_messages_type_status ON messages(type, status) WHERE deleted_at IS NULL;
CREATE INDEX idx_messages_created_at ON messages(created_at);
CREATE INDEX idx_messages_room_id ON messages(room_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_messages_deleted_at ON messages(deleted_at) WHERE deleted_at IS NOT NULL;

-- Participants table indexes
CREATE INDEX idx_participants_room_id ON participants(room_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_participants_user_id ON participants(user_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_participants_node_id ON participants(node_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_participants_status ON participants(status) WHERE deleted_at IS NULL;
CREATE INDEX idx_participants_created_at ON participants(created_at);
CREATE INDEX idx_participants_deleted_at ON participants(deleted_at) WHERE deleted_at IS NOT NULL;

-- Update latest message trigger for rooms
CREATE OR REPLACE FUNCTION update_room_latest_message()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE rooms 
        SET latest_message_id = NEW.id,
            latest_message_created_at = NEW.created_at,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = NEW.room_id;
        RETURN NEW;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_room_latest_message
    AFTER INSERT ON messages
    FOR EACH ROW
    EXECUTE FUNCTION update_room_latest_message();

-- Update updated_at column triggers
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_rooms_updated_at
    BEFORE UPDATE ON rooms
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER trigger_messages_updated_at
    BEFORE UPDATE ON messages
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
