CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    "fullName" VARCHAR,
    "userName" VARCHAR NOT NULL,
    bio VARCHAR,
    "externalId" VARCHAR NOT NULL,
    avatar VARCHAR,
    "createdAt" TIMESTAMP NOT NULL,
    "updatedAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "lastSeenAt" TIMESTAMP
);

CREATE TABLE rooms (
    id SERIAL PRIMARY KEY,
    title VARCHAR NOT NULL,
    password VARCHAR,
    avatar VARCHAR,
    "latestMessageCreatedAt" TIMESTAMP,
    code VARCHAR NOT NULL UNIQUE,
    "createdAt" TIMESTAMP NOT NULL,
    "updatedAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "latestMessageId" INTEGER,
    status INTEGER NOT NULL,
    "type" INTEGER NOT NULL
);

CREATE TABLE members (
    id SERIAL PRIMARY KEY,
    "createdAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "softDeletedAt" TIMESTAMP,
    "userId" INTEGER,
    "roomId" INTEGER,
    role INTEGER NOT NULL,
    FOREIGN KEY ("userId") REFERENCES users(id),
    FOREIGN KEY ("roomId") REFERENCES rooms(id)
);

CREATE TABLE messages (
    id SERIAL PRIMARY KEY,
    data VARCHAR NOT NULL,
    "createdAt" TIMESTAMP NOT NULL,
    "updatedAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "createdById" INTEGER,
    "roomId" INTEGER,
    "type" INTEGER NOT NULL,
    status INTEGER NOT NULL,
    FOREIGN KEY ("createdById") REFERENCES users(id),
    FOREIGN KEY ("roomId") REFERENCES rooms(id)
);

CREATE TABLE participants (
    id SERIAL PRIMARY KEY,
    "createdAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "userId" INTEGER,
    "roomId" INTEGER,
    "nodeId" VARCHAR,
    status INTEGER NOT NULL,
    FOREIGN KEY ("userId") REFERENCES users(id),
    FOREIGN KEY ("roomId") REFERENCES rooms(id)
);
