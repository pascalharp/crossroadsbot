#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate lazy_static;
extern crate serenity;

pub mod commands;
pub mod conversation;
pub mod data;
pub mod db;
pub mod utils;
pub mod embeds;
