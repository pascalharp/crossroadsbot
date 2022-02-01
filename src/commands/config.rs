use super::ADMIN_ROLE_CHECK;
use crate::{data::*, db, log::*, utils};
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::prelude::*;
use serenity::prelude::*;

#[group]
#[only_in(guilds)]
#[commands(set_log_channel)]
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
