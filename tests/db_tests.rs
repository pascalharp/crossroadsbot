use diesel::prelude::*;
use diesel::pg::PgConnection;
use dotenv::dotenv;
use std::env;
use chrono::{
    NaiveDate,
};


use crossroadsbot::db;
use crossroadsbot::db::models::*;

#[macro_use]
extern crate diesel_migrations;
use diesel_migrations::embed_migrations;

embed_migrations!("migrations/");

struct TestDB {
    base_url: String,
    db_name: String,
}

impl TestDB {

    fn new(db_name: &str) -> Self {

        dotenv().ok();

        // Create Testing Database
        let base_url = env::var("TEST_BASE_URL")
            .expect("TEST_BASE_URL must be set");
        let conn = PgConnection::establish(&base_url)
            .expect("Failed to connect to database");
        let query = diesel::sql_query(
            format!("CREATE DATABASE {}", db_name).as_str());
        query
            .execute(&conn)
            .expect("Failed to create database");

        // Run Migrations on Testing Database
        let conn = PgConnection::establish( &format!("{}/{}", base_url, db_name))
            .expect("Failed to connect to database");
        embedded_migrations::run(&conn)
            .expect("Failed to run migrations on test database");

        Self {
            base_url: base_url.to_string(),
            db_name: db_name.to_string(),
        }
    }

    fn connect(&self) -> PgConnection {
        PgConnection::establish(
            &format!("{}/{}", self.base_url, self.db_name))
            .expect("Failed to connect to database")
    }
}

impl Drop for TestDB {

    fn drop(&mut self) {
        let pg_url = format!("{}/postgres", self.base_url);
        let conn = PgConnection::establish(&pg_url)
            .expect("Failed to connect to database");

        let disconnect_users = format!(
            "SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE datname = '{}';",
            self.db_name
        );
        diesel::sql_query(disconnect_users.as_str())
            .execute(&conn)
            .unwrap();

        diesel::sql_query(
            format!("DROP DATABASE {}", self.db_name).as_str())
            .execute(&conn)
            .expect("Failed to drop database");

    }
}

#[test]
fn dummy_database() {
    let db = TestDB::new("dummydb");
    db.connect();
}

#[test]
fn add_user() {
    let db =TestDB::new("test_adduser");
    let conn = db.connect();
    db::add_user(&conn, 1234, "gw2ID").expect("Failed to add user");
}

#[test]
fn get_user_inval() {
    let db = TestDB::new("test_getuserinval");
    let conn = db.connect();
    db::get_user(&conn, 0)
        .expect_err("Should not return a user");
}

#[test]
fn add_get_user() {
    let db = TestDB::new("test_addgetuser");
    let conn = db.connect();

    let d_id = std::u64::MAX;
    let g_id = "MAX user";
    let insert = db::add_user(&conn, std::u64::MAX, g_id)
        .expect("Failed to add user");
    let read = db::get_user(&conn, d_id)
        .expect("Failed to read added user");

    assert_eq!(d_id, insert.discord_id());
    assert_eq!(g_id, insert.gw2_id);
    assert_eq!(d_id, read.discord_id());
    assert_eq!(g_id, read.gw2_id);

    let d_id = std::u64::MIN;
    let g_id = "MIN user";
    let insert = db::add_user(&conn, std::u64::MIN, g_id)
        .expect("Failed to add user");
    let read = db::get_user(&conn, d_id)
        .expect("Failed to read added user");

    assert_eq!(d_id, insert.discord_id());
    assert_eq!(g_id, insert.gw2_id);
    assert_eq!(d_id, read.discord_id());
    assert_eq!(g_id, read.gw2_id);
}

#[test]
fn training() {
    let db = TestDB::new("test_addtraining");

    let title = "Beginner Training";
    let date = NaiveDate::from_ymd(2021, 04, 20).and_hms(19, 00, 00);

    let training = {
        let conn = db.connect();
        db::add_training(&conn, title, &date).unwrap()
    };

    assert_eq!(title, training.title);
    assert_eq!(date, training.date);
    assert_eq!(false, training.open);

    let training = {
        let conn = db.connect();
        training.open(&conn).unwrap()
    };

    assert_eq!(title, training.title);
    assert_eq!(date, training.date);
    assert_eq!(true, training.open);

    let training = {
        let conn = db.connect();
        training.close(&conn).unwrap()
    };

    assert_eq!(title, training.title);
    assert_eq!(date, training.date);
    assert_eq!(false, training.open);
}

#[test]
fn open_trainings() {
    let db = TestDB::new("test_opentraining");
    let conn = db.connect();

    let t1 = db::add_training(
        &conn,
        "Beginner Training",
        &NaiveDate::from_ymd(2021, 04, 20).and_hms(19, 00, 00)
        ).unwrap();

    let t2 = db::add_training(
        &conn,
        "Intermediate Training",
        &NaiveDate::from_ymd(2021, 06, 20).and_hms(19, 00, 00)
        ).unwrap();

    let t3 = db::add_training(
        &conn,
        "Advanced Training",
        &NaiveDate::from_ymd(2021, 08, 20).and_hms(19, 00, 00)
        ).unwrap();

    assert_eq!(0, db::get_open_trainings(&conn).unwrap().len());

    t1.open(&conn).unwrap();
    assert_eq!(1, db::get_open_trainings(&conn).unwrap().len());

    t2.open(&conn).unwrap();
    assert_eq!(2, db::get_open_trainings(&conn).unwrap().len());

    t3.open(&conn).unwrap();
    assert_eq!(3, db::get_open_trainings(&conn).unwrap().len());
}

#[test]
fn invalid_signup() {
    let db = TestDB::new("test_invalsignup");

    // Create User and Training manually that are not in db

    let user = User {
        id: 666,
        discord_id: 1234,
        gw2_id: String::from("notagw2id"),
    };

    let training = Training {
        id: 999,
        title: String::from("Imaginary Training"),
        date: NaiveDate::from_ymd(2021, 04, 19).and_hms(09, 21, 30),
        open: false,
    };

    let conn = db.connect();
    db::add_signup(&conn, &user, &training).expect_err("Should have failed");
}

#[test]
fn signup() {
    let db = TestDB::new("test_signup");
    let conn = db.connect();

    let user1 = db::add_user(&conn, 1234, "gw2ID").unwrap();
    let user2 = db::add_user(&conn, 4321, "BAR").unwrap();
    let training = db::add_training(
        &conn,
        "Some Training",
        &NaiveDate::from_ymd(2021, 04, 19).and_hms(09, 21, 30)
        ).unwrap();

    let signup1 = db::add_signup(&conn, &user1, &training).unwrap();
    let signup2 = db::add_signup(&conn, &user2, &training).unwrap();

    let all_signups = vec![signup1, signup2];
    assert_eq!(all_signups, training.get_signups(&conn).unwrap());
}
