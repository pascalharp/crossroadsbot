use super::SQUADMAKER_ROLE_CHECK;
use crate::{
    conversation, db, embeds,
    utils::{self, *},
};
use chrono::{DateTime, Utc};
use chrono_tz::Europe::{London, Moscow, Paris};
use serenity::framework::standard::{
    macros::{command, group},
    ArgError, Args, CommandResult,
};
use serenity::futures::{prelude::*, stream};
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::try_join;

#[group]
#[prefix = "training"]
#[commands(list, show, add, set, info)]
pub struct Training;

#[command]
#[checks(squadmaker_role)]
#[usage = "training_name %Y-%m-%dT%H:%M:%S% training_tier [ role_identifier... ]"]
#[example = "\"Beginner Training\" 2021-05-11T19:00:00 none pdps cdps banners"]
#[min_args(3)]
#[only_in(guild)]
pub async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let discord_user = &msg.author;
    let training_name = args.single_quoted::<String>()?;

    let training_time = match args.single_quoted::<chrono::NaiveDateTime>() {
        Ok(r) => r,
        Err(e) => {
            match e {
                ArgError::Parse(_) => {
                    msg.reply(
                        ctx,
                        "Failed to parse date. Required Format: %Y-%m-%dT%H:%M:%S%",
                    )
                    .await?;
                }
                _ => {
                    return Err(e.into());
                }
            }
            return Ok(());
        }
    };

    let training_tier = args.single_quoted::<String>()?;
    let training_tier: Option<db::Tier> = {
        if training_tier.to_lowercase().eq("none") {
            None
        } else {
            match db::Tier::by_name(training_tier).await {
                Err(_) => {
                    msg.reply(
                        ctx,
                        "Tier not found. You can use \"none\" to open the training for everyone",
                    )
                    .await?;
                    return Ok(());
                }
                Ok(t) => Some(t),
            }
        }
    };

    let mut presel_roles: HashSet<String> = HashSet::new();

    for role in args.iter::<String>() {
        if let Ok(r) = role {
            presel_roles.insert(r);
        }
    }

    let mut conv = match conversation::Conversation::start(ctx, discord_user).await {
        Ok(c) => c,
        Err(e) => {
            msg.reply(ctx, e).await?;
            return Ok(());
        }
    };

    conv.msg
        .edit(ctx, |m| m.content("Loading roles..."))
        .await?;
    // Get roles and turn them into a HashMap with Emojis
    let roles = db::Role::all().await?;
    // Keep track of what roles are selected by EmojiId
    let mut selected: HashSet<&db::Role> = HashSet::new();
    let mut unselected: HashSet<&db::Role> = HashSet::new();

    // Enter pre selected roles
    for r in &roles {
        if presel_roles.contains(&r.repr) {
            selected.insert(r);
        } else {
            unselected.insert(r);
        }
    }

    conv.msg
        .edit(ctx, |m| m.content("Select roles for new training"))
        .await?;

    let selected = match utils::select_roles(ctx, &mut conv, selected, unselected).await {
        Ok((s, _)) => s,
        Err(e) => {
            if let Some(e) = e.downcast_ref::<conversation::ConversationError>() {
                conv.chan.send_message(ctx, |m| m.content(e)).await?;
                return Ok(());
            } else {
                conv.chan
                    .send_message(ctx, |m| m.content("Unexpected Error"))
                    .await?;
                return Err(e.into());
            }
        }
    };

    let confirm_msg = conv
        .chan
        .send_message(ctx, |m| {
            m.content(format!(
                "{} created a new training",
                Mention::from(discord_user)
            ));
            m.embed(|e| {
                e.field("Title", &training_name, true);
                e.field(
                    "Tier",
                    training_tier.as_ref().map_or("none", |t| &t.name),
                    true,
                );
                e.field("Date", &training_time, true);
                e.field("Roles", "------------", false);
                e.fields(
                    selected
                        .iter()
                        .map(|r| (r.repr.clone(), r.title.clone(), true)),
                );
                e.footer(|f| {
                    f.text(format!(
                        "Confirm new training with {} or {} to abort",
                        CHECK_EMOJI, CROSS_EMOJI
                    ))
                });
                e
            });
            m
        })
        .await?;

    utils::send_yes_or_no(ctx, &confirm_msg).await?;
    match utils::await_yes_or_no(ctx, &confirm_msg, Some(discord_user.id)).await {
        None => {
            conv.timeout_msg(ctx).await?;
            return Ok(());
        }
        Some(s) => match s {
            utils::YesOrNo::Yes => (),
            utils::YesOrNo::No => {
                conv.canceled_msg(ctx).await?;
                return Ok(());
            }
        },
    }

    // Do all the database stuff
    let training = {
        let training_tier_id = match training_tier {
            Some(t) => Some(t.id),
            None => None,
        };
        let new_training = db::NewTraining {
            title: String::from(training_name),
            date: training_time,
            tier_id: training_tier_id,
        };
        let training = match new_training.add().await {
            Err(e) => {
                msg.reply(ctx, format!("{}", e)).await?;
                return Ok(());
            }
            Ok(t) => t,
        };

        for r in &selected {
            match training.add_role(r.id).await {
                Err(e) => {
                    msg.reply(ctx, format!("{}", e)).await?;
                    return Ok(());
                }
                _ => (),
            };
        }
        training
    };

    confirm_msg.reply(ctx, "Training added").await?;

    let emb = embeds::training_base_embed(&training);
    msg.channel_id
        .send_message(ctx, |m| {
            m.allowed_mentions(|a| a.empty_parse());
            m.content(format!(
                "{} created a new training",
                Mention::from(discord_user)
            ));
            m.embed(|e| {
                e.0 = emb.0;
                e.field("Roles", "-----", false);
                e.fields(
                    selected
                        .into_iter()
                        .map(|r| (r.repr.clone(), r.title.clone(), true)),
                );
                e
            });
            m
        })
        .await?;

    Ok(())
}

