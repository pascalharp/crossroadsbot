//! # db
//! This file contains abstractions for diesel sql query calls. A global connection pool
//! is used to hold connections and allowing diesel calls to be move to a blocking thread
//! with tokio task::spawn_blocking to not block on the executer thread

use crate::data::DBPoolData;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::result::QueryResult;
use lazy_static::lazy_static;
use serenity::client::Context;
use serenity::model::id::UserId;
use std::env;
use std::sync::Arc;
use tokio::task;
use chrono::NaiveDateTime;

pub mod models;
pub mod schema;

pub use models::*;
use schema::*;

pub struct DBPool(Pool<ConnectionManager<PgConnection>>);

impl DBPool {
    pub fn new() -> Self {
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let manager = ConnectionManager::<PgConnection>::new(database_url);
        DBPool(Pool::new(manager).unwrap())
    }

    async fn load(ctx: &Context) -> Arc<Self> {
        ctx.data.read().await.get::<DBPoolData>().unwrap().clone()
    }

    fn conn(&self) -> PooledConnection<ConnectionManager<PgConnection>> {
        self.0.get().unwrap()
    }
}

// Insert und Upsert
async fn upsert_user(ctx: &Context, user: NewUser) -> QueryResult<User> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        diesel::insert_into(users::table)
            .values(&user)
            .on_conflict(users::discord_id)
            .do_update()
            .set(&user)
            .get_result(&pool.conn())
    })
    .await
    .unwrap()
}

async fn insert_training(ctx: &Context, t: NewTraining) -> QueryResult<Training> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        diesel::insert_into(trainings::table)
            .values(&t)
            .get_result(&pool.conn())
    })
    .await
    .unwrap()
}

async fn insert_training_role(ctx: &Context, tr: NewTrainingRole) -> QueryResult<TrainingRole> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        diesel::insert_into(training_roles::table)
            .values(&tr)
            .get_result(&pool.conn())
    })
    .await
    .unwrap()
}

// Select
async fn select_user_by_discord_id(ctx: &Context, discord_id: u64) -> QueryResult<User> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        users::table
            .filter(users::discord_id.eq(discord_id as i64))
            .first(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_all_signups_by_user(ctx: &Context, user_id: i32) -> QueryResult<Vec<Signup>> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        let join = signups::table
            .inner_join(users::table)
            .inner_join(trainings::table);
        join.filter(users::id.eq(user_id))
            .select(signups::all_columns)
            .load(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_joined_active_trainings_by_user(
    ctx: &Context,
    user_id: i32,
) -> QueryResult<Vec<Training>> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        let join = signups::table
            .inner_join(users::table)
            .inner_join(trainings::table);
        join.filter(users::id.eq(user_id))
            .filter(
                trainings::state
                    .eq(TrainingState::Open)
                    .or(trainings::state.eq(TrainingState::Closed))
                    .or(trainings::state.eq(TrainingState::Started)),
            )
            .select(trainings::all_columns)
            .load(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_active_signups_trainings_by_user(
    ctx: &Context,
    user_id: i32,
) -> QueryResult<Vec<(Signup, Training)>> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        let join = signups::table
            .inner_join(users::table)
            .inner_join(trainings::table);
        join.filter(users::id.eq(user_id))
            .filter(
                trainings::state
                    .eq(TrainingState::Open)
                    .or(trainings::state.eq(TrainingState::Closed))
                    .or(trainings::state.eq(TrainingState::Started)),
            )
            .select((signups::all_columns, trainings::all_columns))
            .load(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_training_by_id(ctx: &Context, id: i32) -> QueryResult<Training> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || trainings::table.find(id).first(&pool.conn()))
        .await
        .unwrap()
}

async fn select_trainings_by_state(
    ctx: &Context,
    state: TrainingState,
) -> QueryResult<Vec<Training>> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        trainings::table
            .filter(trainings::state.eq(state))
            .load(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_training_by_id_and_state(
    ctx: &Context,
    id: i32,
    state: TrainingState,
) -> QueryResult<Training> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        trainings::table
            .find(id)
            .filter(trainings::state.eq(state))
            .first::<Training>(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_active_trainings(ctx: &Context) -> QueryResult<Vec<Training>> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        trainings::table
            .filter(
                trainings::state
                    .eq(TrainingState::Open)
                    .or(trainings::state.eq(TrainingState::Closed))
                    .or(trainings::state.eq(TrainingState::Started)),
            )
            .load::<Training>(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_signups_by_training(ctx: &Context, id: i32) -> QueryResult<Vec<Signup>> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        let join = signups::table.inner_join(trainings::table);
        join.filter(trainings::id.eq(id))
            .select(signups::all_columns)
            .load(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_tier_by_id(ctx: &Context, id: i32) -> QueryResult<Tier> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || tiers::table.find(id).first(&pool.conn()))
        .await
        .unwrap()
}

async fn select_training_roles_by_training(
    ctx: &Context,
    id: i32,
) -> QueryResult<Vec<TrainingRole>> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        let join = training_roles::table.inner_join(trainings::table);
        join.filter(trainings::id.eq(id))
            .select(training_roles::all_columns)
            .load(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_roles_by_training(ctx: &Context, id: i32) -> QueryResult<Vec<Role>> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        let join = training_roles::table
            .inner_join(trainings::table)
            .inner_join(roles::table);
        join.filter(trainings::id.eq(id))
            .select(roles::all_columns)
            .load(&pool.conn())
    })
    .await
    .unwrap()
}

async fn select_active_roles_by_training(ctx: &Context, id: i32) -> QueryResult<Vec<Role>> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        let join = training_roles::table
            .inner_join(trainings::table)
            .inner_join(roles::table);
        join.filter(trainings::id.eq(id))
            .filter(roles::active.eq(true))
            .select(roles::all_columns)
            .load(&pool.conn())
    })
    .await
    .unwrap()
}

