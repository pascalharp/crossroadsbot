use super::ADMIN_ROLE_CHECK;
use crate::{data::*, db, log::*, signup_board, utils};
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::prelude::*;
use serenity::prelude::*;

#[group]
#[only_in(guilds)]
#[commands(
    set_log_channel,
    set_signup_board_category,
    signup_board_reset
)]
struct Config;

#[command]
#[checks(admin_role)]
#[description = "Sets the log channel for info"]
#[example = "#logs"]
#[usage = "channel_mention"]
#[only_in("guild")]
#[num_args(1)]
pub async fn set_log_channel(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let channel_id: ChannelId = args.single::<ChannelId>().log_reply(msg)?;
        {
            let write_lock = ctx
                .data
                .read()
                .await
                .get::<LogConfigData>()
                .unwrap()
                .clone();
            write_lock.write().await.log = Some(channel_id);
        }

        let conf = db::Config {
            name: String::from(INFO_LOG_NAME),
            value: channel_id.to_string(),
        };
        conf.save(ctx).await.log_reply(msg)?;

        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI))
            .await?;
        Ok(())
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
    log_command(ctx, msg, || async {
        let channel_id: ChannelId = args.single::<ChannelId>().log_reply(msg)?;

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

        let conf = db::Config {
            name: String::from(signup_board::SIGNUP_BOARD_NAME),
            value: channel_id.to_string(),
        };

        conf.save(ctx).await.log_reply(msg)?;

        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI))
            .await?;
        Ok(())
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
    log_command(ctx, msg, || async {
        let write_lock = ctx
            .data
            .read()
            .await
            .get::<SignupBoardData>()
            .unwrap()
            .clone();

        write_lock.write().await.reset(ctx).await.log_reply(msg)?;

        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI))
            .await?;
        Ok(())
    })
    .await
}
