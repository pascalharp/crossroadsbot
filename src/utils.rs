use crate::{components::*, conversation::*, data::*, db, embeds::*, log::*};

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
        interactions::{
            message_component::MessageComponentInteraction, InteractionResponseType,
            InteractionType,
        },
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

// Using Deferred since updating the message and Interaction Response
// doesnt update the original message
pub async fn clear_components(
    ctx: &Context,
    interaction: &MessageComponentInteraction,
    msg: &mut Message,
) -> Result<()> {
    interaction
        .create_interaction_response(ctx, |r| {
            r.kind(InteractionResponseType::DeferredUpdateMessage)
        })
        .await?;

    msg.edit(ctx, |m| m.components(|c| c)).await?;

    Ok(())
}

pub async fn await_confirm_abort_interaction(ctx: &Context, msg: &mut Message) -> _LogResult<()> {
    let interaction = msg
        .await_component_interaction(ctx)
        .timeout(DEFAULT_TIMEOUT)
        .await;
    match interaction {
        None => return Err(ConversationError::TimedOut).log_reply(&msg),
        Some(i) => match resolve_button_response(&i) {
            ButtonResponse::Confirm => {
                clear_components(ctx, &i, msg).await.log_only()?;
            }
            ButtonResponse::Abort => {
                clear_components(ctx, &i, msg).await.log_only()?;
                return Err(ConversationError::Canceled).log_reply(&msg);
            }
            _ => {
                clear_components(ctx, &i, msg).await.log_only()?;
                return Err(ConversationError::InvalidInput).log_reply(&msg);
            }
        },
    }
    Ok(())
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
            if selected.is_empty() {
                e.footer(|f| f.text(format!("{} Select at least one role", WARNING_EMOJI)));
            }
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
                    // only accept if at least one role selectec
                    if !selected.is_empty() {
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
                                if selected.is_empty() {
                                    e.footer(|f| {
                                        f.text(format!(
                                            "{} Select at least one role",
                                            WARNING_EMOJI
                                        ))
                                    });
                                }
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
