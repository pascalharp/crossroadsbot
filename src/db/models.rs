use crate::db::schema::{
    config, roles, signup_roles, signups, tier_mappings, tiers, training_roles, trainings, users,
};
use diesel_derive_enum::DbEnum;
use std::{fmt, str};

use chrono::naive::NaiveDateTime;

#[derive(Identifiable, Queryable, PartialEq, Debug)]
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
}

#[derive(Insertable, Debug)]
#[table_name = "signups"]
pub struct NewSignup {
    pub user_id: i32,
    pub training_id: i32,
}

#[derive(Debug, DbEnum, PartialEq, PartialOrd, Clone)]
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
    type Err = ();

    fn from_str(input: &str) -> Result<TrainingState, Self::Err> {
        match input {
            "created" => Ok(TrainingState::Created),
            "open" => Ok(TrainingState::Open),
            "closed" => Ok(TrainingState::Closed),
            "started" => Ok(TrainingState::Started),
            "finished" => Ok(TrainingState::Finished),
            _ => Err(()),
        }
    }
}

#[derive(Identifiable, Queryable, Associations, PartialEq, Debug)]
#[belongs_to(Tier)]
#[table_name = "trainings"]
pub struct Training {
    pub id: i32,
    pub title: String,
    pub date: NaiveDateTime,
    pub state: TrainingState,
    pub tier_id: Option<i32>,
}

#[derive(Insertable, Debug)]
#[table_name = "trainings"]
pub(super) struct NewTraining {
    pub title: String,
    pub date: NaiveDateTime,
    pub tier_id: Option<i32>,
}

#[derive(Identifiable, Queryable, Associations, Hash, PartialEq, Eq, Debug)]
#[table_name = "roles"]
pub struct Role {
    pub id: i32,
    pub title: String,
    pub repr: String,
    pub emoji: i64,
    pub active: bool,
}

#[derive(Insertable, Debug)]
#[table_name = "roles"]
pub struct NewRole {
    pub title: String,
    pub repr: String,
    pub emoji: i64,
}

#[derive(Identifiable, Queryable, Associations, PartialEq, Debug)]
#[belongs_to(Signup)]
#[belongs_to(Role)]
#[table_name = "signup_roles"]
pub struct SignupRole {
    pub id: i32,
    pub signup_id: i32,
    pub role_id: i32,
}

#[derive(Insertable, Debug)]
#[table_name = "signup_roles"]
pub struct NewSignupRole {
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
pub struct NewTrainingRole {
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
pub struct NewTier {
    pub name: String,
}

#[derive(Identifiable, Queryable, Associations, PartialEq, Debug)]
#[table_name = "tier_mappings"]
#[belongs_to(Tier)]
pub struct TierMapping {
    pub id: i32,
    pub tier_id: i32,
    pub discord_role_id: i64,
}

#[derive(Insertable, Debug)]
#[table_name = "tier_mappings"]
pub struct NewTierMapping {
    pub tier_id: i32,
    pub discord_role_id: i64,
}

#[derive(Queryable, Insertable)]
#[table_name = "config"]
pub struct Config {
    pub name: String,
    pub value: String,
}
