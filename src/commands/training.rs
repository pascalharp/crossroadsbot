use super::SQUADMAKER_ROLE_CHECK;
use crate::{
    components, data, db,
    embeds::*,
    log::*,
    signup_board::SignupBoard,
    status,
    utils::{self, *},
};
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serenity::framework::standard::{
    macros::{command, group},
    ArgError, Args, CommandResult,
};
use serenity::futures::prelude::*;
use serenity::http::AttachmentType;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::try_join;

#[group]
#[prefix = "training"]
#[commands(list, show, add, set, info, download)]
pub struct Training;

#[command]
#[checks(squadmaker_role)]
#[usage = "training_name %Y-%m-%dT%H:%M:%S% training_tier [ role_identifier... ]"]
#[example = "\"Beginner Training\" 2021-05-11T19:00:00 none dps hfb banners"]
#[min_args(3)]
#[only_in(guild)]
pub async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let discord_user = &msg.author;
        let training_name = args.single_quoted::<String>().log_reply(msg)?;

        let training_time = match args.single_quoted::<chrono::NaiveDateTime>() {
            Ok(r) => r,
            Err(e) => match e {
                ArgError::Parse(_) => {
                    return LogError::new(
                        "Failed to parse date. Required Format: %Y-%m-%dT%H:%M:%S%",
                        msg,
                    )
                    .into();
                }
                _ => {
                    return Err(e.into());
                }
            },
        };

        let training_tier = args.single_quoted::<String>()?;
        let training_tier: Option<db::Tier> = {
            if training_tier.to_lowercase().eq("none") {
                None
            } else {
                Some(db::Tier::by_name(ctx, training_tier).await.log_reply(msg)?)
            }
        };

        let roles = db::Role::all_active(ctx).await.log_reply(msg)?;
        let roles_lookup: HashMap<String, &db::Role> =
            roles.iter().map(|r| (String::from(&r.repr), r)).collect();
        let mut selected: HashSet<String> = HashSet::with_capacity(roles.len());

        for r in args.iter::<String>().flatten() {
            if roles_lookup.contains_key(&r) {
                selected.insert(r);
            }
        }

        let mut m = msg
            .channel_id
            .send_message(ctx, |m| {
                m.allowed_mentions(|a| a.empty_parse());
                m.add_embed(|e| {
                    e.xstyle();
                    e.description("New Training");
                    e.field("Name", &training_name, true);
                    e.field("Date", &training_time, true);
                    e.field(
                        "Tier",
                        training_tier.as_ref().map_or("none", |t| &t.name),
                        true,
                    );
                    e
                })
            })
            .await?;

        let selected = utils::select_roles(ctx, &mut m, discord_user, &roles, selected)
            .await
            .log_reply(msg)?;

        // Do all the database stuff
        let training = {
            let training_tier_id = match training_tier {
                Some(t) => Some(t.id),
                None => None,
            };

            let training =
                db::Training::insert(ctx, training_name, training_time, training_tier_id)
                    .await
                    .log_reply(msg)?;

            for r in &selected {
                training
                    .add_role(ctx, roles_lookup.get(r).unwrap().id)
                    .await?;
            }

            training
        };

        // Update with new roles from db
        let roles = training.active_roles(ctx).await.log_reply(msg)?;

        let mut emb = training_base_embed(&training);
        embed_add_roles(&mut emb, &roles, false, true);

        m.edit(ctx, |m| {
            m.embed(|e| {
                e.0 = emb.0;
                e.author(|a| a.name(format!("{} created:", discord_user.tag())));
                e.footer(|f| f.text(format!("Training added {}", utils::CHECK_EMOJI)));
                e
            });
            m
        })
        .await?;

        Ok(())
    })
    .await
}

#[command]
#[checks(squadmaker_role)]
#[description = "Displays information about the training with the specified id"]
#[example = "121"]
#[usage = "training_id"]
pub async fn show(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let training_id = args.single::<i32>().log_reply(msg)?;

        let training = db::Training::by_id(ctx, training_id).await.log_reply(msg)?;

        match training.state {
            db::TrainingState::Created | db::TrainingState::Finished => {
                return LogError::new("Information for this training is not public", msg).into();
            }
            _ => (),
        }

        let roles = training.active_roles(ctx).await.log_unexpected_reply(msg)?;

        let tiers = {
            let tier = training.get_tier(ctx).await;
            match tier {
                None => None,
                Some(t) => {
                    let t = t.log_unexpected_reply(msg)?;
                    let r = t.get_discord_roles(ctx).await.log_unexpected_reply(msg)?;
                    Some((t, r))
                }
            }
        };

        let mut embed = training_base_embed(&training);
        training_embed_add_tier(&mut embed, &tiers, true);
        embed_add_roles(&mut embed, &roles, false, true);

        msg.channel_id
            .send_message(ctx, |m| {
                m.allowed_mentions(|am| am.empty_parse());
                m.set_embed(embed)
            })
            .await?;

        Ok(())
    })
    .await
}

