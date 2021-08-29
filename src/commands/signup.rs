use crate::{conversation::*, data, db, embeds, log::*, utils};
use regex::Regex;
use serenity::builder::CreateEmbed;
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
    let (user, training) = match LogResult::value_silent(ctx, &msg.author, || async {
        let id = match args.single_quoted::<i32>() {
            Ok(i) => i,
            Err(_) => {
                let reply = "Failed to parse trainings id".to_string();
                msg.reply(ctx, &reply).await.ok();
                return Err(reply.into());
            }
        };

        let db_user = match db::User::by_discord_id(ctx, msg.author.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                let embed = embeds::not_registered_embed();
                msg.channel_id
                    .send_message(ctx, |m| {
                        m.reference_message(msg);
                        m.add_embed(|e| {
                            e.0 = embed.0;
                            e
                        })
                    })
                    .await?;
                return Err(NOT_REGISTERED.into());
            }
            Err(e) => return Err(e.into()),
        };

        let training = match db::Training::by_id_and_state(ctx, id, db::TrainingState::Open).await {
            Ok(t) => t,
            Err(diesel::NotFound) => {
                let reply = format!("No **open** training with id: {}", id);
                msg.reply(ctx, &reply).await.ok();
                return Err(reply.into());
            }
            Err(e) => return Err(e.into()),
        };
        Ok((db_user, training))
    })
    .await
    {
        Some(t) => t,
        None => return Ok(()),
    };

    let emb = embeds::training_base_embed(&training);

    let mut conv = match LogResult::value(ctx, msg, || async {
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

    LogResult::command_separate_reply(ctx, &msg, &conv.msg.clone(), || async move {
        join_training(ctx, &mut conv, &training, &user).await
    })
    .await
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
    let (training, signup) = match LogResult::value_silent(ctx, &msg.author, || async {
        let training_id = match args.single_quoted::<i32>() {
            Ok(id) => id,
            Err(_) => {
                let reply = "Failed to parse training id".to_string();
                msg.reply(ctx, &reply).await.ok();
                return Err(reply.into());
            }
        };

        let training =
            match db::Training::by_id_and_state(ctx, training_id, db::TrainingState::Open).await {
                Ok(t) => t,
                Err(diesel::NotFound) => {
                    let reply = format!("No **open** training with id {} found", training_id);
                    msg.reply(ctx, &reply).await.ok();
                    return Err(reply.into());
                }
                Err(e) => return Err(e.into()),
            };

        let signup =
            match db::Signup::by_discord_user_and_training(ctx, &msg.author.id, &training).await {
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
                    return Err("No signup found".into());
                }
                Err(e) => return Err(e.into()),
            };
        Ok((training, signup))
    })
    .await
    {
        Some(s) => s,
        None => return Ok(()),
    };

    let emb = embeds::training_base_embed(&training);

    let mut conv = match LogResult::value(ctx, msg, || async {
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

    LogResult::command_separate_reply(ctx, &msg, &conv.msg.clone(), || async move {
        edit_signup(ctx, &mut conv, &training, &signup).await
    })
    .await
}

#[command]
#[description = "Lists all active trainings you are currently signed up for"]
#[example = ""]
#[usage = ""]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    let user = match LogResult::value_silent(ctx, &msg.author, || async {
        let db_user = match db::User::by_discord_id(ctx, msg.author.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                let embed = embeds::not_registered_embed();
                msg.channel_id
                    .send_message(ctx, |m| {
                        m.reference_message(msg);
                        m.add_embed(|e| {
                            e.0 = embed.0;
                            e
                        })
                    })
                    .await?;
                return Err(NOT_REGISTERED.into());
            }
            Err(e) => return Err(e.into()),
        };
        Ok(db_user)
    })
    .await
    {
        Some(t) => t,
        None => return Ok(()),
    };

    let mut emb = CreateEmbed::default();
    emb.description(format!("User information"));
    emb.field("Guild Wars 2 account name", &user.gw2_id, false);

    let mut conv = match LogResult::value(ctx, msg, || async {
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

    LogResult::command_separate_reply(ctx, &msg, &conv.msg.clone(), || async move {
        _list_signup(ctx, &mut conv, &user).await
    })
    .await
}
