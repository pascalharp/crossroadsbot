use crate::db::schema::{
    config, roles, signup_board_channels, signup_roles, signups, tier_mappings, tiers,
    training_boss_mappings, training_bosses, training_roles, trainings, users,
};
use diesel_derive_enum::DbEnum;
use serde::Serialize;
use std::{fmt, str};

use chrono::naive::{NaiveDate, NaiveDateTime};

#[derive(Identifiable, Queryable, PartialEq, Debug, Serialize)]
#[table_name = "users"]
pub struct User {
    pub id: i32,
    pub discord_id: i64,
    pub gw2_id: String,
}

impl User {
    pub fn discord_id(&self) -> u64 {
        self.discord_id as u64
    }
}

#[derive(Insertable, AsChangeset, Debug)]
#[table_name = "users"]
pub(super) struct NewUser {
    pub discord_id: i64,
    pub gw2_id: String,
}

#[derive(Identifiable, Queryable, Associations, Clone, PartialEq, Debug)]
#[belongs_to(User)]
#[belongs_to(Training)]
#[table_name = "signups"]
pub struct Signup {
    pub id: i32,
    pub user_id: i32,
    pub training_id: i32,
    pub comment: Option<String>,
}

#[derive(Insertable, Debug)]
#[table_name = "signups"]
pub struct NewSignup {
    pub user_id: i32,
    pub training_id: i32,
}

#[derive(Debug, DbEnum, PartialEq, PartialOrd, Clone, Serialize)]
#[DieselType = "Training_state"]
pub enum TrainingState {
    Created,
    Open,
    Closed,
    Started,
    Finished,
}

impl fmt::Display for TrainingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrainingState::Created => write!(f, "created"),
            TrainingState::Open => write!(f, "open"),
            TrainingState::Closed => write!(f, "closed"),
            TrainingState::Started => write!(f, "started"),
            TrainingState::Finished => write!(f, "finished"),
        }
    }
}

impl str::FromStr for TrainingState {
    type Err = String;

    fn from_str(input: &str) -> Result<TrainingState, Self::Err> {
        match input {
            "created" => Ok(TrainingState::Created),
            "open" => Ok(TrainingState::Open),
            "closed" => Ok(TrainingState::Closed),
            "started" => Ok(TrainingState::Started),
            "finished" => Ok(TrainingState::Finished),
            e => Err(format!("unknown training state: {}", e)),
        }
    }
}

#[derive(Identifiable, Queryable, Associations, PartialEq, Debug, Serialize, Clone)]
#[belongs_to(Tier)]
#[table_name = "trainings"]
pub struct Training {
    pub id: i32,
    pub title: String,
    pub date: NaiveDateTime,
    pub state: TrainingState,
    pub tier_id: Option<i32>,
    pub board_message_id: Option<i64>,
}

#[derive(Insertable, Debug)]
#[table_name = "trainings"]
pub(super) struct NewTraining {
    pub title: String,
    pub date: NaiveDateTime,
    pub tier_id: Option<i32>,
}

#[derive(Identifiable, Queryable, Associations, Hash, PartialEq, Eq, Debug, Serialize)]
#[table_name = "roles"]
pub struct Role {
    pub id: i32,
    pub title: String,
    pub repr: String,
    pub emoji: i64,
    pub active: bool,
    pub priority: i16,
}

#[derive(Insertable, Debug)]
#[table_name = "roles"]
pub(super) struct NewRole {
    pub title: String,
    pub repr: String,
    pub emoji: i64,
    pub priority: Option<i16>,
}

#[derive(Identifiable, Queryable, Associations, PartialEq, Debug)]
#[belongs_to(Signup)]
#[belongs_to(Role)]
#[table_name = "signup_roles"]
#[primary_key(signup_id, role_id)]
pub struct SignupRole {
    pub signup_id: i32,
    pub role_id: i32,
}

#[derive(Insertable, Debug)]
#[table_name = "signup_roles"]
pub(super) struct NewSignupRole {
    pub signup_id: i32,
    pub role_id: i32,
}

#[derive(Identifiable, Queryable, Associations, PartialEq, Debug)]
#[belongs_to(Training)]
#[belongs_to(Role)]
#[table_name = "training_roles"]
#[primary_key(training_id, role_id)]
pub struct TrainingRole {
    pub training_id: i32,
    pub role_id: i32,
}

#[derive(Insertable, Debug)]
#[table_name = "training_roles"]
pub(super) struct NewTrainingRole {
    pub training_id: i32,
    pub role_id: i32,
}

#[derive(Identifiable, Queryable, PartialEq, Debug)]
#[table_name = "tiers"]
pub struct Tier {
    pub id: i32,
    pub name: String,
}

#[derive(Insertable, Debug)]
#[table_name = "tiers"]
pub(super) struct NewTier {
    pub name: String,
}

#[derive(Identifiable, Queryable, Associations, PartialEq, Debug)]
#[table_name = "tier_mappings"]
#[belongs_to(Tier)]
#[primary_key(tier_id, discord_role_id)]
pub struct TierMapping {
    pub tier_id: i32,
    pub discord_role_id: i64,
}

#[derive(Insertable, Debug)]
#[table_name = "tier_mappings"]
pub(super) struct NewTierMapping {
    pub tier_id: i32,
    pub discord_role_id: i64,
}

#[derive(Queryable, Insertable, Debug)]
#[table_name = "config"]
pub struct Config {
    pub name: String,
    pub value: String,
}

#[derive(Identifiable, Queryable, Insertable, Debug)]
#[table_name = "signup_board_channels"]
#[primary_key(day)]
pub struct SignupBoardChannel {
    pub day: NaiveDate,
    pub channel_id: i64,
}

#[derive(Identifiable, Queryable, Associations, Hash, PartialEq, Eq, Debug, Serialize)]
#[table_name = "training_bosses"]
pub struct TrainingBoss {
    pub id: i32,
    pub repr: String,
    pub name: String,
    pub wing: i32,
    pub position: i32,
    pub emoji: i64,
    pub url: Option<String>,
}

#[derive(Insertable, Associations, Debug)]
#[table_name = "training_bosses"]
pub struct NewTrainingBoss {
    pub repr: String,
    pub name: String,
    pub wing: i32,
    pub position: i32,
    pub emoji: i64,
    pub url: Option<String>,
}

#[derive(Insertable, Queryable, Associations, Debug, Hash, PartialEq, Eq)]
#[table_name = "training_boss_mappings"]
pub struct TrainingBossMapping {
    pub training_id: i32,
    pub training_boss_id: i32,
}
