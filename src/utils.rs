use crate::commands::{ConfigValuesData, CHECK_EMOJI, CROSS_EMOJI, DEFAULT_TIMEOUT};
use crate::db;
use serenity::{
    prelude::*,
    client::bridge::gateway::ShardMessenger,
    collector::reaction_collector::ReactionAction,
    http::CacheHttp,
    model::{
        channel::{Message, Reaction, ReactionType},
        user::User,
        id::{UserId, RoleId},
    },
};
use std::{
    sync::Arc,
    collections::{HashMap,HashSet},
    iter::FromIterator,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

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

/// Verifies if the discord user has the required tier for a training
pub async fn verify_tier(ctx: &Context, training: &db::Training, user: &User) -> Result<bool> {
    let tier = training.get_tier().await;
    let tier_mappings = match tier {
        None => return Ok(true),
        Some(t) => {
            Arc::new(t?).get_discord_roles().await?
        }
    };
    let roles_set = {
        let guild = ctx.data.read().await.get::<ConfigValuesData>().unwrap().main_guild_id;
        let roles = guild.member(ctx, user.id).await?.roles;
        HashSet::<RoleId>::from_iter(roles)
    };

    let passed = tier_mappings
        .iter()
        .any(|t| {
            roles_set.contains(&RoleId::from(t.discord_role_id as u64))
        });
    Ok(passed)
}

/// Looks at the user permissions and filters out trainings the user has not sufficient permissions
/// for
pub async fn filter_trainings(ctx: &Context, trainings: Vec<db::Training>, user: &User) -> Result<Vec<db::Training>> {
    let roles = {
        let guild = ctx.data.read().await.get::<ConfigValuesData>().unwrap().main_guild_id;
        guild.member(ctx, user.id).await?.roles
    };

    let tiers = db::Tier::all().await?;

    let mut tier_map: HashMap<i32, Vec<db::TierMapping>> = HashMap::new();

    for t in tiers {
        let t = Arc::new(t);
        let discord_roles = t.clone().get_discord_roles().await?;
        tier_map.insert(t.id, discord_roles);
    }

    Ok(trainings.into_iter().filter( |tier| {
        match tier.tier_id {
            None => true,
            Some(id) => {
                match tier_map.get(&id) {
                    None => false,
                    Some(tm_vec) => tm_vec.iter().any( |tm| {
                        roles.iter().any(|r| { *r == (tm.discord_role_id as u64) })
                    }),
                }
            }
        }
    }).collect())
}

pub fn format_training_slim(t: &db::Training) -> String {
    String::from(format!(
        "Name: `{}`\nDate `{} UTC`",
        t.title,
        t.date,
    ))
}
