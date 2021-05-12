use serenity::model::prelude::*;
use serenity::prelude::*;

use crate::commands::CHECK_EMOJI;
use crate::db;
use regex::Regex;
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use std::sync::Arc;
use tracing::info;

#[group]
#[commands(register)]
struct Signup;

#[command]
#[description = "Register or update your GW2 account name with the bot"]
#[example = "AccountName.1234"]
#[usage = "account_name"]
#[num_args(1)]
pub async fn register(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let acc_name = args.single::<String>()?;
    let re = Regex::new("^[a-zA-Z]{3,27}.[0-9]{4}$").unwrap();

    if !re.is_match(&acc_name) {
        msg.reply(
            &ctx.http,
            "This does not look like a gw2 account name. Please try again",
        )
        .await?;
        return Ok(());
    }

    let user_req = db::User::get(*msg.author.id.as_u64()).await;
    match user_req {
        // User already exist. update account name
        Ok(user) => {
            let user = Arc::new(user);
            user.clone().update_gw2_id(&acc_name).await?;
            info!(
                "{}#{} updated gw2 account name from {} to {}",
                &msg.author.name, &msg.author.discriminator, &user.gw2_id, &acc_name
            );
            msg.react(&ctx.http, CHECK_EMOJI).await?;
        },
        // User does not exist. Create new one
        Err(diesel::result::Error::NotFound) => {
            db::User::add(*msg.author.id.as_u64(), acc_name.clone()).await?;
            info!(
                "{}#{} registered for the first time with gw2 account name: {}",
                &msg.author.name, &msg.author.discriminator, &acc_name
            );
            msg.react(&ctx.http, CHECK_EMOJI).await?;
        }
        Err(e) => {
            msg.reply(ctx, "An unexpected error occurred").await?;
            return Err(e.into());
        }
    }
    Ok(())
}