const TRAINING_TIME_FMT: &str = "%a, %B %Y at %H:%M %Z";

#[command]
#[description = "Displays information about the training with the specified id"]
#[example = "121"]
#[usage = "training_id"]
pub async fn show(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let training_id = match args.single::<i32>() {
        Ok(i) => i,
        Err(_) => {
            msg.reply(ctx, "Unable to parse training id").await?;
            return Ok(());
        }
    };

    let training = match db::Training::by_id(training_id).await {
        Ok(t) => Arc::new(t),
        Err(_) => {
            msg.reply(ctx, "Unable to find training with this id")
                .await?;
            return Ok(());
        }
    };

    match training.state {
        db::TrainingState::Created | db::TrainingState::Finished => {
            msg.reply(ctx, "Information for this training is not public")
                .await?;
            return Ok(());
        }
        _ => (),
    }

    let roles: Vec<db::Role> = {
        let stream = stream::iter(training.clone().get_training_roles().await?);
        stream
            .filter_map(|r| async move {
                // Ignores deactivated roles
                r.role().await.ok()
            })
            .collect()
            .await
    };

    let (tier, tier_roles) = {
        let tier = training.get_tier().await;
        match tier {
            None => (None, None),
            Some(t) => {
                let t = Arc::new(t?);
                (Some(t.clone()), Some(t.clone().get_discord_roles().await?))
            }
        }
    };

    let role_map = role_emojis(ctx, roles).await?;

    let utc = DateTime::<Utc>::from_utc(training.date, Utc);
    msg.channel_id
        .send_message(ctx, |m| {
            m.allowed_mentions(|am| am.empty_parse());
            m.embed(|f| {
                f.description(format!(
                    "{} {}",
                    match &training.state {
                        db::TrainingState::Open => GREEN_CIRCLE_EMOJI,
                        db::TrainingState::Closed => RED_CIRCLE_EMOJI,
                        db::TrainingState::Started => RUNNING_EMOJI,
                        _ => ' ',
                    },
                    &training.title
                ));
                f.field(
                    "**Date**",
                    format!(
                        "{}\n{}\n{}\n{}",
                        utc.format(TRAINING_TIME_FMT),
                        utc.with_timezone(&London).format(TRAINING_TIME_FMT),
                        utc.with_timezone(&Paris).format(TRAINING_TIME_FMT),
                        utc.with_timezone(&Moscow).format(TRAINING_TIME_FMT),
                    ),
                    false,
                );
                f.field(
                    "**Requirements**",
                    match tier {
                        Some(t) => {
                            format!(
                                "{}\n{}",
                                t.name,
                                tier_roles
                                    .unwrap_or(vec![])
                                    .iter()
                                    .map(|r| {
                                        Mention::from(RoleId::from(r.discord_role_id as u64))
                                            .to_string()
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            )
                        }
                        None => "Open for everyone".to_string(),
                    },
                    true,
                );
                f.field("**State**", &training.state, true);
                f.field("**ID**", &training.id, true);
                f.field("**Available roles**    ", "**-----------------**", false);
                f.fields(role_map.values().map(|rm| {
                    (
                        format!("{}   {}", Mention::from(rm.emoji.id), &rm.role.repr),
                        &rm.role.title,
                        true,
                    )
                }));
                f
            })
        })
        .await?;

    Ok(())
}

#[command]
#[checks(squadmaker_role)]
#[description = "sets the training with the specified id to the specified state"]
#[example = "19832 started"]
#[usage = "training_id ( created | open | closed | started | finished )"]
#[num_args(2)]
pub async fn set(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let training_id = match args.single::<i32>() {
        Ok(i) => i,
        Err(_) => {
            msg.reply(ctx, "Unable to parse training id").await?;
            return Ok(());
        }
    };

    let state = match args.single::<db::TrainingState>() {
        Ok(s) => s,
        Err(_) => {
            msg.reply(ctx, "Not a training state").await?;
            return Ok(());
        }
    };

    let training = match db::Training::by_id(training_id).await {
        Ok(t) => t,
        Err(_) => {
            msg.reply(ctx, "Failed to load training, double check id")
                .await?;
            return Ok(());
        }
    };

    training.set_state(state).await?;
    msg.react(ctx, CHECK_EMOJI).await?;

    Ok(())
}

async fn list_by_state(ctx: &Context, msg: &Message, state: db::TrainingState) -> CommandResult {
    let author_id = msg.author.id;
    let trainings = { db::Training::by_state(state.clone()).await? };

    // An embed can only have 25 fields. So partition the training to be sent
    // over multiple messages if needed
    let partitioned = trainings.rchunks(25).collect::<Vec<_>>();

    if partitioned.is_empty() {
        msg.reply(ctx, "No trainings found").await?;
        return Ok(());
    }

    if partitioned.len() > 1 {
        let msg = msg.channel_id.send_message(ctx, |m| {
            m.embed( |f| {
                f.description("**WARNING**");
                f.color( (230, 160, 20) );
                f.field(
                    format!("{}", WARNING_EMOJI),
                    "More than 25 trainings found. This will take multiple messages to send. Continue?",
                    false);
                f.footer( |f| {
                    f.text(format!(
                            "{} to continue. {} to cancel",
                            CHECK_EMOJI,
                            CROSS_EMOJI))
                })
            })
        }).await?;

        utils::send_yes_or_no(ctx, &msg).await?;
        match utils::await_yes_or_no(ctx, &msg, Some(author_id)).await {
            None => {
                msg.reply(ctx, "Timed out").await?;
                return Ok(());
            }
            Some(utils::YesOrNo::Yes) => (),
            Some(utils::YesOrNo::No) => {
                msg.reply(ctx, "Aborted").await?;
                return Ok(());
            }
        }
    }

    let state = &state;
    for trainings in partitioned.iter() {
        msg.channel_id
            .send_message(ctx, |m| {
                m.embed(move |f| {
                    f.title(format!(
                        "{} Trainings",
                        match state {
                            db::TrainingState::Open => "Open",
                            db::TrainingState::Created => "Created",
                            db::TrainingState::Closed => "Closed",
                            db::TrainingState::Started => "Started",
                            db::TrainingState::Finished => "Finished",
                        }
                    ));
                    for t in trainings.iter() {
                        f.field(
                            &t.title,
                            format!("**Date**: {}\n**Id**: {}", t.date, t.id),
                            true,
                        );
                    }
                    f
                })
            })
            .await?;
    }
    Ok(())
}

async fn list_amounts(ctx: &Context, msg: &Message) -> CommandResult {
    let (created, open, closed, started, finished) = match try_join!(
        db::Training::amount_by_state(db::TrainingState::Created),
        db::Training::amount_by_state(db::TrainingState::Open),
        db::Training::amount_by_state(db::TrainingState::Closed),
        db::Training::amount_by_state(db::TrainingState::Started),
        db::Training::amount_by_state(db::TrainingState::Finished),
    ) {
        Ok(ok) => ok,
        Err(e) => {
            msg.reply(ctx, "Unexpected error loading trainings").await?;
            return Err(e.into());
        }
    };

    let total = created + open + closed + started + finished;
    let active = open + closed + started;

    msg.channel_id.send_message(ctx, |m| {
        m.embed( |e| {
            e.description("Amount of trainings");
            e.field("Total and listed per state",
                format!("`{}`\n`{}`\n\n`{}`\n`{}`\n`{}`\n`{}`\n`{}`\n",
                    format!("Total    : {}", total),
                    format!("Active*  : {}", active),
                    format!("Created  : {}", created),
                    format!("Open     : {}", open),
                    format!("Closed   : {}", closed),
                    format!("Started  : {}", started),
                    format!("Finished : {}", finished),
                ),
                false);
            e.footer( |f| {
                f.text("For more details pass the state. For example: training list open\n*(Active = Open + Closed + Started)")
            });
            e
        })
    }).await?;
    Ok(())
}

#[command]
#[description = "List trainings. Lists published trainings by default"]
#[usage = "[ training_state ]"]
#[checks(squadmaker_role)]
#[max_args(1)]
async fn list(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let state: Option<db::TrainingState> = match args.single_quoted::<db::TrainingState>() {
        Ok(s) => Some(s),
        Err(ArgError::Eos) => None,
        Err(_) => {
            msg.reply(
                ctx,
                "Failed to parse state. Make sure its a valid training state",
            )
            .await?;
            return Ok(());
        }
    };

    match state {
        Some(s) => return list_by_state(ctx, msg, s).await,
        None => return list_amounts(ctx, msg).await,
    }
}

#[command]
#[checks(squadmaker_role)]
#[description = "lists information about the amount of sign ups and selected roles"]
#[example = "123"]
#[usage = "training_id"]
#[num_args(1)]
pub async fn info(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let training_id = match args.single_quoted::<i32>() {
        Ok(id) => id,
        Err(_) => {
            msg.reply(ctx, "Failed to parse training id").await?;
            return Ok(());
        }
    };

    let training = match db::Training::by_id(training_id).await {
        Ok(t) => Arc::new(t),
        Err(diesel::NotFound) => {
            msg.reply(ctx, "Training not found").await?;
            return Ok(());
        }
        Err(_) => {
            msg.reply(ctx, "Unexpected error").await?;
            return Ok(());
        }
    };

    let signups = training
        .clone()
        .get_signups()
        .await?
        .into_iter()
        .map(|s| Arc::new(s))
        .collect::<Vec<_>>();

    let mut roles = training
        .clone()
        .all_roles()
        .await?
        .into_iter()
        .map(|(_, r)| (r, 0))
        .collect::<HashMap<db::Role, u32>>();

    let signed_up_roles = future::try_join_all(signups.iter().map(|s| s.clone().get_roles()))
        .await?
        .into_iter()
        .flatten()
        .map(|(_, r)| r)
        .collect::<Vec<_>>();

    for sr in signed_up_roles {
        roles.entry(sr).and_modify(|e| *e += 1);
    }

    let embed = embeds::training_base_embed(training.as_ref());

    msg.channel_id
        .send_message(ctx, |m| {
            m.embed(|e| {
                e.0 = embed.0;
                e.field("Total sign ups", format!("**{}**", signups.len()), false);
                e.fields(roles.iter().map(|(role, count)| {
                    (
                        format!("Role: {}", role.repr),
                        format!("Count: {}", count),
                        true,
                    )
                }));
                e
            })
        })
        .await?;

    Ok(())
}
