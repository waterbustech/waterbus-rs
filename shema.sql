CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    "fullName" VARCHAR,
    "userName" VARCHAR NOT NULL,
    bio VARCHAR,
    "googleId" VARCHAR,
    "customId" VARCHAR,
    avatar VARCHAR,
    "createdAt" TIMESTAMP NOT NULL,
    "updatedAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "lastSeenAt" TIMESTAMP
);

CREATE TABLE ccus (
    id SERIAL PRIMARY KEY,
    "socketId" VARCHAR NOT NULL,
    "podName" VARCHAR NOT NULL,
    "createdAt" TIMESTAMP NOT NULL,
    "userId" INTEGER REFERENCES users(id)
);

CREATE TABLE meetings (
    id SERIAL PRIMARY KEY,
    title VARCHAR NOT NULL,
    password VARCHAR NOT NULL,
    avatar VARCHAR,
    "latestMessageCreatedAt" TIMESTAMP,
    code INTEGER NOT NULL,
    "createdAt" TIMESTAMP NOT NULL,
    "updatedAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "latestMessageId" INTEGER,
    status INTEGER NOT NULL
);

CREATE TABLE members (
    id SERIAL PRIMARY KEY,
    "createdAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "softDeletedAt" TIMESTAMP,
    "userId" INTEGER REFERENCES users(id),
    "meetingId" INTEGER REFERENCES meetings(id),
    role INTEGER NOT NULL,
    status INTEGER NOT NULL
);

CREATE TABLE messages (
    id SERIAL PRIMARY KEY,
    data VARCHAR NOT NULL,
    "createdAt" TIMESTAMP NOT NULL,
    "updatedAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "createdById" INTEGER REFERENCES users(id),
    "meetingId" INTEGER REFERENCES meetings(id),
    "type" INTEGER NOT NULL,
    status INTEGER NOT NULL
);

CREATE TABLE participants (
    id SERIAL PRIMARY KEY,
    "createdAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "userId" INTEGER REFERENCES users(id),
    "meetingId" INTEGER REFERENCES meetings(id),
    "ccuId" INTEGER REFERENCES ccus(id),
    status INTEGER NOT NULL
);

CREATE TABLE records (
    id SERIAL PRIMARY KEY,
    "urlToVideo" VARCHAR,
    thumbnail VARCHAR,
    duration INTEGER NOT NULL,
    "createdAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "meetingId" INTEGER REFERENCES meetings(id),
    "createdById" INTEGER REFERENCES users(id),
    status INTEGER NOT NULL
);

CREATE TABLE "record-tracks" (
    id SERIAL PRIMARY KEY,
    "urlToVideo" VARCHAR NOT NULL,
    "startTime" VARCHAR NOT NULL,
    "endTime" VARCHAR NOT NULL,
    "createdAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "recordId" INTEGER REFERENCES records(id),
    "userId" INTEGER REFERENCES users(id)
);

CREATE TABLE "white-boards" (
    id SERIAL PRIMARY KEY,
    paints TEXT NOT NULL,
    "createdAt" TIMESTAMP NOT NULL,
    "deletedAt" TIMESTAMP,
    "meetingId" INTEGER REFERENCES meetings(id)
);
