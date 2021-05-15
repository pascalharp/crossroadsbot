use serenity::model::prelude::*;
use serenity::prelude::*;

use crate::commands::{
    ConfigValuesData, Conversation, ConversationError, CHECK_EMOJI, CROSS_EMOJI, DIZZY_EMOJI,
    ENVELOP_EMOJI,
};
use crate::db;
use crate::utils;
use regex::Regex;
use serenity::{
    framework::standard::{
        macros::{command, group},
        ArgError, Args, CommandResult,
    },
    futures::future,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tracing::info;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[group]
#[commands(register, join, list)]
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

async fn select_training(ctx: &Context, conv: &mut Conversation) -> Result<i32> {
    conv.msg
        .edit(ctx, |m| m.content("Loading possible trainings ..."))
        .await?;

    let trainings = db::Training::by_state(db::TrainingState::Open).await?;
    let trainings = utils::filter_trainings(ctx, trainings, &conv.user).await?;
    let trainings: HashMap<i32, db::Training> = trainings.into_iter().map(|t| (t.id, t)).collect();
    conv.msg
        .edit(ctx, |m| {
            m.embed(|e| {
                e.description("Select Training");
                for t in trainings.values() {
                    e.field(t.id.to_string(), utils::format_training_slim(t), true);
                }
                e.footer(|f| {
                    f.text("Select a training by responding with the id or type\"cancel\" to abort")
                })
            })
        })
        .await?;

    let id = loop {
        match conv.await_reply(ctx).await {
            None => {
                return Err(Box::new(ConversationError::TimedOut));
            }
            Some(m) => {
                if m.content.to_lowercase().eq("cancel") {
                    return Err(Box::new(ConversationError::Canceled));
                }
                let id: i32 = match m.content.parse() {
                    Ok(i) => i,
                    Err(_) => {
                        conv.chan
                            .send_message(ctx, |m| m.content("Could not parse id. Try again"))
                            .await?;
                        continue;
                    }
                };
                if !trainings.contains_key(&id) {
                    conv.chan
                        .send_message(ctx, |m| m.content("Not a valid id. Try again"))
                        .await?;
                    continue;
                }
                break id;
            }
        }
    };

    Ok(id)
}

/// Everything that triggers a signup should eventually land here where everything is checked
/// This only adds the signup and does not handle roles for the sign up
pub async fn join_training(
    ctx: &Context,
    conv: &mut Conversation,
    training_id: i32,
) -> Result<db::Signup> {
    // Check user
    let user_db = match db::User::get(*conv.user.id.as_u64()).await {
        Ok(u) => u,
        Err(diesel::NotFound) => {
            return Err(Box::new(ConversationError::Other(String::from(
                "User not found. Please use the register command first",
            ))));
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    // Get training with id
    let training = match db::Training::by_id_and_state(training_id, db::TrainingState::Open).await {
        Ok(t) => t,
        Err(diesel::NotFound) => {
            return Err(Box::new(ConversationError::Other(String::from(
                "No open training with that id found",
            ))));
        }
        Err(e) => {
            return Err(e.into());
        }
    };
    let training = Arc::new(training);

    // verify if tier requirements pass
    match utils::verify_tier(ctx, &training, &conv.user).await {
        Ok(pass) => {
            if !pass {
                return Err(Box::new(ConversationError::Other(String::from(
                    "Tier requirement failed",
                ))));
            }
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    // Check if signup already exist
    match db::Signup::by_user_and_training(&training, &user_db).await {
        Ok(_) => {
            return Err(Box::new(ConversationError::Other(String::from(
                "Already signed up for this training",
            ))));
        }
        Err(diesel::NotFound) => (), // This is what we want
        Err(e) => {
            return Err(e.into());
        }
    };

    let new_signup = db::NewSignup {
        training_id: training.id,
        user_id: user_db.id,
    };

    let signup = new_signup.add().await?;

    Ok(signup)
}

#[command]
#[description = "Join a training. Optionally provide training id and roles to speed up sign up process"]
#[example = "103 pdps cdps"]
#[usage = "[ training_id [ roles ... ] ]"]
#[min_args(0)]
pub async fn join(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    // check user first
    let user_discord = &msg.author;
    match db::User::get(*user_discord.id.as_u64()).await {
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

    let training_id = match args.single_quoted::<i32>() {
        Ok(i) => i,
        Err(ArgError::Eos) => {
            // No id provided start conversation and ask for id
            let mut conv = Conversation::start(ctx, &msg.author).await?;
            match select_training(ctx, &mut conv).await {
                Ok(t) => t,
                Err(e) => {
                    if let Some(ce) = e.downcast_ref::<ConversationError>() {
                        match ce {
                            ConversationError::TimedOut => {
                                conv.timeout_msg(ctx).await?;
                                return Ok(());
                            }
                            ConversationError::Canceled => {
                                conv.canceled_msg(ctx).await?;
                                return Ok(());
                            }
                            _ => return Err(e),
                        }
                    }
                    return Err(e);
                }
            }
        }
        Err(_) => {
            msg.reply(ctx, "Failed to parse trainings id").await?;
            return Ok(());
        }
    };

    let signup = {
        let mut conv = match Conversation::start(ctx, &user_discord).await {
            Ok(c) => {
                msg.react(ctx, ENVELOP_EMOJI).await?;
                c
            }
            Err(e) => {
                msg.reply(ctx, e).await?;
                return Ok(());
            }
        };
        match join_training(ctx, &mut conv, training_id).await {
            Ok(s) => {
                conv.msg
                    .edit(ctx, |m| {
                        m.content(format!("Signed up for training with id: {}", s.training_id))
                    })
                    .await?;
                s
            }
            Err(e) => {
                if let Some(ce) = e.downcast_ref::<ConversationError>() {
                    match ce {
                        ConversationError::Other(es) => {
                            conv.msg.edit(ctx, |m| m.content(es)).await?;
                            return Ok(());
                        }
                        _ => (), // No timeouts and cancel possible in this step
                    }
                }
                conv.msg
                    .edit(ctx, |m| m.content("Unexpected error"))
                    .await?;
                conv.msg.react(ctx, DIZZY_EMOJI).await?;
                return Err(e.into());
            }
        }
    };

    let training = Arc::new(signup.get_training().await?);
    // training role mapping
    let training_roles = training.clone().get_roles().await?;
    // The actual roles. ignoring deactivated ones (or db load errors in general)
    let roles: Vec<db::Role> = future::join_all(training_roles.iter().map(|tr| tr.role()))
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    let selected_roles = {
        let mut conv = Conversation::start(ctx, &user_discord).await?; // TODO error
        conv.msg.edit(ctx, |m| {
            m.content(format!("Select your roles for: __{}__", training.title))
        }).await?;
        // Create sets for selected and unselected
        let selected: HashSet<&db::Role> = HashSet::with_capacity(roles.len());
        let mut unselected: HashSet<&db::Role> = HashSet::with_capacity(roles.len());
        for r in &roles {
            unselected.insert(r);
        }

        match utils::select_roles(ctx, &mut conv, selected, unselected).await {
            Ok((selected, _ )) => selected,
            Err(e) => {
                if let Some(e) = e.downcast_ref::<ConversationError>() {
                    match e {
                        ConversationError::TimedOut => {
                            conv.timeout_msg(ctx).await?;
                            return Ok(());
                        },
                        ConversationError::Canceled => {
                            conv.canceled_msg(ctx).await?;
                            return Ok(());
                        },
                        _ => (),
                    }
                }
                return Err(e.into());
            }
        }
    };

    let mut conv = Conversation::start(ctx, &user_discord).await?;
    conv.msg.edit(ctx, |m| {
        m.content("Saving roles...")
    }).await?;

    let futs = selected_roles
        .iter()
        .map(|r| {
            let new_signup_role = db::NewSignupRole {
                role_id: r.id,
                signup_id: signup.id
            };
            new_signup_role.add()
        });
    match future::try_join_all(futs).await {
        Ok(_) => {
            conv.msg.edit(ctx, |m| {
                m.content(format!("Roles saved {}", CHECK_EMOJI))
            }).await?;
        },
        Err(e) => {
            conv.msg.edit(ctx, |m| {
                m.content(format!("An unexpected error occurred while saving roles {}", DIZZY_EMOJI))
            }).await?;
            return Err(e.into());
        }
    }

    Ok(())
}

#[command]
#[description = "Lists all active trainings you are currently signed up for"]
#[example = ""]
#[usage = ""]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, _: Args) -> CommandResult {

    let discord_user = &msg.author;
    let user = Arc::new(db::User::get(*discord_user.id.as_u64()).await?);

    let signups = user.clone().active_signups().await?;

    if signups.is_empty() {
        let mut conv = Conversation::start(ctx, &discord_user).await?;
        conv.msg.edit(ctx, |m| {
            m.content("No active signup found")
        }).await?;
        return Ok(())
    }

    let mut conv = Conversation::start(ctx, &discord_user).await?;
    conv.msg.edit(ctx, |m| { m.content(format!("Loading {} active signups", signups.len())) }).await?;
    msg.react(ctx, ENVELOP_EMOJI).await?;
    for (s, t) in signups {
        let signup_id = s.id;
        let roles = s.get_roles().await?;
        let roles = roles
            .iter()
            .map(|(_,r)| r )
            .collect::<Vec<_>>();
        let emb = utils::training_base_embed(&t);
        conv.chan.send_message(ctx, |m| {
            m.embed(|e| {
                e.0 = emb.0;
                e.field("**Signup Id**", &signup_id, true);
                e.field("Your selected roles", "------------------", false);
                e.fields( roles
                    .iter()
                    .map(|r| {
                        (&r.repr, &r.title, true)
                    }));
                e
            })
        }).await?;
    }
    Ok(())
}
