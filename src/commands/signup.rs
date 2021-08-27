use crate::{conversation::*, db, embeds, log::*};
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

#[command]
#[description = "Register or update your GW2 account name with the bot"]
#[example = "AccountName.1234"]
#[usage = "account_name"]
#[num_args(1)]
pub async fn register(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    LogResult::command(ctx, msg, || async {
        let acc_name = args.single::<String>()?;
        let re = Regex::new("^[a-zA-Z]{3,27}\\.[0-9]{4}$").unwrap();
        if !re.is_match(&acc_name) {
            return Ok(Some("Invalid gw2 account name format".into()));
        }

        // this is an update on conflict
        let new_user = db::User::upsert(ctx, *msg.author.id.as_u64(), acc_name).await?;
        Ok(Some(format!(
            "Gw2 account name set to: {}",
            new_user.gw2_id
        )))
    })
    .await
}

#[command]
#[description = "Join a training with the provided id"]
#[example = "103"]
#[usage = "training_id"]
#[num_args(1)]
pub async fn join(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let training: db::Training = match LogResult::value(ctx, msg, || async {
        let id = match args.single_quoted::<i32>() {
            Ok(i) => i,
            Err(_) => return Err("Failed to parse trainings id".into()),
        };
        match db::Training::by_id(ctx, id).await {
            Ok(t) => Ok(t),
            Err(diesel::NotFound) => Err(format!("No open training with id: {}", id).into()),
            Err(e) => Err(e.into()),
        }
    })
    .await
    {
        Some(t) => t,
        None => return Ok(()),
    };

    let emb = embeds::training_base_embed(&training);

    let conv = match LogResult::value(ctx, msg, || async {
        Ok(Conversation::init(ctx, &msg.author, emb).await?)
    })
    .await
    {
        Some(c) => c,
        None => return Ok(()),
    };

    LogResult::conversation(ctx, conv, "join training".to_string(), |mut c| async move {
        _join_training(ctx, &mut c, &training).await
    })
    .await;

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
