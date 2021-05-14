use super::{
    ADMIN_ROLE_CHECK, CHECK_EMOJI, CROSS_EMOJI, DEFAULT_TIMEOUT,
    GREEN_CIRCLE_EMOJI, RED_CIRCLE_EMOJI, RUNNING_EMOJI, WARNING_EMOJI,
};
use crate::{db, utils};
use chrono::{DateTime, Utc};
use chrono_tz::Europe::{London, Moscow, Paris};
use serenity::collector::reaction_collector::ReactionAction;
use serenity::framework::standard::{
    macros::{command, group},
    ArgError, Args, CommandResult,
};
use serenity::futures::prelude::*;
use serenity::futures::stream;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use crate::utils::{RoleEmoji, role_emojis};

#[group]
#[prefix = "training"]
#[commands(list, show, add, set)]
pub struct Training;

type BoxResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

// Helper function to  update add_training embed message
async fn update_add_training(
    ctx: &Context,
    msg: &mut Message,
    role_emojis: &HashMap<EmojiId, RoleEmoji>,
    selected: &HashSet<EmojiId>,
    training_name: &str,
    training_time: &chrono::NaiveDateTime,
    training_tier: &Option<db::Tier>,
) -> BoxResult<()> {
    msg.edit(ctx, |m| {
        m.embed(|e| {
            e.description("New Training");
            e.field(
                "Details",
                format!(
                    "{}\n{}\n{}",
                    training_name,
                    training_time,
                    match training_tier {
                        Some(t) => t.name.clone(),
                        None => String::from("none"),
                    }
                ),
                false,
            );

            for (k, v) in role_emojis.iter() {
                e.field(
                    format!(
                        "{} {}",
                        if selected.contains(k) {
                            CHECK_EMOJI
                        } else {
                            CROSS_EMOJI
                        },
                        v.role.repr
                    ),
                    format!("{} {}", Mention::from(&v.emoji), v.role.title),
                    true,
                );
            }
            e.footer(|f| {
                f.text(format!(
                    "Select roles. Use {} to finish and {} to abort",
                    CHECK_EMOJI, CROSS_EMOJI
                ))
            });
            e
        })
    })
    .await?;

    Ok(())
}

#[command]
#[checks(admin_role)]
#[usage = "training_name %Y-%m-%dT%H:%M:%S% training_tier [ role_identifier... ]"]
#[example = "\"Beginner Training\" 2021-05-11T19:00:00 none pdps cdps banners"]
#[min_args(3)]
pub async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
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

    let mut msg = msg
        .channel_id
        .send_message(ctx, |m| {
            m.embed(|e| {
                e.description("New Training");
                e.field(
                    "Details",
                    format!("{}\n{}", training_name, training_time),
                    false,
                );
                e.footer(|f| f.text("Loading roles ..."));
                e
            })
        })
        .await?;

    // Get roles and turn them into a HashMap with Emojis
    let roles = db::Role::all().await?;
    let re = Arc::new(role_emojis(ctx, roles).await?);
    // Keep track of what roles are selected by EmojiId
    let mut selected: HashSet<EmojiId> = HashSet::new();

    // Enter pre selected roles
    for re in re.values() {
        if presel_roles.contains(&re.role.repr) {
            selected.insert(re.emoji.id);
        }
    }

    msg.react(ctx, CHECK_EMOJI).await?;
    msg.react(ctx, CROSS_EMOJI).await?;

    for i in re.values() {
        msg.react(ctx, i.emoji.clone()).await?;
    }

    update_add_training(
        ctx,
        &mut msg,
        &re,
        &selected,
        &training_name,
        &training_time,
        &training_tier,
    )
    .await?;

    // Create another reference so that it can be moved to filter function
    let collect_re = re.clone();
    let mut reacts = msg
        .await_reactions(ctx)
        .removed(true)
        .timeout(DEFAULT_TIMEOUT)
        .filter(move |r| {
            if r.emoji == ReactionType::from(CHECK_EMOJI)
                || r.emoji == ReactionType::from(CROSS_EMOJI)
            {
                return true;
            }
            match r.emoji {
                ReactionType::Custom {
                    animated: _,
                    id,
                    name: _,
                } => collect_re.contains_key(&id),
                _ => false,
            }
        })
        .await;

    loop {
        match reacts.next().await {
            Some(r) => {
                match r.as_ref() {
                    ReactionAction::Added(r) => {
                        if r.emoji == ReactionType::from(CHECK_EMOJI) {
                            break;
                        } else if r.emoji == ReactionType::from(CROSS_EMOJI) {
                            msg.reply(ctx, "Aborted").await?;
                            return Ok(());
                        }
                        match r.emoji {
                            ReactionType::Custom {
                                animated: _,
                                id,
                                name: _,
                            } => {
                                selected.insert(id);
                            }
                            _ => (),
                        }
                    }
                    ReactionAction::Removed(r) => match r.emoji {
                        ReactionType::Custom {
                            animated: _,
                            id,
                            name: _,
                        } => {
                            selected.remove(&id);
                        }
                        _ => (),
                    },
                }
                update_add_training(
                    ctx,
                    &mut msg,
                    &re,
                    &selected,
                    &training_name,
                    &training_time,
                    &training_tier,
                )
                .await?;
            }
            None => {
                msg.reply(ctx, "Timed out").await?;
                return Ok(());
            }
        }
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

        for r in re.values() {
            if selected.contains(&r.emoji.id) {
                match training.add_role(r.role.id).await {
                    Err(e) => {
                        msg.reply(ctx, format!("{}", e)).await?;
                        return Ok(());
                    }
                    _ => (),
                };
            }
        }
        training
    };

    msg.channel_id
        .send_message(ctx, |m| {
            m.embed(|e| {
                e.description("Training added");
                e.field("Name", training.title, false);
                e.field("Id", training.id, false);
                e.field("Date", training.date, false);
                e.field("State", training.state, false);
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
        let stream = stream::iter(training.clone().get_roles().await?);
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
#[checks(admin_role)]
#[description = "sets the training with the specified id to the specified state"]
#[example = "19832 started"]
#[usage = "training_id ( created | published | closed | started | finished )"]
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

#[command]
#[description = "List trainings. Lists published trainings by default"]
#[usage = "[ training_state ]"]
#[sub_commands(list_created, list_published, list_closed, list_started, list_finished)]
#[num_args(1)]
async fn list(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    msg.reply(ctx, "Not a training state").await?;
    Ok(())
}

#[command("created")]
#[checks(admin_role)]
async fn list_created(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    list_by_state(ctx, msg, db::TrainingState::Created).await
}

#[command("open")]
async fn list_published(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    list_by_state(ctx, msg, db::TrainingState::Open).await
}

#[command("closed")]
async fn list_closed(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    list_by_state(ctx, msg, db::TrainingState::Closed).await
}

#[command("started")]
#[checks(admin_role)]
async fn list_started(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    list_by_state(ctx, msg, db::TrainingState::Started).await
}

#[command("finished")]
#[checks(admin_role)]
async fn list_finished(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    list_by_state(ctx, msg, db::TrainingState::Finished).await
}
