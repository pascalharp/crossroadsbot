use crate::{conversation::*, data, db, embeds, log::*, utils};
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
            return Ok(LogAction::Reply("Invalid gw2 account name format".into()));
        }

        // this is an update on conflict
        let new_user = db::User::upsert(ctx, *msg.author.id.as_u64(), acc_name).await?;
        Ok(LogAction::Reply(format!(
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

    // if not already in dms give a hint
    if !msg.is_private() {
        msg.reply(ctx, format!("Check DM's {}", utils::ENVELOP_EMOJI))
            .await
            .ok();
    }

    LogResult::conversation(ctx, conv, "join training".to_string(), |mut c| async move {
        join_training(ctx, &mut c, &training).await
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
    LogResult::command(ctx, msg, || async {
        let training_id = match args.single_quoted::<i32>() {
            Ok(id) => id,
            Err(_) => return Err("Failed to parse training id".into()),
        };

        let db_user = match db::User::by_discord_id(ctx, msg.author.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                let emb = embeds::not_registered_embed();
                msg.channel_id
                    .send_message(ctx, |m| {
                        m.reference_message(msg);
                        m.embed(|e| {
                            e.0 = emb.0;
                            e
                        })
                    })
                    .await?;
                return Ok(LogAction::LogOnly(NOT_REGISTERED.into()));
            }
            Err(e) => return Err(e.into()),
        };

        let training =
            match db::Training::by_id_and_state(ctx, training_id, db::TrainingState::Open).await {
                Ok(t) => t,
                Err(diesel::NotFound) => {
                    return Err(
                        format!("No **open** training with id {} found", training_id).into(),
                    );
                }
                Err(e) => return Err(e.into()),
            };

        let signup = match db::Signup::by_user_and_training(ctx, &db_user, &training).await {
            Ok(s) => s,
            Err(diesel::NotFound) => {
                msg.channel_id
                    .send_message(ctx, |m| {
                        m.reference_message(msg);
                        m.embed(|e| {
                            e.description(format!("{} No signup found", utils::CROSS_EMOJI));
                            e.field(
                                "You are not yet signed up for training:",
                                &training.title,
                                false,
                            );
                            e.field(
                                "If you want to join this training use:",
                                format!("`{}join {}`", data::GLOB_COMMAND_PREFIX, training.id),
                                false,
                            )
                        })
                    })
                    .await?;
                return Ok(LogAction::LogOnly(NOT_SIGNED_UP.into()));
            }
            Err(e) => return Err(e.into()),
        };

        match signup.remove(ctx).await {
            Ok(1) => (),
            Ok(a) => {
                return Err(format!("Unexpected amount of signups removed. Amount: {}", a).into())
            }
            Err(e) => return Err(e.into()),
        }

        msg.channel_id
            .send_message(ctx, |m| {
                m.reference_message(msg);
                m.content("");
                m.embed(|e| {
                    e.description(format!("{} Signup removed", utils::CHECK_EMOJI));
                    e.field("Signup removed for training:", &training.title, false)
                })
            })
            .await?;
        Ok(LogAction::LogOnly("Signup removed".into()))
    })
    .await
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
