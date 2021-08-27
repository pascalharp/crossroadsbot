use super::ADMIN_ROLE_CHECK;
use crate::{data::*, db, log::*, signup_board};
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::prelude::*;
use serenity::prelude::*;

#[group]
#[only_in(guilds)]
#[commands(
    set_log_info,
    set_log_error,
    set_signup_board_category,
    signup_board_reset
)]
struct Config;

enum LogChannelType {
    Info,
    Error,
}

async fn set_log_channel(ctx: &Context, mut args: Args, kind: LogChannelType) -> LogResult {
    let channel_id: ChannelId = match args.single::<ChannelId>() {
        Err(_) => {
            return Err("No valid channel provided".into());
        }
        Ok(c) => c,
    };

    // save in memory
    {
        let write_lock = ctx
            .data
            .read()
            .await
            .get::<LogConfigData>()
            .unwrap()
            .clone();
        match kind {
            LogChannelType::Info => write_lock.write().await.info = Some(channel_id),
            LogChannelType::Error => write_lock.write().await.error = Some(channel_id),
        }
    }

    // save to db
    let conf = db::Config {
        name: match kind {
            LogChannelType::Info => String::from(INFO_LOG_NAME),
            LogChannelType::Error => String::from(ERROR_LOG_NAME),
        },
        value: channel_id.to_string(),
    };
    conf.save(ctx).await?;

    Ok(Some("Log channel saved".into()))
}

#[command]
#[checks(admin_role)]
#[description = "Sets the log channel for info"]
#[example = "#logs_info"]
#[usage = "channel_mention"]
#[only_in("guild")]
#[num_args(1)]
pub async fn set_log_info(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    LogResult::command(ctx, msg, || async {
        set_log_channel(ctx, args, LogChannelType::Info).await
    })
    .await
}

#[command]
#[checks(admin_role)]
#[description = "Sets the log channel for error"]
#[example = "#logs_error"]
#[usage = "channel_mention"]
#[only_in("guild")]
#[num_args(1)]
pub async fn set_log_error(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    LogResult::command(ctx, msg, || async {
        set_log_channel(ctx, args, LogChannelType::Error).await
    })
    .await
}

#[command]
#[checks(admin_role)]
#[description = "Sets category id for the SignupBoard"]
#[usage = "category_id"]
#[only_in("guild")]
#[num_args(1)]
pub async fn set_signup_board_category(
    ctx: &Context,
    msg: &Message,
    mut args: Args,
) -> CommandResult {
    LogResult::command(ctx, msg, || async {
        let channel_id: ChannelId = match args.single::<ChannelId>() {
            Err(_) => {
                return Ok(Some("No valid channel provided".into()));
            }
            Ok(c) => c,
        };

        // set in memory
        {
            let write_lock = ctx
                .data
                .read()
                .await
                .get::<SignupBoardData>()
                .unwrap()
                .clone();
            write_lock.write().await.set_category_channel(channel_id);
        }

        // save to db
        let conf = db::Config {
            name: String::from(signup_board::SIGNUP_BOARD_NAME),
            value: channel_id.to_string(),
        };

        match conf.save(ctx).await {
            Ok(_) => Ok(Some("Signup board category saved".into())),
            Err(e) => {
                return Err(e.into());
            }
        }
    })
    .await
}

#[command]
#[checks(admin_role)]
#[description = "fully resets the Signup Board"]
#[usage = ""]
#[only_in("guild")]
#[num_args(0)]
pub async fn signup_board_reset(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    LogResult::command(ctx, msg, || async {
        let write_lock = ctx
            .data
            .read()
            .await
            .get::<SignupBoardData>()
            .unwrap()
            .clone();

        write_lock.write().await.reset(ctx).await?;
        LogResult::Ok(Some("Signup board resetted".into()))
    })
    .await
}
