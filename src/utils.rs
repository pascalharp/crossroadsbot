use crate::{data::*, db};

use serenity::{
    model::{
        channel::Message,
        id::RoleId,
        interactions::{message_component::MessageComponentInteraction, InteractionResponseType},
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
pub const RIGHT_ARROW_EMOJI: char = '➡';
pub const LEFT_ARROW_EMOJI: char = '⬅';
pub const DOCUMENT_EMOJI: char = '🧾';
pub const LOCK_EMOJI: char = '🔒';

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
