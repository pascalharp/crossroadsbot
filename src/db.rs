//! # db
//! This file contains abstractions for diesel sql query calls. A global connection pool
//! is used to hold connections and allowing diesel calls to be move to a blocking thread
//! with tokio task::spawn_blocking to not block on the executer thread

use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::result::QueryResult;
use lazy_static::lazy_static;
use std::env;
use std::sync::Arc;
use tokio::task;

pub mod models;
pub mod schema;

pub use models::*;
use schema::*;

lazy_static! {
    /// Global connection pool for postgresql database. Lazily created on first use
    static ref POOL: Pool<ConnectionManager<PgConnection>> = {
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let manager = ConnectionManager::<PgConnection>::new(database_url);
        Pool::new(manager).unwrap()
    };
}

pub async fn pool_test() -> QueryResult<Vec<Role>> {
    let pool = POOL.clone();
    task::spawn_blocking(move || {
        let conn = pool.get().unwrap();
        roles::table
            .filter(roles::active.eq(true))
            .load::<Role>(&conn)
    })
    .await
    .unwrap()
}

/* --- User --- */

pub async fn get_user(discord_id: u64) -> QueryResult<User> {
    let pool = POOL.clone();
    task::spawn_blocking(move || {
        users::table
            .filter(users::discord_id.eq(discord_id as i64))
            .first::<User>(&pool.get().unwrap())
    })
    .await
    .unwrap()
}

pub async fn add_user(discord_id: u64, gw2_id: &str) -> QueryResult<User> {
    let pool = POOL.clone();
    let gw2_id = String::from(gw2_id);
    task::spawn_blocking(move || {
        let user = NewUser {
            discord_id: discord_id as i64,
            gw2_id: &gw2_id,
        };

        diesel::insert_into(users::table)
            .values(&user)
            .get_result(&pool.get().unwrap())
    })
    .await
    .unwrap()
}

impl User {
    pub async fn get_signups(self: Arc<User>) -> QueryResult<Vec<Signup>> {
        let pool = POOL.clone();
        task::spawn_blocking(move || Signup::belonging_to(self.as_ref()).load(&pool.get().unwrap()))
            .await
            .unwrap()
    }

