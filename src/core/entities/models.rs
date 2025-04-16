// Generated by diesel_ext

#![allow(clippy::all)]

use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::database::schema::*;

#[repr(i32)]
#[derive(Debug)]
pub enum MeetingsStatusEnum {
    Active,
    Archived,
}

impl TryFrom<i32> for MeetingsStatusEnum {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MeetingsStatusEnum::Active),
            1 => Ok(MeetingsStatusEnum::Archived),
            _ => Err(()),
        }
    }
}

#[repr(i32)]
#[derive(Debug)]
pub enum MembersRoleEnum {
    Host,
    Attendee,
}

#[repr(i32)]
#[derive(Debug)]
pub enum MembersStatusEnum {
    Inviting,
    Invisible,
    Joined,
}

impl TryFrom<i32> for MembersStatusEnum {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MembersStatusEnum::Inviting),
            1 => Ok(MembersStatusEnum::Invisible),
            2 => Ok(MembersStatusEnum::Joined),
            _ => Err(()),
        }
    }
}

#[repr(i32)]
#[derive(Debug)]
pub enum MessagesTypeEnum {
    Default,
    System,
}

#[repr(i32)]
#[derive(Debug)]
pub enum MessagesStatusEnum {
    Active,
    Inactive,
}

#[repr(i32)]
#[derive(Debug)]
pub enum RecordsStatusEnum {
    Recording,
    Processing,
    Finish,
}

#[repr(i32)]
#[derive(Debug)]
pub enum ParticipantsStatusEnum {
    Active,
    Inactive,
}

#[derive(
    Queryable, Selectable, Debug, Clone, Serialize, Deserialize, QueryableByName, Identifiable,
)]
#[diesel(table_name = ccus)]
#[serde(rename_all = "camelCase")]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Ccu {
    pub id: i32,
    pub socket_id: String,
    pub pod_name: String,
    pub created_at: NaiveDateTime,
    pub user_id: Option<i32>,
}

#[derive(
    Queryable, Selectable, Debug, Clone, Serialize, Deserialize, QueryableByName, Identifiable,
)]
#[diesel(table_name = meetings)]
#[serde(rename_all = "camelCase")]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Meeting {
    pub id: i32,
    pub title: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub avatar: Option<String>,
    pub status: i32,
    pub latest_message_created_at: Option<NaiveDateTime>,
    pub code: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub latest_message_id: Option<i32>,
}

#[derive(
    Queryable,
    Selectable,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    QueryableByName,
    Associations,
    Identifiable,
)]
#[diesel(table_name = members)]
#[diesel(belongs_to(Meeting))]
#[diesel(belongs_to(User))]
#[serde(rename_all = "camelCase")]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Member {
    pub id: i32,
    pub role: i32,
    pub status: i32,
    pub created_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub soft_deleted_at: Option<NaiveDateTime>,
    pub user_id: Option<i32>,
    pub meeting_id: Option<i32>,
}

#[derive(
    Queryable,
    Selectable,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    QueryableByName,
    Associations,
    Identifiable,
)]
#[diesel(table_name = messages)]
#[diesel(belongs_to(Meeting, foreign_key = meeting_id))]
#[diesel(belongs_to(User, foreign_key = created_by_id))]
#[serde(rename_all = "camelCase")]
#[diesel(primary_key(id))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Message {
    pub id: i32,
    pub data: String,
    pub type_: i32,
    pub status: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub created_by_id: Option<i32>,
    pub meeting_id: Option<i32>,
}

#[derive(
    Queryable,
    Selectable,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    QueryableByName,
    Associations,
    Identifiable,
)]
#[diesel(table_name = participants)]
#[serde(rename_all = "camelCase")]
#[diesel(belongs_to(Meeting))]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Participant {
    pub id: i32,
    pub status: i32,
    pub created_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub user_id: Option<i32>,
    pub meeting_id: Option<i32>,
    pub ccu_id: Option<i32>,
}

#[derive(
    Queryable,
    Selectable,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    QueryableByName,
    Associations,
    Identifiable,
)]
#[diesel(table_name = record_tracks)]
#[serde(rename_all = "camelCase")]
#[diesel(belongs_to(Record))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RecordTrack {
    pub id: i32,
    pub url_to_video: String,
    pub start_time: String,
    pub end_time: String,
    pub created_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub record_id: Option<i32>,
    pub user_id: Option<i32>,
}

#[derive(
    Queryable,
    Selectable,
    Debug,
    Clone,
    Serialize,
    Deserialize,
    QueryableByName,
    Associations,
    Identifiable,
)]
#[diesel(table_name = records)]
#[serde(rename_all = "camelCase")]
#[diesel(belongs_to(Meeting))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Record {
    pub id: i32,
    pub url_to_video: Option<String>,
    pub thumbnail: Option<String>,
    pub duration: i32,
    pub status: i32,
    pub created_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub meeting_id: Option<i32>,
    pub created_by_id: Option<i32>,
}

#[derive(
    Queryable, Selectable, Debug, Clone, Serialize, Deserialize, QueryableByName, Identifiable,
)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i32,
    pub created_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub user_id: Option<i32>,
}

#[derive(
    Queryable, Selectable, Debug, Clone, Serialize, Deserialize, QueryableByName, Identifiable,
)]
#[diesel(table_name = users)]
#[serde(rename_all = "camelCase")]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub id: i32,
    pub full_name: Option<String>,
    pub user_name: String,
    pub bio: Option<String>,
    #[serde(skip_serializing)]
    pub google_id: Option<String>,
    #[serde(skip_serializing)]
    pub custom_id: Option<String>,
    pub avatar: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub last_seen_at: Option<NaiveDateTime>,
}

#[derive(
    Queryable, Selectable, Debug, Clone, Serialize, Deserialize, QueryableByName, Identifiable,
)]
#[diesel(table_name = white_boards)]
#[serde(rename_all = "camelCase")]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct WhiteBoard {
    pub id: i32,
    pub paints: String,
    pub created_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub meeting_id: Option<i32>,
}

#[derive(Insertable)]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    pub full_name: Option<&'a str>,
    pub user_name: &'a str,
    pub bio: Option<&'a str>,
    pub google_id: Option<&'a str>,
    pub custom_id: Option<&'a str>,
    pub avatar: Option<&'a str>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = messages)]
pub struct NewMessage<'a> {
    pub data: &'a str,
    pub created_by_id: Option<&'a i32>,
    pub meeting_id: Option<&'a i32>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Insertable)]
#[diesel(table_name = meetings)]
pub struct NewMeeting<'a> {
    pub title: &'a str,
    pub password: &'a str,
    pub code: &'a i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub latest_message_created_at: NaiveDateTime,
    pub status: i32,
}

#[derive(Insertable)]
#[diesel(table_name = members)]
pub struct NewMember<'a> {
    pub meeting_id: &'a i32,
    pub created_at: NaiveDateTime,
    pub user_id: Option<i32>,
    pub status: i32,
    pub role: i32,
}

#[derive(Insertable)]
#[diesel(table_name = participants)]
pub struct NewParticipant<'a> {
    pub meeting_id: &'a i32,
    pub user_id: Option<i32>,
    pub created_at: NaiveDateTime,
    pub status: i32,
    pub ccu_id: Option<i32>,
}

#[derive(Insertable)]
#[diesel(table_name = ccus)]
pub struct NewCcu<'a> {
    pub socket_id: &'a str,
    pub pod_name: &'a str,
    pub created_at: NaiveDateTime,
    pub user_id: Option<i32>,
}
