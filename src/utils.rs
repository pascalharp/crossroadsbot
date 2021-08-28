use crate::{components::*, conversation::*, data::*, db, embeds::*};

use serenity::{
    builder::CreateEmbed,
    client::bridge::gateway::ShardMessenger,
    collector::reaction_collector::*,
    futures::StreamExt,
    http::CacheHttp,
    model::{
        channel::{Message, Reaction, ReactionType},
        guild::{Emoji, Guild},
        id::{EmojiId, RoleId, UserId},
        interactions::{InteractionResponseType, InteractionType},
        user::User,
    },
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
    sync::Arc,
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60 * 3);
pub const CHECK_EMOJI: char = '✅';
pub const CROSS_EMOJI: char = '❌';
pub const X_EMOJI: char = '✖';
pub const ENVELOP_EMOJI: char = '✉';
pub const WARNING_EMOJI: char = '⚠';
pub const DIZZY_EMOJI: char = '😵';
pub const RUNNING_EMOJI: char = '🏃';
pub const GREEN_CIRCLE_EMOJI: char = '🟢';
pub const RED_CIRCLE_EMOJI: char = '🔴';
pub const CONSTRUCTION_SITE_EMOJI: char = '🚧';
pub const MEMO_EMOJI: char = '📝';
pub const GREEN_SQUARE_EMOJI: char = '🟩';
pub const RED_SQUARE_EMOJI: char = '🟥';
pub const ALARM_CLOCK_EMOJI: char = '⏰';

pub enum YesOrNo {
    Yes,
    No,
}

/// Reacts with CHECK_EMOJI and CROSS_EMOJI on the provided message
pub async fn send_yes_or_no<'a>(
    cache_http: &'a impl CacheHttp,
    msg: &'a Message,
) -> Result<(Reaction, Reaction)> {
    let check = msg.react(cache_http, CHECK_EMOJI).await?;
    let cross = msg.react(cache_http, CROSS_EMOJI).await?;
    Ok((check, cross))
}

/// Awaits the CHECK_EMOJI or CROSS_EMOJI reaction on a message using the default timeout
pub async fn await_yes_or_no<'a>(
    shard_messenger: &'a impl AsRef<ShardMessenger>,
    msg: &'a Message,
    user_id: Option<UserId>,
) -> Option<YesOrNo> {
    let collector = msg
        .await_reaction(shard_messenger)
        .timeout(DEFAULT_TIMEOUT)
        .filter(|r| {
            r.emoji == ReactionType::from(CHECK_EMOJI) || r.emoji == ReactionType::from(CROSS_EMOJI)
        });

    let collector = match user_id {
        Some(u) => collector.author_id(u),
        None => collector,
    };

    match collector.await {
        None => return None,
        Some(r) => match r.as_ref() {
            ReactionAction::Added(e) => {
                if e.emoji == ReactionType::from(CHECK_EMOJI) {
                    return Some(YesOrNo::Yes);
                }
                return Some(YesOrNo::No);
            }
            _ => return None,
        },
    }
}

/// Helper struct
pub struct RoleEmoji {
    pub role: db::Role,
    pub emoji: Emoji,
}

pub type RoleEmojiMap = HashMap<EmojiId, RoleEmoji>;

