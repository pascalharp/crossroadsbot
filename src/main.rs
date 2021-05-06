use crossroadsbot::commands;
use crossroadsbot::db;
use dashmap::DashSet;
use dotenv::dotenv;
use serenity::{
    async_trait,
    client::{Client, EventHandler},
    framework::standard::{macros::hook, CommandResult, StandardFramework},
    model::prelude::*,
    prelude::*,
};
use std::env;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

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
async fn after(ctx: &Context, msg: &Message, command_name: &str, command_result: CommandResult) {
    let author = &msg.author;
    // Log to subscriber
    match command_result {
        Ok(()) => {
            info!(
                "{}",
                format!("{} used command {}: {}", author.id, command_name, "OK")
            );
            let log_info = {
                ctx.data
                    .read()
                    .await
                    .get::<commands::LogginConfigData>()
                    .unwrap()
                    .clone()
                    .read()
                    .await
                    .info
            };
            if let Some(chan) = log_info {
                chan.send_message(ctx, |m| {
                    m.embed(|e| {
                        e.description("[INFO] Command used");
                        e.field("User", Mention::from(author), true);
                        e.field("Command", command_name, true);
                        e
                    })
                })
                .await
                .ok();
            }
        }
        Err(why) => {
            error!(
                "{}",
                format!("{} used command {}: {}", author.name, command_name, why)
            );
            let err_info = {
                ctx.data
                    .read()
                    .await
                    .get::<commands::LogginConfigData>()
                    .unwrap()
                    .clone()
                    .read()
                    .await
                    .error
            };
            if let Some(chan) = err_info {
                chan.send_message(ctx, |m| {
                    m.embed(|e| {
                        e.description("[ERROR] Command failed");
                        e.field("User", Mention::from(author), true);
                        e.field("Command", command_name, true);
                        e.field("Error", why, false);
                        e
                    })
                })
                .await
                .ok();
            }
        }
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

    tracing::subscriber::set_global_default(subscriber).expect("Failed to start the logger");

    // Make a quick check to the database
    {
        db::connect();
    }

    let token = env::var("DISCORD_TOKEN").expect("discord token not set");

    let main_guild_id = GuildId::from(
        env::var("MAIN_GUILD_ID")
            .expect("MAIN_GUILD_ID not set")
            .parse::<u64>()
            .expect("Failed to parse manager guild id"),
    );

    let emoji_guild_id = GuildId::from(
        env::var("EMOJI_GUILD_ID")
            .expect("EMOJI_GUILD_ID not set")
            .parse::<u64>()
            .expect("Failed to parse emoji guild id"),
    );

    let admin_role_id = RoleId::from(
        env::var("ADMIN_ROLE_ID")
            .expect("ADMIN_ROLE_ID not set")
            .parse::<u64>()
            .expect("Failed to parse admin role id"),
    );

    let squadmaker_role_id = RoleId::from(
        env::var("SQUADMAKER_ROLE_ID")
            .expect("SQUADMAKER_ROLE_ID not set")
            .parse::<u64>()
            .expect("Failed to parse squadmaker role id"),
    );

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("~"))
        .after(after)
        .help(&commands::HELP_CMD)
        .group(&commands::MISC_GROUP)
        .group(&commands::ROLE_GROUP)
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
        data.insert::<commands::ConfigValuesData>(Arc::new(commands::ConfigValues {
            main_guild_id: main_guild_id,
            admin_role_id: admin_role_id,
            squadmaker_role_id: squadmaker_role_id,
            emoji_guild_id: emoji_guild_id,
        }));
        data.insert::<commands::LogginConfigData>(Arc::new(RwLock::new(commands::LogginConfig {
            info: None,
            error: None,
        })));
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}
