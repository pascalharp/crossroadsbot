use super::ADMIN_ROLE_CHECK;
use crate::{
    db,
    utils::{self, *}
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
    let tiers = db::Tier::all().await?;

    let mut tier_roles: Vec<(Arc<db::Tier>, Vec<db::TierMapping>)> = vec![];

    for t in tiers {
        let t = Arc::new(t);
        let m = t.clone().get_discord_roles().await?;
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

    Ok(())
}

#[command]
#[checks(admin_role)]
#[description = "Add a tier permission that can be selected for trainings. The tier name may not contain any spaces"]
#[example = "tierII @TierII @TierIII"]
#[usage = "tier_name [ discord_role ... ]"]
#[only_in("guild")]
#[min_args(2)]
pub async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let author_id = msg.author.id;
    let tier_name = match args.single_quoted::<String>() {
        Err(_) => {
            msg.reply(ctx, "Unable to parse tier name").await?;
            return Ok(());
        }
        Ok(t) => {
            if t.contains(" ") {
                msg.reply(ctx, "Tier name may not contain spaces").await?;
                return Ok(());
            } else if t.to_lowercase().eq("none") {
                msg.reply(ctx, "none is a reserved keyword and can not be used")
                    .await?;
                return Ok(());
            }
            t
        }
    };

    let roles: Vec<_> = match args.iter::<RoleId>().collect() {
        Err(_) => {
            msg.reply(ctx, "Unable to parse provide discord role")
                .await?;
            return Ok(());
        }
        Ok(v) => v,
    };

    let msg = msg
        .channel_id
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
    match utils::await_yes_or_no(ctx, &msg, Some(author_id)).await {
        None => {
            msg.reply(ctx, "Timed out").await?;
            return Ok(());
        }
        Some(r) => match r {
            utils::YesOrNo::No => {
                msg.reply(ctx, "Aborted").await?;
                return Ok(());
            }
            _ => (),
        },
    }

    let new_tier = db::NewTier {
        name: String::from(tier_name),
    };
    let tier = match new_tier.add().await {
        Err(e) => {
            msg.reply(ctx, "Error adding tier to database").await?;
            return Err(e.into());
        }
        Ok(t) => t,
    };

    for r in roles {
        tier.add_discord_role(*r.as_u64()).await?;
    }

    msg.reply(ctx, "Tier added").await?;

    Ok(())
}

#[command]
#[checks(admin_role)]
#[aliases("rm")]
#[description = "Remove a tier role. This will remove the tier requirement from all trainings, that currently have this tier requirement set."]
#[example = "tierII"]
#[usage = "tier_name"]
#[only_in("guild")]
#[num_args(1)]
pub async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let author_id = msg.author.id;
    let tier_name = match args.single_quoted::<String>() {
        Ok(t) => t,
        Err(_) => {
            msg.reply(ctx, "Unable to parse tier name").await?;
            return Ok(());
        }
    };

    let tier = match db::Tier::by_name(tier_name).await {
        Ok(t) => Arc::new(t),
        Err(e) => {
            msg.reply(
                ctx,
                "Failed to load tier from database. Double check tier name",
            )
            .await?;
            return Err(e.into());
        }
    };
    let roles = tier.clone().get_discord_roles().await?;
    let trainings = tier.clone().get_trainings().await?;

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

    let msg = msg.channel_id.send_message(ctx, |m| {
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
    match utils::await_yes_or_no(ctx, &msg, Some(author_id)).await {
        None => {
            msg.reply(ctx, "Timed out").await?;
            return Ok(());
        }
        Some(r) => match r {
            utils::YesOrNo::Yes => (),
            utils::YesOrNo::No => {
                msg.reply(ctx, "Aborted").await?;
                return Ok(());
            }
        },
    }

    for r in roles {
        r.delete().await?;
    }
    for t in trainings {
        t.set_tier(None).await?;
    }
    match Arc::try_unwrap(tier) {
        Ok(t) => {
            t.delete().await?;
        }
        Err(_) => {
            msg.reply(ctx, "Dangling reference to tier. Failed to delete =(")
                .await?;
            return Ok(());
        }
    };

    msg.reply(ctx, "Tier removed").await?;

    Ok(())
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

#[command("add")]
#[checks(admin_role)]
#[description = "Add a discord role to an already existing tier"]
#[example = "tierII @TierIII"]
#[usage = "tier_name discord_role"]
#[only_in("guild")]
#[num_args(2)]
pub async fn edit_add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let tier = args.single_quoted::<String>()?;
    let role = match args.single_quoted::<RoleId>() {
        Ok(r) => r,
        Err(_) => {
            msg.reply(ctx, "Failed to parse discord role").await?;
            return Ok(());
        }
    };

    let tier = match db::Tier::by_name(tier).await {
        Ok(t) => Arc::new(t),
        Err(_) => {
            msg.reply(ctx, "Failed to load tier. Check spelling")
                .await?;
            return Ok(());
        }
    };
    let discord_roles = tier.clone().get_discord_roles().await?;
    if discord_roles
        .iter()
        .any(|d| RoleId::from(d.discord_role_id as u64) == role)
    {
        msg.reply(ctx, "That discord role is already part of that tier")
            .await?;
        return Ok(());
    }

    let added = tier.clone().add_discord_role(*role.as_u64()).await;
    match added {
        Ok(_) => {
            msg.reply(ctx, "Discord role added").await?;
            return Ok(());
        }
        Err(e) => {
            msg.reply(ctx, "Failed to add discord role").await?;
            return Err(e.into());
        }
    }
}

#[command("remove")]
#[aliases("rm")]
#[checks(admin_role)]
#[description = "Remove a discord role from an already existing tier"]
#[example = "tierII @TierI"]
#[usage = "tier_name discord_role"]
#[only_in("guild")]
#[num_args(2)]
pub async fn edit_remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let tier = args.single_quoted::<String>()?;
    let role = match args.single_quoted::<RoleId>() {
        Ok(r) => r,
        Err(_) => {
            msg.reply(ctx, "Failed to parse discord role").await?;
            return Ok(());
        }
    };

    let tier = match db::Tier::by_name(tier).await {
        Ok(t) => Arc::new(t),
        Err(_) => {
            msg.reply(ctx, "Failed to load tier. Check spelling")
                .await?;
            return Ok(());
        }
    };
    let discord_roles = tier.get_discord_roles().await?;
    let to_remove = discord_roles
        .into_iter()
        .find(|d| RoleId::from(d.discord_role_id as u64) == role);
    let to_remove = match to_remove {
        None => {
            msg.reply(
                ctx,
                "Provided discord role is not part of the provided tier",
            )
            .await?;
            return Ok(());
        }
        Some(i) => i,
    };

    let removed = to_remove.delete().await;
    match removed {
        Ok(_) => {
            msg.reply(ctx, "Discord role removed").await?;
            return Ok(());
        }
        Err(e) => {
            msg.reply(ctx, "Failed to remove discord role").await?;
            return Err(e.into());
        }
    }
}
