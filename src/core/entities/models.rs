// Generated by diesel_ext

#![allow(unused)]
#![allow(clippy::all)]
#![allow(non_snake_case)]

use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel_derive_enum::DbEnum;
use serde::{Deserialize, Serialize};

use crate::core::database::schema::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "crate::core::database::schema::sql_types::MeetingsStatusEnum"]
pub enum MeetingsStatusEnum {
    Active,
    Archied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "crate::core::database::schema::sql_types::MembersRoleEnum"]
pub enum MembersRoleEnum {
    Host,
    Attendee,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "crate::core::database::schema::sql_types::MembersStatusEnum"]
pub enum MembersStatusEnum {
    Inviting,
    Invisible,
    Joined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "crate::core::database::schema::sql_types::MessagesTypeEnum"]
pub enum MessagesTypeEnum {
    Default,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "crate::core::database::schema::sql_types::MessagesStatusEnum"]
pub enum MessagesStatusEnum {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "crate::core::database::schema::sql_types::RecordsStatusEnum"]
pub enum RecordsStatusEnum {
    Recording,
    Processing,
    Finish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, DbEnum, Serialize, Deserialize)]
#[ExistingTypePath = "crate::core::database::schema::sql_types::ParticipantsStatusEnum"]
pub enum ParticipantsStatusEnum {
    Active,
    Inactive,
}

#[derive(Queryable, Selectable, Debug, Serialize, Deserialize, Clone)]
#[diesel(table_name = ccus)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Ccu {
    pub id: i32,
    pub socketId: String,
    pub podName: String,
    pub createdAt: NaiveDateTime,
    pub userId: Option<i32>,
}

#[derive(Queryable, Selectable, Debug, Clone, Serialize, Deserialize)]
#[diesel(table_name = meetings)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Meeting {
    pub id: i32,
    pub title: String,
    pub password: String,
    pub avatar: Option<String>,
    pub status: MeetingsStatusEnum,
    pub latestMessageCreatedAt: Option<NaiveDateTime>,
    pub code: i32,
    pub createdAt: NaiveDateTime,
    pub updatedAt: NaiveDateTime,
    pub deletedAt: Option<NaiveDateTime>,
    pub latestMessageId: Option<i32>,
}

#[derive(Queryable, Selectable, Debug, Clone, Serialize, Deserialize)]
#[diesel(table_name = members)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Member {
    pub id: i32,
    pub role: MembersRoleEnum,
    pub status: MembersStatusEnum,
    pub createdAt: NaiveDateTime,
    pub deletedAt: Option<NaiveDateTime>,
    pub softDeletedAt: Option<NaiveDateTime>,
    pub userId: Option<i32>,
    pub meetingId: Option<i32>,
}

#[derive(Queryable, Selectable, Debug, Clone, Serialize, Deserialize)]
#[diesel(table_name = messages)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Message {
    pub id: i32,
    pub data: String,
    pub type_: MessagesTypeEnum,
    pub status: MessagesStatusEnum,
    pub createdAt: NaiveDateTime,
    pub updatedAt: NaiveDateTime,
    pub deletedAt: Option<NaiveDateTime>,
    pub createdById: Option<i32>,
    pub meetingId: Option<i32>,
}

#[derive(Queryable, Selectable, Debug, Clone, Serialize, Deserialize)]
#[diesel(table_name = participants)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Participant {
    pub id: i32,
    pub status: ParticipantsStatusEnum,
    pub createdAt: NaiveDateTime,
    pub deletedAt: Option<NaiveDateTime>,
    pub userId: Option<i32>,
    pub meetingId: Option<i32>,
    pub ccuId: Option<i32>,
}

#[derive(Queryable, Selectable, Debug, Serialize, Deserialize, Clone)]
#[diesel(table_name = record_tracks)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RecordTrack {
    pub id: i32,
    pub urlToVideo: String,
    pub startTime: String,
    pub endTime: String,
    pub createdAt: NaiveDateTime,
    pub deletedAt: Option<NaiveDateTime>,
    pub recordId: Option<i32>,
    pub userId: Option<i32>,
}

#[derive(Queryable, Selectable, Debug, Serialize, Deserialize, Clone)]
#[diesel(table_name = records)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Record {
    pub id: i32,
    pub urlToVideo: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: i32,
    pub status: RecordsStatusEnum,
    pub createdAt: NaiveDateTime,
    pub deletedAt: Option<NaiveDateTime>,
    pub meetingId: Option<i32>,
    pub createdById: Option<i32>,
}

#[derive(Queryable, Selectable, Debug, Serialize, Deserialize, Clone)]
pub struct Session {
    pub id: i32,
    pub createdAt: NaiveDateTime,
    pub deletedAt: Option<NaiveDateTime>,
    pub userId: Option<i32>,
}

#[derive(Queryable, Selectable, Debug, Serialize, Deserialize, Clone)]
#[diesel(table_name = users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub id: i32,
    pub fullName: Option<String>,
    pub userName: String,
    pub bio: Option<String>,
    pub googleId: Option<String>,
    pub githubId: Option<String>,
    pub appleId: Option<String>,
    pub avatar: Option<String>,
    pub createdAt: NaiveDateTime,
    pub updatedAt: NaiveDateTime,
    pub deletedAt: Option<NaiveDateTime>,
    pub lastSeenAt: Option<NaiveDateTime>,
}

#[derive(Queryable, Selectable, Debug, Serialize, Deserialize, Clone)]
#[diesel(table_name = white_boards)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct WhiteBoard {
    pub id: i32,
    pub paints: String,
    pub createdAt: NaiveDateTime,
    pub deletedAt: Option<NaiveDateTime>,
    pub meetingId: Option<i32>,
}

#[derive(Insertable)]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    pub fullName: Option<&'a str>,
    pub userName: &'a str,
    pub bio: Option<&'a str>,
    pub googleId: Option<&'a str>,
    pub githubId: Option<&'a str>,
    pub appleId: Option<&'a str>,
    pub avatar: Option<&'a str>,
    pub createdAt: NaiveDateTime,
    pub updatedAt: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = messages)]
pub struct NewMessage<'a> {
    pub data: &'a str,
    pub createdById: Option<&'a i32>,
    pub meetingId: Option<&'a i32>,
    pub createdAt: NaiveDateTime,
    pub updatedAt: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = meetings)]
pub struct NewMeeting<'a> {
    pub title: &'a str,
    pub password: &'a str,
    pub createdAt: NaiveDateTime,
    pub updatedAt: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = members)]
pub struct NewMember<'a> {
    pub meetingId: &'a i32,
    pub createdAt: NaiveDateTime,
    pub userId: Option<i32>,
    pub status : MembersStatusEnum,
    pub role: MembersRoleEnum,
}

#[derive(Insertable)]
#[diesel(table_name = participants)]
pub struct NewParticipant<'a> {
    pub meetingId: &'a i32,
    pub userId: Option<i32>,
    pub createdAt: NaiveDateTime,
    pub status: ParticipantsStatusEnum,
}
