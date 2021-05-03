use std::env;
use std::sync::Arc;
use dotenv::dotenv;
use crossroadsbot::db;
use tracing::{info, error};
use tracing_subscriber::{
    FmtSubscriber,
    EnvFilter,
};
use serenity::{
    prelude::*,
    async_trait,
    client::{
        Client,
        EventHandler
    },
    framework::{
        standard::{
            StandardFramework,
            CommandResult,
            macros::hook,
        }
    },
    model::prelude::*,
};
use dashmap::DashSet;
use crossroadsbot::commands;

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

#[hook]
async fn after(_ctx: &Context, _msg: &Message, command_name: &str, command_result: CommandResult) {
    match command_result {
        Ok(()) => info!("Processed command '{}'", command_name),
        Err(why) => error!("Command '{}' returned error {:?}", command_name, why),
    }
}


#[tokio::main]
async fn main() {

    // Load .env into ENV
    dotenv().ok();

    // Set up logging
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to start the logger");

    // Make a quick check to the database
    {
        db::connect();
    }

    let token = env::var("DISCORD_TOKEN")
        .expect("discord token not set");

    let manager_guild_id = GuildId::from(
        env::var("MANAGER_GUILD_ID")
        .expect("manager guild id not set")
        .parse::<u64>()
        .expect("Failed to parse manager guild id")
        );

    let framework = StandardFramework::new()
        .configure(|c| c
                   .prefix("~"))
        .after(after)
        .group(&commands::MISC_GROUP)
        .group(&commands::SIGNUP_GROUP)
        .group(&commands::CONFIG_GROUP);

    let mut client = Client::builder(token)
        .framework(framework)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<commands::ConversationLock>(Arc::new(DashSet::new()));
        data.insert::<commands::ConfigValuesData>(Arc::new(
            commands::ConfigValues {
                manager_guild_id: manager_guild_id,
            }));
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}
