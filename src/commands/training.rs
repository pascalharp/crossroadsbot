use super::{ConfigValuesData, ADMIN_ROLE_CHECK, CHECK_EMOJI, CROSS_EMOJI, DEFAULT_TIMEOUT};
use crate::db;
use serenity::collector::reaction_collector::ReactionAction;
use serenity::framework::standard::{
    macros::{command, group},
    ArgError, Args, CommandResult,
};
use serenity::futures::prelude::*;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[group]
#[prefix = "training"]
#[commands(add)]
pub struct Training;

type BoxResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct RoleEmoji {
    role: db::models::Role,
    emoji: Emoji,
}

// --- Helper functions ---
/// Returns a Hashmap of of Emojis and Roles that overlap with EmojiId as key
async fn role_emojis(
    ctx: &Context,
    roles: Vec<db::models::Role>,
) -> BoxResult<HashMap<EmojiId, RoleEmoji>> {
    let mut map = HashMap::new();
    let emojis_guild_id = ctx
        .data
        .read()
        .await
        .get::<ConfigValuesData>()
        .unwrap()
        .emoji_guild_id;
    let emoji_guild = Guild::get(ctx, emojis_guild_id).await?;
    let emojis = emoji_guild.emojis;

    for r in roles {
        if let Some(e) = emojis.get(&EmojiId::from(r.emoji as u64)) {
            let role_emoji = RoleEmoji {
                role: r,
                emoji: e.clone(),
            };
            map.insert(e.id, role_emoji);
        }
    }

    Ok(map)
}

// Helper function to  update add_training embed message
async fn update_add_training(
    ctx: &Context,
    msg: &mut Message,
    role_emojis: &HashMap<EmojiId, RoleEmoji>,
    selected: &HashSet<EmojiId>,
    training_name: &str,
    training_time: &chrono::NaiveDateTime,
) -> BoxResult<()> {
    msg.edit(ctx, |m| {
        m.embed(|e| {
            e.description("New Training");
            e.field(
                "Details",
                format!("{}\n{}", training_name, training_time),
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
#[usage = "training_name %Y-%m-%dT%H:%M:%S% [ role_identifier... ]"]
#[example = "\"Beginner Training\" 2021-05-11T19:00:00 pdps cdps banners"]
#[min_args(2)]
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
    let roles = {
        let conn = db::connect();
        db::get_roles(&conn)?
    };
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
        let conn = db::connect();
        let training = db::add_training(&conn, &training_name, &training_time);
        let training = match training {
            Err(e) => {
                msg.reply(ctx, format!("{}", e)).await?;
                return Ok(());
            }
            Ok(t) => t,
        };

        for r in re.values() {
            if selected.contains(&r.emoji.id) {
                let training_role = training.add_role(&conn, r.role.id);
                match training_role {
                    Err(e) => {
                        msg.reply(ctx, format!("{}", e)).await?;
                        return Ok(());
                    }
                    _ => (),
                }
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
                e.field("Open", training.open, false);
                e
            });
            m
        })
        .await?;

    Ok(())
}