#[command]
#[checks(squadmaker_role)]
#[description = "set one or multiple training(s) with the specified id to the specified state"]
#[example = "19832 started"]
#[usage = "( created | open | closed | started | finished ) training_id [ training_id ...]"]
#[min_args(2)]
pub async fn set(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let state = args.single::<db::TrainingState>().log_reply(msg)?;

        let training_id: Result<Vec<i32>, _> = args.iter::<i32>().collect();
        let training_id = training_id.log_reply(msg)?;

        // load trainings
        let futs = training_id
            .iter()
            .map(|id| db::Training::by_id(ctx, *id))
            .collect::<Vec<_>>();
        let trainings = future::try_join_all(futs).await.log_reply(msg)?;

        // set states
        let futs = trainings
            .into_iter()
            .map(|t| t.set_state(ctx, state.clone()))
            .collect::<Vec<_>>();
        future::try_join_all(futs).await.log_unexpected_reply(msg)?;

        let mut embed = serenity::builder::CreateEmbed::default();
        for id in training_id {
            let res = SignupBoard::update_training(ctx, id).await.log_reply(msg);
            match res {
                Ok(some) => match some {
                    Some(msg) => {
                        embed.field(
                            format!("Training id: {}", id),
                            format!("[Message on Board]({})", msg.link()),
                            false,
                        );
                    }
                    None => {
                        embed.field(
                            format!("Training id: {}", id),
                            "_Message removed_".to_string(),
                            false,
                        );
                    }
                },
                Err(err) => {
                    embed.field(
                        format!("Training id: {}", id),
                        format!("_Error_: {}", err.to_string()),
                        false,
                    );
                }
            }
        }

        msg.channel_id
            .send_message(ctx, |m| {
                m.embed(|e| {
                    e.0 = embed.0;
                    e.color((255, 255, 0));
                    e.description("Signup board updates:")
                })
            })
            .await?;

        status::update_status(ctx).await;

        Ok(())
    })
    .await
}

