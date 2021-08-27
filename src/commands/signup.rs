use crate::{conversation::*, db, log::*};
use regex::Regex;
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::prelude::*;
use serenity::prelude::*;

#[group]
#[commands(register, join, leave, edit, list)]
struct Signup;

async fn _register(ctx: &Context, user: &User, gw2_acc: String) -> LogResult {
    let re = Regex::new("^[a-zA-Z]{3,27}\\.[0-9]{4}$").unwrap();
    if !re.is_match(&gw2_acc) {
        return Ok(Some("Invalid gw2 account name format".into()));
    }

    // this is an update on conflict
    let new_user = db::User::upsert(ctx, *user.id.as_u64(), gw2_acc.clone()).await?;
    Ok(Some(format!("Gw2 account set to: {}", new_user.gw2_id)))
}

#[command]
#[description = "Register or update your GW2 account name with the bot"]
#[example = "AccountName.1234"]
#[usage = "account_name"]
#[num_args(1)]
pub async fn register(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let acc_name = args.single::<String>()?;
    let res = _register(ctx, &msg.author, acc_name).await;
    res.reply(ctx, msg).await?;
    res.log(ctx, msg.into(), &msg.author).await;
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

    let res = join_training(ctx, &msg.author, training_id).await;
    if let Err(e) = &res {
        if let Some(e) = e.downcast_ref::<ConversationError>() {
            msg.reply(ctx, e).await?;
        }
    }
    res.log(ctx, msg.into(), &msg.author).await;
    Ok(())
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

    let res = remove_signup(ctx, &msg.author, training_id).await;
    if let Err(e) = &res {
        if let Some(e) = e.downcast_ref::<ConversationError>() {
            msg.reply(ctx, e).await?;
        }
    }
    res.log(ctx, msg.into(), &msg.author).await;
    Ok(())
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

    let res = edit_signup(ctx, &msg.author, training_id).await;
    if let Err(e) = &res {
        if let Some(e) = e.downcast_ref::<ConversationError>() {
            msg.reply(ctx, e).await?;
        }
    }
    res.log(ctx, msg.into(), &msg.author).await;
    Ok(())
}

#[command]
#[description = "Lists all active trainings you are currently signed up for"]
#[example = ""]
#[usage = ""]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    let res = list_signup(ctx, &msg.author).await;
    if let Err(e) = &res {
        if let Some(e) = e.downcast_ref::<ConversationError>() {
            msg.reply(ctx, e).await?;
        }
    }
    res.log(ctx, msg.into(), &msg.author).await;
    Ok(())
}