// Count
async fn count_trainings_by_state(ctx: &Context, state: TrainingState) -> QueryResult<i64> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        trainings::table
            .filter(trainings::state.eq(state))
            .count()
            .get_result(&pool.conn())
    })
    .await
    .unwrap()
}

// Update
async fn update_training_state(
    ctx: &Context,
    id: i32,
    state: TrainingState,
) -> QueryResult<Training> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        diesel::update(trainings::table.find(id))
            .set(trainings::state.eq(state))
            .get_result(&pool.conn())
    })
    .await
    .unwrap()
}

async fn update_training_tier(
    ctx: &Context,
    id: i32,
    tier_id: Option<i32>,
) -> QueryResult<Training> {
    let pool = DBPool::load(ctx).await;
    task::spawn_blocking(move || {
        diesel::update(trainings::table.find(id))
            .set(trainings::tier_id.eq(tier_id))
            .get_result(&pool.conn())
    })
    .await
    .unwrap()
}

// TODO remove once done refactoring
lazy_static! {
    /// Global connection pool for postgresql database. Lazily created on first use
    static ref POOL: Pool<ConnectionManager<PgConnection>> = {
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let manager = ConnectionManager::<PgConnection>::new(database_url);
        Pool::new(manager).unwrap()
    };
}

// TODO remove once done refactoring
/// Retrieves an Arc from the connection pool
pub fn get_connection() -> Pool<ConnectionManager<PgConnection>> {
    POOL.clone()
}

/* --- User --- */
impl User {
    pub async fn upsert(ctx: &Context, discord_id: u64, gw2_id: String) -> QueryResult<User> {
        let user = NewUser {
            discord_id: discord_id as i64,
            gw2_id,
        };
        upsert_user(ctx, user).await
    }

    pub async fn by_discord_id(ctx: &Context, id: UserId) -> QueryResult<User> {
        select_user_by_discord_id(ctx, *id.as_u64()).await
    }

    pub async fn joined_active_trainings(&self, ctx: &Context) -> QueryResult<Vec<Training>> {
        select_joined_active_trainings_by_user(ctx, self.id).await
    }

    pub async fn active_signups(&self, ctx: &Context) -> QueryResult<Vec<(Signup, Training)>> {
        select_active_signups_trainings_by_user(ctx, self.id).await
    }

    pub async fn all_signups(&self, ctx: &Context) -> QueryResult<Vec<Signup>> {
        select_all_signups_by_user(ctx, self.id).await
    }
}

