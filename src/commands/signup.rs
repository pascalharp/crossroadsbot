use serenity::model::prelude::*;
use serenity::prelude::*;

use crate::db;
use regex::Regex;
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use std::sync::Arc;
use tracing::{error, info};

#[group]
#[commands(register)]
struct Signup;

#[command]
pub async fn register(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    if let Ok(acc_name) = args.single::<String>() {
        let re = Regex::new("^[a-zA-Z]{3,27}.[0-9]{4}$").unwrap();

        if !re.is_match(&acc_name) {
            msg.reply(
                &ctx.http,
                "This does not look like a gw2 account name. Please try again",
            )
            .await?;
            return Ok(());
        }

        let user_req = db::get_user(*msg.author.id.as_u64()).await;
        match user_req {
            // User already exist. update account name
            Ok(user) => {
                let user = Arc::new(user);
                if let Err(e) = user.clone().update_gw2_id(&acc_name).await {
                    error!("{}", e);
                    return Ok(());
                } else {
                    info!(
                        "{}#{} updated gw2 account name from {} to {}",
                        &msg.author.name, &msg.author.discriminator, &user.gw2_id, &acc_name
                    );
                }
                msg.react(&ctx.http, ReactionType::Unicode("✅".to_string()))
                    .await?;
            }
            // User does not exist. Create new one
            Err(_) => {
                if let Err(e) = db::add_user(*msg.author.id.as_u64(), &acc_name).await {
                    error!("{}", e);
                    return Ok(());
                } else {
                    info!(
                        "{}#{} registered for the first time with gw2 account name: {}",
                        &msg.author.name, &msg.author.discriminator, &acc_name
                    );
                }
                msg.react(&ctx.http, ReactionType::Unicode("✅".to_string()))
                    .await?;
            }
        }
    } else {
        msg.reply(
            &ctx.http,
            "No account name provided.\nUsage: register AccountName.1234",
        )
        .await?;
    }
    Ok(())
}
