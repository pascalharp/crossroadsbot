use diesel::prelude::*;
use diesel::pg::PgConnection;
use diesel::result::QueryResult;
use dotenv::dotenv;
use std::env;
use chrono::NaiveDateTime;

pub mod models;
pub mod schema;

use schema::*;
use models::*;

pub fn connect() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    PgConnection::establish(&database_url)
        .expect(&format!("Error connecting to {}", database_url))
}

/* --- User --- */

pub fn get_user(conn: &PgConnection, discord_id: &str) -> QueryResult<User> {
    users::table
        .filter(users::discord_id.eq(discord_id))
        .first::<User>(conn)
}

pub fn add_user(conn: &PgConnection, discord_id: &str, gw2_id: &str) ->QueryResult<User> {

    let user = NewUser {
        discord_id: discord_id,
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
}

/* -- Training -- */

pub fn add_training(conn: &PgConnection, title: &str, date: &NaiveDateTime) -> QueryResult<Training> {

    let training = NewTraining {
        title: title,
        date: date,
    };

    diesel::insert_into(trainings::table)
        .values(&training)
        .get_result(conn)
}

pub fn get_open_trainings(conn: &PgConnection) -> QueryResult<Vec<Training>> {

    trainings::table
        .filter(trainings::open.eq(true))
        .load::<Training>(conn)
}

impl Training {

    pub fn open(&self, conn: &PgConnection) -> QueryResult<Training> {
        diesel::update(trainings::table.find(self.id))
            .set(trainings::open.eq(true))
            .get_result(conn)
    }

    pub fn close(&self, conn: &PgConnection) -> QueryResult<Training> {
        diesel::update(trainings::table.find(self.id))
            .set(trainings::open.eq(false))
            .get_result(conn)
    }

    pub fn get_signups(&self, conn: &PgConnection) -> QueryResult<Vec<Signup>> {
        Signup::belonging_to(self).load(conn)
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

    pub fn get_training(self, conn: &PgConnection) ->QueryResult<Training> {
        trainings::table
            .filter(trainings::id.eq(self.training_id))
            .first::<Training>(conn)
    }

}

/* -- Role -- */
/* TODO */
