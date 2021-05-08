use chrono::NaiveDateTime;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::result::QueryResult;
use std::env;

pub mod models;
pub mod schema;

pub use models::*;
use schema::*;

pub fn connect() -> PgConnection {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    PgConnection::establish(&database_url).expect(&format!("Error connecting to {}", database_url))
}

/* --- User --- */

pub fn get_user(conn: &PgConnection, discord_id: u64) -> QueryResult<User> {
    users::table
        .filter(users::discord_id.eq(discord_id as i64))
        .first::<User>(conn)
}

pub fn add_user(conn: &PgConnection, discord_id: u64, gw2_id: &str) -> QueryResult<User> {
    let user = NewUser {
        discord_id: discord_id as i64,
        gw2_id: gw2_id,
    };

    diesel::insert_into(users::table)
        .values(&user)
        .get_result(conn)
}

impl User {
    pub fn get_signups(&self, conn: &PgConnection) -> QueryResult<Vec<Signup>> {
        Signup::belonging_to(self).load(conn)
    }

    pub fn update_gw2_id(&self, conn: &PgConnection, gw2_id: &str) -> QueryResult<User> {
        diesel::update(users::table.find(self.id))
            .set(users::gw2_id.eq(gw2_id))
            .get_result(conn)
    }
}

/* -- Training -- */

pub fn add_training(
    conn: &PgConnection,
    title: &str,
    date: &NaiveDateTime,
) -> QueryResult<Training> {
    let training = NewTraining {
        title: title,
        date: date,
    };

    diesel::insert_into(trainings::table)
        .values(&training)
        .get_result(conn)
}

pub fn get_trainings_by_state(
    conn: &PgConnection,
    state: &TrainingState,
) -> QueryResult<Vec<Training>> {
    trainings::table
        .filter(trainings::state.eq(state))
        .load::<Training>(conn)
}

pub fn get_training_by_id(conn: &PgConnection, id: i32) -> QueryResult<Training> {
    trainings::table
        .filter(trainings::id.eq(id))
        .first::<Training>(conn)
}

impl Training {
    pub fn set_state(&self, conn: &PgConnection, state: &TrainingState) -> QueryResult<Training> {
        diesel::update(trainings::table.find(self.id))
            .set(trainings::state.eq(state))
            .get_result(conn)
    }

    pub fn get_signups(&self, conn: &PgConnection) -> QueryResult<Vec<Signup>> {
        Signup::belonging_to(self).load(conn)
    }

    pub fn add_role(&self, conn: &PgConnection, role_id: i32) -> QueryResult<TrainingRole> {
        let training_role = NewTrainingRole {
            training_id: self.id,
            role_id: role_id,
        };

        diesel::insert_into(training_roles::table)
            .values(&training_role)
            .get_result(conn)
    }

    pub fn get_roles(&self, conn: &PgConnection) -> QueryResult<Vec<TrainingRole>> {
        TrainingRole::belonging_to(self).load(conn)
    }
}

/* -- Signup -- */

pub fn add_signup(conn: &PgConnection, user: &User, training: &Training) -> QueryResult<Signup> {
    let signup = NewSignup {
        user_id: user.id,
        training_id: training.id,
    };

    diesel::insert_into(signups::table)
        .values(&signup)
        .get_result(conn)
}

impl Signup {
    pub fn get_training(self, conn: &PgConnection) -> QueryResult<Training> {
        trainings::table
            .filter(trainings::id.eq(self.training_id))
            .first::<Training>(conn)
    }
}

/* -- Role -- */

pub fn add_role(conn: &PgConnection, title: &str, repr: &str, emoji: u64) -> QueryResult<Role> {
    let role = NewRole {
        title: title,
        repr: repr,
        emoji: emoji as i64,
    };

    diesel::insert_into(roles::table)
        .values(&role)
        .get_result(conn)
}

impl Role {
    pub fn deactivate(self, conn: &PgConnection) -> QueryResult<Role> {
        diesel::update(roles::table.find(self.id))
            .set(roles::active.eq(false))
            .get_result(conn)
    }
}

pub fn rm_role(conn: &PgConnection, repr: &str) -> QueryResult<usize> {
    diesel::delete(roles::table.filter(roles::repr.eq(repr))).execute(conn)
}

pub fn get_roles(conn: &PgConnection) -> QueryResult<Vec<Role>> {
    roles::table
        .filter(roles::active.eq(true))
        .load::<Role>(conn)
}

pub fn get_role_by_emoji(conn: &PgConnection, emoji: u64) -> QueryResult<Role> {
    roles::table
        .filter(roles::active.eq(true))
        .filter(roles::emoji.eq(emoji as i64))
        .first::<Role>(conn)
}

pub fn get_role_by_repr(conn: &PgConnection, repr: &str) -> QueryResult<Role> {
    roles::table
        .filter(roles::active.eq(true))
        .filter(roles::repr.eq(repr))
        .first::<Role>(conn)
}

// --- TrainingRole ---

impl TrainingRole {
    /// Ignores deactivated roles. To load deactivated roles as well use
    /// role_unfilterd
    pub fn role(&self, conn: &PgConnection) -> QueryResult<Role> {
        roles::table
            .filter(roles::active.eq(true))
            .filter(roles::id.eq(self.role_id))
            .first::<Role>(conn)
    }

    /// Loads the role even if it is deactivated
    pub fn role_unfilterd(&self, conn: &PgConnection) -> QueryResult<Role> {
        roles::table
            .filter(roles::id.eq(self.role_id))
            .first::<Role>(conn)
    }
}

// --- Tier ---

pub fn add_tier(conn: &PgConnection, name: &str) -> QueryResult<Tier> {
    let new_tier = NewTier { name };

    diesel::insert_into(tiers::table)
        .values(&new_tier)
        .get_result(conn)
}

impl Tier {
    pub fn add_discord_role(
        &self,
        conn: &PgConnection,
        discord_id: u64,
    ) -> QueryResult<TierMapping> {
        let new_tier_mapping = NewTierMapping {
            tier_id: self.id,
            discord_role_id: discord_id as i64,
        };

        diesel::insert_into(tier_mappings::table)
            .values(&new_tier_mapping)
            .get_result(conn)
    }
}

// --- TierMapping ---
