use super::ADMIN_ROLE_CHECK;
use crate::{components, data::*, db, embeds, log::*, signup_board, utils};
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
    post_welcome_message,
    signup_board_reset,
    signup_board_reset_hard,
    signup_board_init_overview
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
            let sb = signup_board::SignupBoard::get(ctx).await;
            let mut write_lock = sb.write().await;
            write_lock.discord_category_id = Some(channel_id);
            write_lock.save_to_db(ctx).await.log_reply(msg)?;
        }

        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI))
            .await?;
        Ok(())
    })
    .await
}

#[command]
#[checks(admin_role)]
#[description = "Post the welcome/instruction message to a channel"]
#[usage = "channel_id"]
#[only_in("guild")]
#[num_args(1)]
pub async fn post_welcome_message(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let channel_id: ChannelId = args.single::<ChannelId>().log_reply(msg)?;
        channel_id
            .send_message(ctx, |m| {
                m.set_embed(embeds::welcome_post_embed());
                m.components(|c| c.add_action_row(components::register_list_action_row()));
                m
            })
            .await
            .log_reply(msg)?;
        Ok(())
    })
    .await
}

#[command]
#[checks(admin_role)]
#[description = "softly refreshes the Signup Board"]
#[usage = ""]
#[only_in("guild")]
#[num_args(0)]
pub async fn signup_board_reset(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        signup_board::SignupBoard::reset(ctx)
            .await
            .log_reply(msg)
            .log_reply(msg)?;
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
pub async fn signup_board_reset_hard(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        signup_board::SignupBoard::reset_hard(ctx)
            .await
            .log_reply(msg)
            .log_reply(msg)?;
        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI))
            .await?;
        Ok(())
    })
    .await
}

#[command]
#[checks(admin_role)]
#[description = "initialize the overview message"]
#[usage = ""]
#[only_in("guild")]
#[num_args(0)]
pub async fn signup_board_init_overview(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        {
            // Write lock only as long as needed
            let sb = signup_board::SignupBoard::get(ctx).await;
            let mut write_lock = sb.write().await;
            write_lock.set_up_overview(ctx).await.log_reply(msg)?
        }
        signup_board::SignupBoard::get(ctx)
            .await
            .read()
            .await
            .update_overview(ctx)
            .await
            .log_reply(msg)?;
        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI))
            .await?;
        Ok(())
    })
    .await
}