    pub async fn update_gw2_id(self: Arc<User>, gw2_id: &str) -> QueryResult<User> {
        let pool = POOL.clone();
        let gw2_id = String::from(gw2_id);
        task::spawn_blocking(move || {
            diesel::update(users::table.find(self.id))
                .set(users::gw2_id.eq(gw2_id))
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

/* -- Training -- */

impl Training {
    pub async fn by_state(state: TrainingState) -> QueryResult<Vec<Training>> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            trainings::table
                .filter(trainings::state.eq(state))
                .load::<Training>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn by_id(id: i32) -> QueryResult<Training> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            trainings::table
                .filter(trainings::id.eq(id))
                .first::<Training>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn set_state(&self, state: TrainingState) -> QueryResult<Training> {
        let training_id = self.id;
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::update(trainings::table.find(training_id))
                .set(trainings::state.eq(state))
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn set_tier(&self, tier: Option<i32>) -> QueryResult<Training> {
        let training_id = self.id;
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::update(trainings::table.find(training_id))
                .set(trainings::tier_id.eq(tier))
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn get_signups(self: Arc<Training>) -> QueryResult<Vec<Signup>> {
        let pool = POOL.clone();
        task::spawn_blocking(move || Signup::belonging_to(self.as_ref()).load(&pool.get().unwrap()))
            .await
            .unwrap()
    }

    pub async fn add_role(&self, role_id: i32) -> QueryResult<TrainingRole> {
        let training_role = NewTrainingRole {
            training_id: self.id,
            role_id: role_id,
        };
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::insert_into(training_roles::table)
                .values(&training_role)
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn get_roles(self: Arc<Training>) -> QueryResult<Vec<TrainingRole>> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            TrainingRole::belonging_to(self.as_ref()).load(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn get_tier(&self) -> QueryResult<Option<Tier>> {
        let tier_id = match self.tier_id {
            None => return Ok(None),
            Some(t) => t,
        };

        let pool = POOL.clone();
        task::spawn_blocking(move || {
            let tier = tiers::table
                .filter(tiers::id.eq(tier_id))
                .get_result::<Tier>(&pool.get().unwrap());

            match tier {
                Err(e) => Err(e),
                Ok(t) => Ok(Some(t)),
            }
        })
        .await
        .unwrap()
    }
}

impl NewTraining {
    pub async fn add(self: NewTraining) -> QueryResult<Training> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::insert_into(trainings::table)
                .values(&self)
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

/* -- Signup -- */

impl Signup {
    pub async fn get_training(&self) -> QueryResult<Training> {
        let training_id = self.training_id;
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            trainings::table
                .filter(trainings::id.eq(training_id))
                .first::<Training>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

impl NewSignup {
    pub async fn add(self) -> QueryResult<Signup> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::insert_into(signups::table)
                .values(&self)
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

/* -- Role -- */

impl Role {
    /// Deactivates the role but keeps it in database
    pub async fn deactivate(self) -> QueryResult<Role> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::update(roles::table.find(self.id))
                .set(roles::active.eq(false))
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    /// Loads all active roles
    pub async fn all() -> QueryResult<Vec<Role>> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            roles::table
                .filter(roles::active.eq(true))
                .load::<Role>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    /// Loads the current active role associated with provided emoji
    pub async fn by_emoji(emoji: u64) -> QueryResult<Role> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            roles::table
                .filter(roles::active.eq(true))
                .filter(roles::emoji.eq(emoji as i64))
                .first::<Role>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    /// Loads the current active role with specified repr
    pub async fn by_repr(repr: String) -> QueryResult<Role> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            roles::table
                .filter(roles::active.eq(true))
                .filter(roles::repr.eq(repr))
                .first::<Role>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

impl NewRole {
    pub async fn add(self) -> QueryResult<Role> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::insert_into(roles::table)
                .values(&self)
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

// --- TrainingRole ---

impl TrainingRole {
    /// Ignores deactivated roles. To load deactivated roles as well use
    /// role_unfilterd
    pub async fn role(&self) -> QueryResult<Role> {
        let role_id = self.role_id;
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            roles::table
                .filter(roles::active.eq(true))
                .filter(roles::id.eq(role_id))
                .first::<Role>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    /// Like role() but also loads deactivated roles
    pub async fn role_unfilterd(&self) -> QueryResult<Role> {
        let role_id = self.role_id;
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            roles::table
                .filter(roles::id.eq(role_id))
                .first::<Role>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

// --- Tier ---
impl Tier {
    pub async fn all() -> QueryResult<Vec<Tier>> {
        let pool = POOL.clone();
        task::spawn_blocking(move || tiers::table.load::<Tier>(&pool.get().unwrap()))
            .await
            .unwrap()
    }

    pub async fn by_name(name: String) -> QueryResult<Tier> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            tiers::table
                .filter(tiers::name.eq(name))
                .first::<Tier>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn add_discord_role(&self, discord_id: u64) -> QueryResult<TierMapping> {
        let pool = POOL.clone();
        let new_tier_mapping = NewTierMapping {
            tier_id: self.id,
            discord_role_id: discord_id as i64,
        };

        task::spawn_blocking(move || {
            diesel::insert_into(tier_mappings::table)
                .values(&new_tier_mapping)
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn delete(self) -> QueryResult<usize> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::delete(tiers::table.filter(tiers::id.eq(self.id))).execute(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn get_discord_roles(self: Arc<Tier>) -> QueryResult<Vec<TierMapping>> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            TierMapping::belonging_to(self.as_ref()).load(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn get_trainings(self: Arc<Tier>) -> QueryResult<Vec<Training>> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            Training::belonging_to(self.as_ref()).load(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

impl NewTier {
    pub async fn add(self) -> QueryResult<Tier> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::insert_into(tiers::table)
                .values(&self)
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

// --- TierMapping ---

impl TierMapping {
    pub async fn delete(self) -> QueryResult<usize> {
        let pool = POOL.clone();
        let id = self.id;
        task::spawn_blocking(move || {
            diesel::delete(tier_mappings::table.filter(tier_mappings::id.eq(id)))
                .execute(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}
