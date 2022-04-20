#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate serenity;

mod data;
mod db;
mod embeds;
mod interactions;
mod logging;
mod signup_board;
mod slash_commands;
mod status;
mod tasks;

use anyhow::bail;
use data::*;
use logging::{log_discord, LogInfo};
//use crate::logging;
//use crate::{
//    data::*, db, interactions, logging::*, signup_board::*, slash_commands, status, tasks,
//};
use diesel::prelude::*;
use diesel::{pg::PgConnection, result::Error::NotFound};
use dotenv::dotenv;
use serenity::{
    async_trait,
    client::{Client, EventHandler},
    model::prelude::*,
    prelude::*,
};
use signup_board::SignupBoard;
use std::{
    env,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[macro_use]
extern crate diesel_migrations;
use diesel_migrations::embed_migrations;
embed_migrations!("migrations/");

struct Handler {
    signup_board_loop_running: AtomicBool,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
        info!("Refreshing config values");

        let log_channel = db::Config::load(&ctx, String::from(INFO_LOG_NAME))
            .await
            .ok();

        let data_read = ctx.data.read().await;
        let mut log_write = data_read.get::<LogConfigData>().unwrap().write().await;

        match log_channel {
            None => info!("Log channel not found in db. skipped"),
            Some(info) => match ChannelId::from_str(&info.value) {
                Err(e) => error!("Failed to parse log channel id: {}", e),
                Ok(id) => log_write.log = Some(id),
            },
        }

        // Register slash commands for main guild
        let main_guild_id = {
            ctx.data
                .read()
                .await
                .get::<ConfigValuesData>()
                .unwrap()
                .clone()
                .main_guild_id
        };

        info!("Setting up slash commands");
        match main_guild_id
            .set_application_commands(&ctx, |cmds| {
                cmds.set_application_commands(slash_commands::AppCommands::create_default())
            })
            .await
        {
            Ok(cmds) => {
                //info!("Setting slash commands permissions for: {:#?}", cmds);
                // Register slash commands for main guild
                let confs = {
                    ctx.data
                        .read()
                        .await
                        .get::<ConfigValuesData>()
                        .unwrap()
                        .clone()
                };

                let perms = cmds
                    .iter()
                    .map(|c| {
                        slash_commands::AppCommands::from_str(&c.name)
                            .map(|ac| ac.permission(c, &confs))
                    })
                    .collect::<Result<Vec<_>, _>>();
                match perms {
                    Err(e) => error!("Failed to figure out permissions for slash commands: {}", e),
                    Ok(perms) => {
                        if let Err(e) = main_guild_id
                            .set_application_commands_permissions(&ctx, |p| {
                                p.set_application_commands(perms)
                            })
                            .await
                        {
                            error!("Failed to set permissions for slash commands {:?}", e);
                        }
                    }
                }
            }
            Err(e) => error!("Failed to create application commands: {:?}", e),
        }

        // attempt to load SignupBoardData from db
        data_read
            .get::<SignupBoardData>()
            .unwrap()
            .write()
            .await
            .load_from_db(&ctx)
            .await
            .unwrap();

        info!("Setting presence");
        status::update_status(&ctx).await;

        if !self.signup_board_loop_running.load(Ordering::Relaxed) {
            // ctx is save to clone
            let ctx = ctx.clone();
            tokio::task::spawn(tasks::signup_board_task(ctx));
            self.signup_board_loop_running.swap(true, Ordering::Relaxed);
        }
        info!("Starting signup board loop");
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        let _ = &__arg1;
        info!("Resumed");
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::MessageComponent(mci) => interactions::button_interaction(&ctx, mci).await,
            Interaction::ApplicationCommand(aci) => {
                slash_commands::slash_command_interaction(&ctx, &aci).await
            }
            _ => (),
        }
    }

    async fn guild_member_removal(
        &self,
        ctx: Context,
        guild_id: GuildId,
        user: User,
        _member_data_if_available: Option<Member>,
    ) {
        // Check if in correct guild
        let main_guild_id = {
            ctx.data
                .read()
                .await
                .get::<ConfigValuesData>()
                .unwrap()
                .clone()
                .main_guild_id
        };

        if guild_id != main_guild_id {
            return;
        }

        let ctx = &ctx;
        let user_id = user.id;
        let mut log_info = LogInfo::automatic("User left server");
        log_info.add_user(user);

        log_discord(ctx, log_info, |trace| async move {
            trace.step("Loading user database info");
            match db::User::by_discord_id(ctx, user_id).await {
                Ok(db_user) => {
                    trace.step("Deleting user from db");
                    db_user.delete(ctx).await?;
                }
                Err(NotFound) => {
                    trace.step("User not found in database");
                    return Err(logging::InfoError::NotRegistered.into());
                }
                Err(e) => bail!(e),
            };
            Ok(())
        })
        .await;
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    // Load .env into ENV
    dotenv().ok();

    // Set up logging
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to start the logger");

    // Run migrations on the database
    {
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
        let conn = PgConnection::establish(&database_url)
            .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));
        embedded_migrations::run(&conn).expect("Failed to run migrations");
    }

    let token = env::var("DISCORD_TOKEN").expect("discord token not set");
    let app_id = env::var("APPLICATION_ID")
        .expect("application id not set")
        .parse::<u64>()
        .expect("Failed to parse application id");

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

    let intents = GatewayIntents::non_privileged() | GatewayIntents::GUILD_MEMBERS | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(token, intents)
        .application_id(app_id)
        .event_handler(Handler {
            signup_board_loop_running: AtomicBool::new(false),
        })
        .await
        .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ConfigValuesData>(Arc::new(ConfigValues {
            main_guild_id,
            admin_role_id,
            squadmaker_role_id,
            emoji_guild_id,
        }));
        data.insert::<LogConfigData>(Arc::new(RwLock::new(LogConfig { log: None })));
        data.insert::<DBPoolData>(Arc::new(db::DBPool::new()));
        data.insert::<SignupBoardData>(Arc::new(RwLock::new(SignupBoard {
            overview_channel_id: None,
            overview_message_id: None,
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
