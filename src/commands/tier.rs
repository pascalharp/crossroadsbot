use super::ADMIN_ROLE_CHECK;
use crate::{
    db,
    log::*,
    utils::{self, *},
};
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::sync::Arc;

#[group]
#[prefix = "tier"]
#[commands(list, add, remove, edit)]
pub struct Tier;

#[command]
#[checks(admin_role)]
#[description = "Lists all tiers and their corresponding discord roles"]
#[example = ""]
#[usage = ""]
#[only_in("guild")]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    let tiers = db::Tier::all(ctx).await?;

    let mut tier_roles: Vec<(Arc<db::Tier>, Vec<db::TierMapping>)> = vec![];

    for t in tiers {
        let t = Arc::new(t);
        let m = t.get_discord_roles(ctx).await?;
        tier_roles.push((t, m));
    }

    // List tiers with more roles first.It feels more inclusive =D
    tier_roles.sort_by(|(_, a), (_, b)| b.len().cmp(&a.len()));

    msg.channel_id
        .send_message(ctx, |m| {
            m.allowed_mentions(|am| am.empty_parse());
            m.embed(|e| {
                e.description("Current Tiers for trainings");
                e.fields(tier_roles.into_iter().map(|(t, r)| {
                    (
                        String::from(&t.name),
                        r.iter()
                            .map(|r| {
                                Mention::from(RoleId::from(r.discord_role_id as u64)).to_string()
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                        true,
                    )
                }))
            })
        })
        .await?;

    let res: LogResult = Ok("Success".into());
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    res.cmd_result()
}

async fn _add(ctx: &Context, channel: ChannelId, author: UserId, mut args: Args) -> LogResult {
    let tier_name = match args.single_quoted::<String>() {
        Err(_) => {
            return Ok("Unable to parse tier name".into());
        }
        Ok(t) => {
            if t.contains(" ") {
                return Ok("Tier name may not contain spaces".into());
            } else if t.to_lowercase().eq("none") {
                return Ok("none is a reserved keyword and can not be used".into());
            }
            t
        }
    };

    let roles: Vec<_> = match args.iter::<RoleId>().collect() {
        Err(_) => {
            return Ok("Unable to parse provide discord role".into());
        }
        Ok(v) => v,
    };

    let msg = channel
        .send_message(ctx, |m| {
            m.allowed_mentions(|am| am.empty_parse());
            m.embed(|e| {
                e.description("New Tier role");
                e.field("Tier name", &tier_name, false);
                e.field(
                    "Discord Roles",
                    roles
                        .iter()
                        .map(|r| Mention::from(*r).to_string())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    false,
                );
                e.footer(|f| {
                    f.text(format!(
                        "{} to confirm. {} to abort",
                        CHECK_EMOJI, CROSS_EMOJI
                    ))
                });
                e
            });
            m
        })
        .await?;

    utils::send_yes_or_no(ctx, &msg).await?;
    match utils::await_yes_or_no(ctx, &msg, Some(author)).await {
        None => {
            return Ok("Timed out".into());
        }
        Some(r) => match r {
            utils::YesOrNo::No => {
                return Ok("Aborted".into());
            }
            _ => (),
        },
    }

    let tier = db::Tier::insert(ctx, tier_name).await?;

    for r in roles {
        tier.add_discord_role(ctx, *r.as_u64()).await?;
    }

    Ok("Tier added".into())
}

#[command]
#[checks(admin_role)]
#[description = "Add a tier permission that can be selected for trainings. The tier name may not contain any spaces"]
#[example = "tierII @TierII @TierIII"]
#[usage = "tier_name [ discord_role ... ]"]
#[only_in("guild")]
#[min_args(2)]
pub async fn add(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let res = _add(ctx, msg.channel_id, msg.author.id, args).await;
    res.reply(ctx, msg).await?;
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    res.cmd_result()
}

async fn _remove(ctx: &Context, channel: ChannelId, author: UserId, mut args: Args) -> LogResult {
    let tier_name = match args.single_quoted::<String>() {
        Ok(t) => t,
        Err(_) => {
            return Ok("Unable to parse tier name".into());
        }
    };

    let tier = match db::Tier::by_name(ctx, tier_name).await {
        Ok(t) => Arc::new(t),
        Err(diesel::NotFound) => return Ok("Tier not found".into()),
        Err(e) => return Err(e.into()),
    };
    let roles = tier.get_discord_roles(ctx).await?;
    let trainings = tier.get_trainings(ctx).await?;

    let (created, open, closed, started, finished) = trainings.iter().fold(
        (0u32, 0u32, 0u32, 0u32, 0u32),
        |(cr, o, cl, s, f), t| match t.state {
            db::TrainingState::Created => (cr + 1, o, cl, s, f),
            db::TrainingState::Open => (cr, o + 1, cl, s, f),
            db::TrainingState::Closed => (cr, o, cl + 1, s, f),
            db::TrainingState::Started => (cr, o, cl, s + 1, f),
            db::TrainingState::Finished => (cr, o, cl, s, f + 1),
        },
    );

    let msg = channel.send_message(ctx, |m| {
        m.embed( |e| {
            e.description("Removing tier");
            e.field(
                "Tier information",
                format!("Name: {}\nId: {}", &tier.name, &tier.id),
                false
            );
            e.field(
            "WARNING",
            format!(
            "Removing this tier will remove all tier requirement for associated trainings:\nCreated: {}\nOpen: {}\nClosed: {}\nStarted: {}\nFinished: {}",
            created, open, closed, started, finished),
            false)
        })
    }).await?;

    utils::send_yes_or_no(ctx, &msg).await?;
    match utils::await_yes_or_no(ctx, &msg, Some(author)).await {
        None => {
            return Ok("Timed out".into());
        }
        Some(r) => match r {
            utils::YesOrNo::Yes => (),
            utils::YesOrNo::No => {
                return Ok("Aborted".into());
            }
        },
    }

    for r in roles {
        r.delete(ctx).await?;
    }
    for t in trainings {
        t.set_tier(ctx, None).await?;
    }
    match Arc::try_unwrap(tier) {
        Ok(t) => {
            t.delete(ctx).await?;
        }
        Err(_) => {
            return Ok("Unexpected internal error unwrapping Arc".into());
        }
    };
    Ok("Tier removed".into())
}

#[command]
#[checks(admin_role)]
#[aliases("rm")]
#[description = "Remove a tier role. This will remove the tier requirement from all trainings, that currently have this tier requirement set."]
#[example = "tierII"]
#[usage = "tier_name"]
#[only_in("guild")]
#[num_args(1)]
pub async fn remove(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let res = _remove(ctx, msg.channel_id, msg.author.id, args).await;
    res.reply(ctx, msg).await?;
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    res.cmd_result()
}

#[command]
#[sub_commands(edit_add, edit_remove)]
#[checks(admin_role)]
#[description = "Edit a tier by adding or removing a discord role"]
#[example = "add @TierII"]
#[usage = "(add | remove) @TierII"]
#[only_in("guild")]
#[num_args(0)]
pub async fn edit(_: &Context, _: &Message, _: Args) -> CommandResult {
    Ok(())
}

async fn _edit_add(ctx: &Context, mut args: Args) -> LogResult {
    let tier = args.single_quoted::<String>()?;
    let role = match args.single_quoted::<RoleId>() {
        Ok(r) => r,
        Err(_) => {
            return Ok("Failed to parse discord role".into());
        }
    };

    let tier = match db::Tier::by_name(ctx, tier).await {
        Ok(t) => Arc::new(t),
        Err(diesel::NotFound) => return Ok("Failed to load tier. Check spelling".into()),
        Err(e) => return Err(e.into()),
    };
    let discord_roles = tier.get_discord_roles(ctx).await?;
    if discord_roles
        .iter()
        .any(|d| RoleId::from(d.discord_role_id as u64) == role)
    {
        return Ok("Discord role is already part of that tier".into());
    }

    tier.add_discord_role(ctx, *role.as_u64()).await?;
    Ok("Discord role added to Tier".into())
}

#[command("add")]
#[checks(admin_role)]
#[description = "Add a discord role to an already existing tier"]
#[example = "tierII @TierIII"]
#[usage = "tier_name discord_role"]
#[only_in("guild")]
#[num_args(2)]
pub async fn edit_add(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let res = _edit_add(ctx, args).await;
    res.reply(ctx, msg).await?;
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    res.cmd_result()
}

async fn _edit_remove(ctx: &Context, mut args: Args) -> LogResult {
    let tier = args.single_quoted::<String>()?;
    let role = match args.single_quoted::<RoleId>() {
        Ok(r) => r,
        Err(_) => {
            return Ok("Failed to parse discord role".into());
        }
    };

    let tier = match db::Tier::by_name(ctx, tier).await {
        Ok(t) => Arc::new(t),
        Err(diesel::NotFound) => return Ok("Failed to load tier. Check spelling".into()),
        Err(e) => return Err(e.into()),
    };
    let discord_roles = tier.get_discord_roles(ctx).await?;
    let to_remove = discord_roles
        .into_iter()
        .find(|d| RoleId::from(d.discord_role_id as u64) == role);
    let to_remove = match to_remove {
        None => return Ok("Provided discord role is not part of the provided tier".into()),
        Some(i) => i,
    };

    to_remove.delete(ctx).await?;
    return Ok("Discord role removed".into());
}

#[command("remove")]
#[aliases("rm")]
#[checks(admin_role)]
#[description = "Remove a discord role from an already existing tier"]
#[example = "tierII @TierI"]
#[usage = "tier_name discord_role"]
#[only_in("guild")]
#[num_args(2)]
pub async fn edit_remove(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let res = _edit_remove(ctx, args).await;
    res.reply(ctx, msg).await?;
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    res.cmd_result()
}
