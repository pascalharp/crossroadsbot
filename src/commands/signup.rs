use crate::{conversation::*, db, embeds, utils::*};
use regex::Regex;
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::sync::Arc;
use tracing::info;

#[group]
#[commands(register, join, leave, edit, list)]
struct Signup;

#[command]
#[description = "Register or update your GW2 account name with the bot"]
#[example = "AccountName.1234"]
#[usage = "account_name"]
#[num_args(1)]
pub async fn register(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let acc_name = args.single::<String>()?;
    let re = Regex::new("^[a-zA-Z]{3,27}\\.[0-9]{4}$").unwrap();

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
        }
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

#[command]
#[description = "Join a training with the provided id"]
#[example = "103"]
#[usage = "training_id"]
#[num_args(1)]
pub async fn join(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let training_id = match args.single_quoted::<i32>() {
        Ok(i) => i,
        Err(_) => {
            msg.reply(ctx, "Failed to parse trainings id").await?;
            return Ok(());
        }
    };

    match join_training(ctx, &msg.author, training_id).await {
        Ok(()) => return Ok(()),
        Err(e) => {
            if let Some(e) = e.downcast_ref::<ConversationError>() {
                msg.reply(ctx, e).await?;
                return Ok(());
            }
            return Err(e.into());
        }
    }
}

#[command]
#[description = "Leave a training you have already signed up up for. Only possible if the training is still open for sign ups"]
#[example = "103"]
#[usage = "training_id"]
#[num_args(1)]
pub async fn leave(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let training_id = match args.single_quoted::<i32>() {
        Ok(i) => i,
        Err(_) => {
            msg.reply(ctx, "Failed to parse trainings id").await?;
            return Ok(());
        }
    };

    match remove_signup(ctx, &msg.author, training_id).await {
        Ok(()) => return Ok(()),
        Err(e) => {
            if let Some(e) = e.downcast_ref::<ConversationError>() {
                msg.reply(ctx, e).await?;
                return Ok(());
            }
            return Err(e.into());
        }
    }
}

#[command]
#[description = "Edit your sign up"]
#[example = "103"]
#[usage = "training_id"]
#[num_args(1)]
pub async fn edit(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let training_id = match args.single_quoted::<i32>() {
        Ok(i) => i,
        Err(_) => {
            msg.reply(ctx, "Failed to parse trainings id").await?;
            return Ok(());
        }
    };

    match edit_signup(ctx, &msg.author, training_id).await {
        Ok(()) => return Ok(()),
        Err(e) => {
            if let Some(e) = e.downcast_ref::<ConversationError>() {
                msg.reply(ctx, e).await?;
                return Ok(());
            }
            return Err(e.into());
        }
    }
}

#[command]
#[description = "Lists all active trainings you are currently signed up for"]
#[example = ""]
#[usage = ""]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    let discord_user = &msg.author;
    let user = match db::User::get(*discord_user.id.as_u64()).await {
        Ok(u) => u,
        Err(diesel::NotFound) => {
            msg.reply(ctx, "User not found. Please use the register command first")
                .await?;
            return Ok(());
        }
        Err(e) => {
            msg.reply(ctx, "Unexpected error").await?;
            return Err(e.into());
        }
    };
    let user = Arc::new(user);

    let signups = user.clone().active_signups().await?;

    if signups.is_empty() {
        let mut conv = Conversation::start(ctx, &discord_user).await?;
        conv.msg
            .edit(ctx, |m| m.content("No active signup found"))
            .await?;
        return Ok(());
    }

    let mut conv = Conversation::start(ctx, &discord_user).await?;
    conv.msg
        .edit(ctx, |m| {
            m.content(format!("Loading {} active signup(s)", signups.len()))
        })
        .await?;
    msg.react(ctx, ENVELOP_EMOJI).await?;
    for (s, t) in signups {
        let signup_id = s.id;
        let s = Arc::new(s);
        let roles = s.get_roles().await?;
        let roles = roles.iter().map(|(_, r)| r).collect::<Vec<_>>();
        let emb = embeds::training_base_embed(&t);
        conv.chan
            .send_message(ctx, |m| {
                m.embed(|e| {
                    e.0 = emb.0;
                    e.field("**Signup Id**", &signup_id, true);
                    e.field("Your selected roles", "------------------", false);
                    e.fields(roles.iter().map(|r| (&r.repr, &r.title, true)));
                    e
                })
            })
            .await?;
    }
    Ok(())
}
