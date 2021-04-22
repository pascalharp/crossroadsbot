use std::env;
use dotenv::dotenv;
use crossroadsbot::db;
use tracing::{error, info};
use tracing_subscriber::{
    FmtSubscriber,
    EnvFilter,
};

use serenity::{
    prelude::*,
    async_trait,
    client::{
        Client,
        //Context,
        EventHandler
    },
    framework::{
        standard::StandardFramework,
        standard::macros::group,
    },
    model::{
        event::ResumedEvent,
        gateway::Ready,
    },
};


use crossroadsbot::commands::{
    misc::*,
};

struct Handler;

#[async_trait]
impl EventHandler for Handler {

    async fn ready(&self, _: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }
}

#[group]
#[commands(ping,dudu)]
struct Misc;


#[tokio::main]
async fn main() {

    dotenv().ok();
    println!("Hello Crossroads!");

    tracing_subscriber::fmt::init();

    // Make a quick check to the database
    {
        db::connect();
    }

    let token = env::var("DISCORD_TOKEN")
        .expect("discord token not set");

    let framework = StandardFramework::new()
        .configure(|c| c
                   .prefix("~"))
        .group(&MISC_GROUP);

    let mut client = Client::builder(token)
        .framework(framework)
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        println!("An error occured while running the client: {:?}", why);
    }
}