/// Returns a Hashmap of of Emojis and Roles that overlap with EmojiId as key
pub async fn role_emojis(ctx: &Context, roles: Vec<db::Role>) -> Result<RoleEmojiMap> {
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

/// Verifies if the discord user has the required tier for a training
pub async fn verify_tier(
    ctx: &Context,
    training: &db::Training,
    user: &User,
) -> Result<(bool, String)> {
    let tier = training.get_tier(ctx).await;
    let tier = match tier {
        None => return Ok((true, "none".to_string())),
        Some(t) => Arc::new(t?),
    };
    let tier_mappings = tier.get_discord_roles(ctx).await?;
    let roles_set = {
        let guild = ctx
            .data
            .read()
            .await
            .get::<ConfigValuesData>()
            .unwrap()
            .main_guild_id;
        let roles = guild.member(ctx, user.id).await?.roles;
        HashSet::<RoleId>::from_iter(roles)
    };

    let passed = tier_mappings
        .iter()
        .any(|t| roles_set.contains(&RoleId::from(t.discord_role_id as u64)));
    Ok((passed, tier.name.clone()))
}

/// Looks at the user permissions and filters out trainings the user has not sufficient permissions
/// for
pub async fn filter_trainings(
    ctx: &Context,
    trainings: Vec<db::Training>,
    user: &User,
) -> Result<Vec<db::Training>> {
    let roles = {
        let guild = ctx
            .data
            .read()
            .await
            .get::<ConfigValuesData>()
            .unwrap()
            .main_guild_id;
        guild.member(ctx, user.id).await?.roles
    };

    let tiers = db::Tier::all(ctx).await?;

    let mut tier_map: HashMap<i32, Vec<db::TierMapping>> = HashMap::new();

    for t in tiers {
        let t = Arc::new(t);
        let discord_roles = t.get_discord_roles(ctx).await?;
        tier_map.insert(t.id, discord_roles);
    }

    Ok(trainings
        .into_iter()
        .filter(|tier| match tier.tier_id {
            None => true,
            Some(id) => match tier_map.get(&id) {
                None => false,
                Some(tm_vec) => tm_vec
                    .iter()
                    .any(|tm| roles.iter().any(|r| *r == (tm.discord_role_id as u64))),
            },
        })
        .collect())
}

pub fn format_training_slim(t: &db::Training) -> String {
    String::from(format!("Name: `{}`\nDate `{} UTC`", t.title, t.date,))
}

pub async fn select_roles(
    ctx: &Context,
    msg: &mut Message,
    // The user who can select
    user: &User,
    // All roles
    roles: &Vec<db::Role>,
    // HashShet with unique reprs of roles
    mut selected: HashSet<String>,
) -> Result<HashSet<String>> {
    let orig_embeds = msg
        .embeds
        .clone()
        .into_iter()
        .map(|e| CreateEmbed::from(e))
        .collect::<Vec<_>>();
    msg.edit(ctx, |m| {
        m.add_embed(|e| {
            e.0 = select_roles_embed(roles, &selected).0;
            e
        });
        m.components(|c| {
            c.set_action_rows(role_action_row(roles));
            c.add_action_row(confirm_abort_action_row());
            c
        });
        m
    })
    .await?;

    let mut interactions = msg
        .await_component_interactions(ctx)
        .author_id(user.id)
        .filter(|f| f.kind == InteractionType::MessageComponent)
        .timeout(DEFAULT_TIMEOUT)
        .await;

    loop {
        let i = interactions.next().await;
        match i {
            None => {
                msg.edit(ctx, |m| {
                    m.set_embeds(orig_embeds.clone());
                    m.add_embed(|e| {
                        e.0 = select_roles_embed(roles, &selected).0;
                        e.footer(|f| {
                            f.text(format!("Role selection timed out {}", ALARM_CLOCK_EMOJI))
                        })
                    });
                    m.components(|c| c)
                })
                .await?;
                return Err(Box::new(ConversationError::TimedOut));
            }
            Some(i) => match resolve_button_response(&i) {
                ButtonResponse::Confirm => {
                    i.create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::DeferredUpdateMessage)
                    })
                    .await?;
                    // Edit message with final selection
                    msg.edit(ctx, |m| {
                        m.set_embeds(orig_embeds);
                        m.add_embed(|e| {
                            e.0 = select_roles_embed(&roles, &selected).0;
                            e
                        });
                        m.components(|c| c)
                    })
                    .await?;
                    break;
                }
                ButtonResponse::Abort => {
                    i.create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::DeferredUpdateMessage)
                    })
                    .await?;
                    // Edit message with final selection
                    msg.edit(ctx, |m| {
                        m.set_embeds(orig_embeds);
                        m.add_embed(|e| {
                            e.0 = select_roles_embed(&roles, &selected).0;
                            e
                        });
                        m.components(|c| c)
                    })
                    .await?;
                    return Err(Box::new(ConversationError::Canceled));
                }
                ButtonResponse::Other(repr) => {
                    if selected.contains(&repr) {
                        selected.remove(&repr);
                    } else {
                        selected.insert(repr);
                    }
                    i.create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::UpdateMessage);
                        r.interaction_response_data(|d| {
                            d.embeds(orig_embeds.clone());
                            d.create_embed(|e| {
                                e.0 = select_roles_embed(roles, &selected).0;
                                e
                            })
                        })
                    })
                    .await?;
                }
            },
        }
    }

    Ok(selected)
}