/* -- Training -- */
impl Training {
    pub async fn insert(ctx: &Context, title: String, date: NaiveDateTime, tier_id: Option<i32>) -> QueryResult<Training> {
        let t = NewTraining {
            title,
            date,
            tier_id
        };
        insert_training(ctx, t).await
    }

    pub async fn by_state(ctx: &Context, state: TrainingState) -> QueryResult<Vec<Training>> {
        select_trainings_by_state(ctx, state).await
    }

    pub async fn all_active(ctx: &Context) -> QueryResult<Vec<Training>> {
        select_active_trainings(ctx).await
    }

    pub async fn amount_by_state(ctx: &Context, state: TrainingState) -> QueryResult<i64> {
        count_trainings_by_state(ctx, state).await
    }

    pub async fn by_id(ctx: &Context, id: i32) -> QueryResult<Training> {
        select_training_by_id(ctx, id).await
    }

    pub async fn by_id_and_state(
        ctx: &Context,
        id: i32,
        state: TrainingState,
    ) -> QueryResult<Training> {
        select_training_by_id_and_state(ctx, id, state).await
    }

    pub async fn set_state(self, ctx: &Context, state: TrainingState) -> QueryResult<Training> {
        update_training_state(ctx, self.id, state).await
    }

    pub async fn get_tier(&self, ctx: &Context) -> Option<QueryResult<Tier>> {
        match self.tier_id {
            None => None,
            Some(id) => Some(select_tier_by_id(ctx, id).await),
        }
    }

    pub async fn set_tier(&self, ctx: &Context, tier_id: Option<i32>) -> QueryResult<Training> {
        update_training_tier(ctx, self.id, tier_id).await
    }

    pub async fn get_signups(&self, ctx: &Context) -> QueryResult<Vec<Signup>> {
        select_signups_by_training(ctx, self.id).await
    }

    pub async fn add_role(&self, ctx: &Context, role_id: i32) -> QueryResult<TrainingRole> {
        let training_role = NewTrainingRole {
            training_id: self.id,
            role_id,
        };
        insert_training_role(ctx, training_role).await
    }

    pub async fn get_training_roles(&self, ctx: &Context) -> QueryResult<Vec<TrainingRole>> {
        select_training_roles_by_training(ctx, self.id).await
    }

    pub async fn all_roles(&self, ctx: &Context) -> QueryResult<Vec<Role>> {
        select_roles_by_training(ctx, self.id).await
    }

    pub async fn active_roles(&self, ctx: &Context) -> QueryResult<Vec<Role>> {
        select_active_roles_by_training(ctx, self.id).await
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

    pub async fn get_user(&self) -> QueryResult<User> {
        let user_id = self.user_id;
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            users::table
                .filter(users::id.eq(user_id))
                .first::<User>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn get_roles(self: Arc<Signup>) -> QueryResult<Vec<(SignupRole, Role)>> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            SignupRole::belonging_to(self.as_ref())
                .inner_join(roles::table)
                .load(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn clear_roles(self: Arc<Signup>) -> QueryResult<usize> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::delete(SignupRole::belonging_to(self.as_ref())).execute(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn by_user_and_training(u: &User, t: &Training) -> QueryResult<Signup> {
        let training_id = t.id;
        let user_id = u.id;
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            signups::table
                .filter(signups::user_id.eq(user_id))
                .filter(signups::training_id.eq(training_id))
                .first::<Signup>(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn remove(self) -> QueryResult<usize> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::delete(signups::table.filter(signups::id.eq(self.id)))
                .execute(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}

impl NewSignupRole {
    pub async fn add(self) -> QueryResult<SignupRole> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::insert_into(signup_roles::table)
                .values(self)
                .get_result::<SignupRole>(&pool.get().unwrap())
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

// Config
impl Config {
    pub async fn load(name: String) -> QueryResult<Config> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            config::table
                .filter(config::name.eq(&name))
                .first(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }

    pub async fn save(self) -> QueryResult<Config> {
        let pool = POOL.clone();
        task::spawn_blocking(move || {
            diesel::insert_into(config::table)
                .values(&self)
                .on_conflict(config::name)
                .do_update()
                .set(config::value.eq(&self.value))
                .get_result(&pool.get().unwrap())
        })
        .await
        .unwrap()
    }
}
