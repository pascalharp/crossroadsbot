use crossroadsbot::{
    commands, conversation, data::*, db, log::*, signup_board::*, utils::DIZZY_EMOJI,
};
use dashmap::DashSet;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use dotenv::dotenv;
use serenity::{
    async_trait,
    client::{Client, EventHandler},
    framework::standard::{macros::hook, CommandResult, DispatchError, StandardFramework},
    model::prelude::*,
    prelude::*,
};
use std::{env, str::FromStr, sync::Arc};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[macro_use]
extern crate diesel_migrations;
use diesel_migrations::embed_migrations;
embed_migrations!("migrations/");

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
        info!("Refreshing config values");

        let log_info = db::Config::load(&ctx, String::from(INFO_LOG_NAME))
            .await
            .ok();
        let log_error = db::Config::load(&ctx, String::from(ERROR_LOG_NAME))
            .await
            .ok();
        let signup_board_category = db::Config::load(&ctx, String::from(SIGNUP_BOARD_NAME))
            .await
            .ok();
        let data_read = ctx.data.read().await;
        let mut log_write = data_read.get::<LogConfigData>().unwrap().write().await;

        match log_info {
            None => info!("Log info not found in db. skipped"),
            Some(info) => match ChannelId::from_str(&info.value) {
                Err(e) => error!("Failed to parse info channel id: {}", e),
                Ok(id) => log_write.info = Some(id),
            },
        }

        match log_error {
            None => info!("Log info not found in db. skipped"),
            Some(error) => match ChannelId::from_str(&error.value) {
                Err(e) => error!("Failed to parse error channel id: {}", e),
                Ok(id) => log_write.error = Some(id),
            },
        }

        let mut board_write = data_read.get::<SignupBoardData>().unwrap().write().await;
        match signup_board_category {
            None => info!("Signup board category not found in db. Skipped"),
            Some(category) => match ChannelId::from_str(&category.value) {
                Err(e) => error!("Failed to parse signup board channel id: {}", e),
                Ok(id) => {
                    board_write.set_category_channel(id);
                    info!("Resetting signup board");
                    if let Err(e) = board_write.reset(&ctx).await {
                        error!("Failed to reset signup board {}", e);
                    }
                }
            },
        }
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }

    async fn reaction_add(&self, ctx: Context, added_reaction: Reaction) {
        let user = added_reaction.user(&ctx).await;
        let user = match user {
            Err(_) => return,
            Ok(u) => {
                if u.bot {
                    return;
                } else {
                    u
                }
            }
        };

        // Keep locks only as long as needed
        let board_lock = {
            let data_read = ctx.data.read().await;
            data_read.get::<SignupBoardData>().unwrap().clone()
        };
        let board_action = {
            let board = board_lock.read().await;
            board.on_reaction(&added_reaction)
        };
        drop(board_lock);

        let ctx = Arc::new(ctx);
        let rm_ctx = ctx.clone();
        let result = match &board_action {
            SignupBoardAction::Ignore => return,
            SignupBoardAction::None => {
                tokio::task::spawn(async move {
                    added_reaction.delete(&*rm_ctx.clone()).await.ok();
                });
                return; // Nothing to log. just a random emoji
            }
            SignupBoardAction::JoinSignup(training) => {
                tokio::task::spawn(async move {
                    added_reaction.delete(&*rm_ctx.clone()).await.ok();
                });
                conversation::join_training(&*ctx, &user, training.id).await
            }
            SignupBoardAction::EditSignup(training) => {
                tokio::task::spawn(async move {
                    added_reaction.delete(&*rm_ctx.clone()).await.ok();
                });
                conversation::edit_signup(&*ctx, &user, training.id).await
            }
            SignupBoardAction::RemoveSignup(training) => {
                tokio::task::spawn(async move {
                    added_reaction.delete(&*rm_ctx.clone()).await.ok();
                });
                conversation::remove_signup(&*ctx, &user, training.id).await
            }
        };
        result
            .log(&ctx, LogType::Interaction(&board_action), &user)
            .await;
    }
}

#[hook]
async fn after(ctx: &Context, msg: &Message, command_name: &str, command_result: CommandResult) {
    let author = &msg.author;
    let command = msg.content_safe(ctx).await;
    // Log to subscriber
    match command_result {
        Ok(_) => (),
        Err(why) => {
            error!(
                "{}",
                format!("{} used command {}: {}", author.name, command_name, why)
            );
            let err_info = {
                ctx.data
                    .read()
                    .await
                    .get::<LogConfigData>()
                    .unwrap()
                    .clone()
                    .read()
                    .await
                    .error
            };
            if let Some(chan) = err_info {
                chan.send_message(ctx, |m| {
                    m.allowed_mentions(|m| m.empty_parse());
                    m.embed(|e| {
                        e.description("[ERROR] Command failed");
                        e.field("User", Mention::from(author), true);
                        e.field("Command", format!("`{}`", command), true);
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

#[hook]
async fn dispatch_error_hook(ctx: &Context, msg: &Message, error: DispatchError) {
    match error {
        DispatchError::NotEnoughArguments { min, given } => {
            let s = format!("Need {} arguments, but only got {}.", min, given);
            msg.reply(ctx, &s).await.ok();
        }
        DispatchError::TooManyArguments { max, given } => {
            let s = format!("Max arguments allowed is {}, but got {}.", max, given);
            msg.reply(ctx, &s).await.ok();
        }
        DispatchError::CheckFailed(..) => {
            let s = format!("No permissions to use this command");
            msg.reply(ctx, &s).await.ok();
        }
        _ => {
            msg.react(ctx, DIZZY_EMOJI).await.ok();
        }
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
            .expect(&format!("Error connecting to {}", database_url));
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

    let framework = StandardFramework::new()
        .configure(|c| {
            c.prefix(GLOB_COMMAND_PREFIX);
            c.no_dm_prefix(true)
        })
        .on_dispatch_error(dispatch_error_hook)
        .after(after)
        .help(&commands::HELP_CMD)
        .group(&commands::SIGNUP_GROUP)
        .group(&commands::TRAINING_GROUP)
        .group(&commands::ROLE_GROUP)
        .group(&commands::TIER_GROUP)
        .group(&commands::CONFIG_GROUP)
        .group(&commands::MISC_GROUP);

    let mut client = Client::builder(token)
        .application_id(app_id)
        .framework(framework)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ConversationLock>(Arc::new(DashSet::new()));
        data.insert::<ConfigValuesData>(Arc::new(ConfigValues {
            main_guild_id,
            admin_role_id,
            squadmaker_role_id,
            emoji_guild_id,
        }));
        data.insert::<LogConfigData>(Arc::new(RwLock::new(LogConfig {
            info: None,
            error: None,
        })));
        data.insert::<SignupBoardData>(Arc::new(RwLock::new(SignupBoard::new())));
        data.insert::<DBPoolData>(Arc::new(db::DBPool::new()));
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
