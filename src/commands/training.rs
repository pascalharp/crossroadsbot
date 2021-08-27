use super::SQUADMAKER_ROLE_CHECK;
use crate::{
    data, db, embeds,
    log::*,
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
    LogResult::command(ctx, msg, || async {
        let discord_user = &msg.author;
        let training_name = args.single_quoted::<String>()?;

        let training_time = match args.single_quoted::<chrono::NaiveDateTime>() {
            Ok(r) => r,
            Err(e) => match e {
                ArgError::Parse(_) => {
                    return Err("Failed to parse date. Required Format: %Y-%m-%dT%H:%M:%S%".into())
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
                match db::Tier::by_name(ctx, training_tier).await {
                    Err(diesel::NotFound) => return Err(
                        "Tier not found. You can use \"none\" to open the training for everyone"
                            .into(),
                    ),
                    Err(e) => return Err(e.into()),
                    Ok(t) => Some(t),
                }
            }
        };

        let roles = db::Role::all_active(ctx).await?;
        let roles_lookup: HashMap<String, &db::Role> =
            roles.iter().map(|r| (String::from(&r.repr), r)).collect();
        let mut selected: HashSet<String> = HashSet::with_capacity(roles.len());

        for a in args.iter::<String>() {
            if let Ok(r) = a {
                if roles_lookup.contains_key(&r) {
                    selected.insert(r);
                }
            }
        }

        let mut m = msg
            .channel_id
            .send_message(ctx, |m| {
                m.allowed_mentions(|a| a.empty_parse());
                m.add_embed(|e| {
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

        let selected = utils::select_roles(ctx, &mut m, discord_user, &roles, selected).await?;

        // Do all the database stuff
        let training = {
            let training_tier_id = match training_tier {
                Some(t) => Some(t.id),
                None => None,
            };

            let training =
                db::Training::insert(ctx, training_name, training_time, training_tier_id).await?;

            for r in &selected {
                training
                    .add_role(ctx, roles_lookup.get(r).unwrap().id)
                    .await?;
            }

            training
        };

        // Update with new roles from db
        let roles = training.active_roles(ctx).await?;

        let mut emb = embeds::training_base_embed(&training);
        embeds::embed_add_roles(&mut emb, &roles, false);

        m.edit(ctx, |m| {
            m.embed(|e| {
                e.0 = emb.0;
                e.author(|a| a.name(format!("{} created:", discord_user.tag())));
                e
            });
            m
        })
        .await?;

        Ok(format!("New training added with id {}", training.id).into())
    })
    .await
}

#[command]
#[checks(squadmaker_role)]
#[description = "Displays information about the training with the specified id"]
#[example = "121"]
#[usage = "training_id"]
pub async fn show(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    LogResult::command(ctx, msg, || async {

        let training_id = args.single::<i32>()?;

        let training = match db::Training::by_id(ctx, training_id).await {
            Ok(t) => t,
            Err(diesel::NotFound) => return Err("Unable to find training with this id".into()),
            Err(e) => return Err(e.into())
        };

        match training.state {
            db::TrainingState::Created | db::TrainingState::Finished => {
                return Err("Information for this training is not public".into())
            },
            _ => (),
        }

        let roles = training.active_roles(ctx).await?;

        let tiers = {
            let tier = training.get_tier(ctx).await;
            match tier {
                None => None,
                Some(t) => {
                    let t = Arc::new(t?);
                    Some( (t.clone(), Arc::new(t.get_discord_roles(ctx).await?)) )
                }
            }
        };

        let mut embed = embeds::training_base_embed(&training);
        embeds::training_embed_add_tier(&mut embed, &tiers, true);
        embeds::embed_add_roles(&mut embed, &roles, false);


        msg.channel_id
            .send_message(ctx, |m| {
                m.allowed_mentions(|am| am.empty_parse());
                m.set_embed(embed)
            })
            .await?;

        Ok(None)
    }).await
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

    let training = match db::Training::by_id(ctx, training_id).await {
        Ok(t) => t,
        Err(_) => {
            msg.reply(ctx, "Failed to load training, double check id")
                .await?;
            return Ok(());
        }
    };

    training.set_state(ctx, state).await?;

    // inform the SignupBoard
    let board_lock = {
        let read_lock = ctx.data.read().await;
        read_lock.get::<data::SignupBoardData>().unwrap().clone()
    };
    let res = {
        let mut board = board_lock.write().await;
        board.update(ctx, training_id).await
    };

    if let Err(_) = res {
        msg.reply(ctx, "State changed but error updating signup board")
            .await?;
    }
    msg.react(ctx, CHECK_EMOJI).await?;

    Ok(())
}

async fn list_by_state(ctx: &Context, msg: &Message, state: db::TrainingState) -> CommandResult {
    let author_id = msg.author.id;
    let trainings = { db::Training::by_state(ctx, state.clone()).await? };

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
        db::Training::amount_by_state(ctx, db::TrainingState::Created),
        db::Training::amount_by_state(ctx, db::TrainingState::Open),
        db::Training::amount_by_state(ctx, db::TrainingState::Closed),
        db::Training::amount_by_state(ctx, db::TrainingState::Started),
        db::Training::amount_by_state(ctx, db::TrainingState::Finished),
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

    let training = match db::Training::by_id(ctx, training_id).await {
        // TODO without Arc
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
        .get_signups(ctx)
        .await?
        .into_iter()
        .map(|s| Arc::new(s))
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

struct SignupCsv {
    user: db::User,
    member: Member,
    training: Arc<db::Training>,
    roles: Vec<db::Role>,
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
    let guild_id = match ctx.data.read().await.get::<data::ConfigValuesData>() {
        Some(conf) => conf.main_guild_id,
        None => {
            msg.reply(
                ctx,
                "Configuration not found. Main guild could not be loaded",
            )
            .await?;
            return Ok(());
        }
    };

    let guild = match PartialGuild::get(ctx, guild_id).await {
        Ok(g) => g,
        Err(e) => {
            msg.reply(ctx, "Error loading guild information").await?;
            return Err(e.into());
        }
    };

    let mut trainings: Vec<Arc<db::Training>> = Vec::with_capacity(args.len());
    let mut log: Vec<String> = vec![];
    let mut signup_csv: Vec<SignupCsv> = vec![];

    for id in args.iter::<i32>() {
        match id {
            Ok(id) => match db::Training::by_id(ctx, id).await {
                Ok(t) => trainings.push(Arc::new(t)),
                Err(diesel::NotFound) => {
                    msg.reply(ctx, format!("Training with id {} not found", id))
                        .await?;
                    return Ok(());
                }
                Err(e) => {
                    msg.reply(ctx, "Unexpected error loading training").await?;
                    return Err(e.into());
                }
            },
            Err(_) => {
                msg.reply(ctx, "Failed to parse training id").await?;
                return Ok(());
            }
        }
    }

    for training in trainings {
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
                    log.push(String::from(format!(
                        "Error loading user entry for signup with id {}. Skipped",
                        s.id
                    )));
                    continue;
                }
            };

            let s = Arc::new(s);
            let roles = match s.clone().get_roles(ctx).await {
                Ok(v) => v.into_iter().collect::<Vec<db::Role>>(),
                Err(_) => {
                    log.push(String::from(format!(
                        "Error loading roles for signup with id {}. Skipped",
                        s.id
                    )));
                    continue;
                }
            };

            if roles.is_empty() {
                log.push(String::from(format!(
                    "No roles selected for signup with id {}. Skipped",
                    s.id
                )));
                continue;
            }

            let member = match guild.member(ctx, user.discord_id()).await {
                Ok(du) => du,
                Err(_) => {
                    log.push(String::from(format!(
                        "Did not find user with id {} in discord guild. Skipped",
                        user.discord_id()
                    )));
                    continue;
                }
            };

            let training = training.clone();

            signup_csv.push(SignupCsv {
                user,
                member,
                training,
                roles,
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

    let wtr_inner = match wtr.into_inner() {
        Ok(w) => w,
        Err(e) => {
            msg.reply(ctx, "Unexpected error").await?;
            return Err(e.into());
        }
    };

    let bytes_csv = match String::from_utf8(wtr_inner) {
        Ok(s) => s.into_bytes(),
        Err(e) => {
            msg.reply(ctx, "Unexpected error").await?;
            return Err(e.into());
        }
    };

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
}