async fn list_by_state(ctx: &Context, msg: &Message, state: db::TrainingState) -> LogResult<()> {
    let trainings = db::Training::by_state(ctx, state.clone()).await?;

    // An embed can only have 25 fields. So partition the training to be sent
    // over multiple messages if needed
    let partitioned = trainings.rchunks(25).collect::<Vec<_>>();

    if partitioned.is_empty() {
        msg.reply(ctx, "No trainings found").await?;
        return Ok(());
    }

    if partitioned.len() > 1 {
        let mut msg = msg.channel_id.send_message(ctx, |m| {
            m.embed( |f| {
                f.xstyle();
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
            });
            m.components( |c| c.add_action_row(components::confirm_abort_action_row(false)))
        }).await?;
        utils::await_confirm_abort_interaction(ctx, &mut msg).await?;
    }

    let state = &state;
    for trainings in partitioned.iter() {
        msg.channel_id
            .send_message(ctx, |m| {
                m.embed(move |f| {
                    f.xstyle();
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

async fn list_amounts(ctx: &Context, msg: &Message) -> LogResult<()> {
    let (created, open, closed, started, finished) = try_join!(
        db::Training::amount_by_state(ctx, db::TrainingState::Created),
        db::Training::amount_by_state(ctx, db::TrainingState::Open),
        db::Training::amount_by_state(ctx, db::TrainingState::Closed),
        db::Training::amount_by_state(ctx, db::TrainingState::Started),
        db::Training::amount_by_state(ctx, db::TrainingState::Finished),
    )
    .log_reply(msg)?;

    let total = created + open + closed + started + finished;
    let active = open + closed + started;

    msg.channel_id.send_message(ctx, |m| {
        m.embed( |e| {
            e.xstyle();
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
    log_command(ctx, msg, || async {
        let state: Option<db::TrainingState> = match args.single_quoted::<db::TrainingState>() {
            Ok(s) => Some(s),
            Err(ArgError::Eos) => None,
            Err(e) => return Err(e.into()),
        };

        match state {
            Some(s) => return list_by_state(ctx, msg, s).await,
            None => return list_amounts(ctx, msg).await,
        }
    })
    .await
}

#[command]
#[checks(squadmaker_role)]
#[description = "lists information about the amount of sign ups and selected roles"]
#[example = "123"]
#[usage = "training_id"]
#[num_args(1)]
pub async fn info(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let training_id = args.single_quoted::<i32>().log_reply(msg)?;

        let training = db::Training::by_id(ctx, training_id).await.log_reply(msg)?;

        let signups = training
            .get_signups(ctx)
            .await
            .log_unexpected_reply(msg)?
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();

        let mut roles = training
            .all_roles(ctx)
            .await?
            .into_iter()
            .map(|r| (r, 0))
            .collect::<HashMap<db::Role, u32>>();

        let signed_up_roles = future::try_join_all(signups.iter().map(|s| s.get_roles(ctx)))
            .await?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        for sr in signed_up_roles {
            roles.entry(sr).and_modify(|e| *e += 1);
        }

        let embed = training_base_embed(&training);

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
    })
    .await
}

struct SignupCsv {
    user: db::User,
    member: Member,
    training: Arc<db::Training>,
    roles: Vec<db::Role>,
    comment: Option<String>
}

impl Serialize for SignupCsv {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Signup", 5)?;
        state.serialize_field("Gw2 Account", &self.user.gw2_id)?;
        state.serialize_field("Discord Account", &self.member.user.tag())?;
        state.serialize_field("Discord Ping", &Mention::from(&self.member).to_string())?;
        state.serialize_field("Training Name", &self.training.title)?;
        let role_str: String = self
            .roles
            .iter()
            .map(|r| r.repr.clone())
            .collect::<Vec<_>>()
            .join(", ");
        state.serialize_field("Roles", &role_str)?;
        state.serialize_field("Comment", &self.comment.clone().unwrap_or("none".to_string()))?;
        state.end()
    }
}

#[command]
#[checks(squadmaker_role)]
#[description = "download the file into a csv"]
#[example = "123"]
#[usage = "training_id"]
#[min_args(1)]
pub async fn download(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let guild_id = match ctx.data.read().await.get::<data::ConfigValuesData>() {
            Some(conf) => conf.main_guild_id,
            None => return LogError::new("Guild configuration could not be loaded", msg).into(),
        };

        let guild = PartialGuild::get(ctx, guild_id)
            .await
            .log_custom_reply(msg, "Guild information could not be loaded")?;

        let mut log: Vec<String> = vec![];
        let mut signup_csv: Vec<SignupCsv> = vec![];

        let training_ids = args
            .iter::<i32>()
            .collect::<Result<Vec<i32>, _>>()
            .log_reply(msg)?;

        let training_futs: Vec<_> = training_ids
            .into_iter()
            .map(|id| db::Training::by_id(ctx, id))
            .collect();

        let trainings = future::try_join_all(training_futs).await.log_reply(msg)?;

        for training in trainings {
            let training = Arc::new(training);
            let signups = match training.get_signups(ctx).await {
                Ok(s) => s,
                Err(e) => {
                    msg.reply(ctx, "Unexpected error loading signups").await?;
                    return Err(e.into());
                }
            };

            for s in signups {
                let user = match s.get_user(ctx).await {
                    Ok(u) => u,
                    Err(_) => {
                        log.push(format!(
                            "Error loading user entry for signup with id {}. Skipped",
                            s.id
                        ));
                        continue;
                    }
                };

                let roles = match s.get_roles(ctx).await {
                    Ok(r) => r,
                    Err(_) => {
                        log.push(format!(
                            "Error loading roles for signup with id {}. Skipped",
                            s.id
                        ));
                        continue;
                    }
                };

                if roles.is_empty() {
                    log.push(format!(
                        "No roles selected for signup with id {}. Skipped",
                        s.id
                    ));
                    continue;
                }

                let member = match guild.member(ctx, user.discord_id()).await {
                    Ok(du) => du,
                    Err(_) => {
                        log.push(format!(
                            "Did not find user with id {} in discord guild. Skipped",
                            user.discord_id()
                        ));
                        continue;
                    }
                };

                signup_csv.push(SignupCsv {
                    user,
                    member,
                    training: training.clone(),
                    roles,
                    comment: s.comment.clone(),
                });
            }
        }

        let mut wtr = csv::Writer::from_writer(vec![]);

        for s in signup_csv {
            if let Err(e) = wtr.serialize(s) {
                msg.reply(ctx, "Error converting to csv").await?;
                return Err(e.into());
            }
        }

        let wtr_inner = wtr.into_inner().log_reply(msg)?;
        let bytes_csv = String::from_utf8(wtr_inner).log_reply(msg)?.into_bytes();

        let file = AttachmentType::Bytes {
            data: Cow::from(bytes_csv),
            filename: String::from("signups.csv"),
        };

        msg.channel_id
            .send_message(ctx, |m| {
                m.content(format!(
                    "Log:\n ```\n{}\n```",
                    if log.is_empty() {
                        "No errors".to_string()
                    } else {
                        log.join("\n")
                    }
                ));
                m.add_file(file);
                m
            })
            .await?;

        Ok(())
    })
    .await
}
