use super::{ConfigValuesData, ADMIN_ROLE_CHECK, CHECK_EMOJI, CROSS_EMOJI, DEFAULT_TIMEOUT};
use crate::{db, utils};
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::prelude::*;
use serenity::prelude::*;

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
    let conn = db::connect();
    let tiers = db::get_tiers(&conn)?;

    let mut tier_roles: Vec<(db::Tier, Vec<db::TierMapping>)> = tiers
        .into_iter()
        .map(|t| {
            let mapping = t.get_discord_roles(&conn)?;
            Ok((t, mapping))
        })
        .collect::<Result<_, diesel::result::Error>>()?;

    // List tiers with more roles first.It feels more inclusive =D
    tier_roles.sort_by(|(_, a), (_, b)| b.len().cmp(&a.len()));

    msg.channel_id
        .send_message(ctx, |m| {
            m.allowed_mentions(|am| am.empty_parse());
            m.embed(|e| {
                e.description("Current Tiers for trainings");
                e.fields(tier_roles.into_iter().map(|(t, r)| {
                    (
                        t.name,
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

    msg.react(ctx, CHECK_EMOJI).await?;
    msg.react(ctx, CROSS_EMOJI).await?;

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

    let conn = db::connect();
    let tier = db::add_tier(&conn, &tier_name);
    let tier = match tier {
        Err(e) => {
            msg.reply(ctx, "Error adding tier to database").await?;
            return Err(e.into());
        }
        Ok(t) => t,
    };

    for r in roles {
        tier.add_discord_role(&conn, *r.as_u64())?;
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

    let conn = db::connect();
    let tier = db::get_tier(&conn, &tier_name);
    let tier = match tier {
        Ok(t) => t,
        Err(e) => {
            msg.reply(
                ctx,
                "Failed to load tier from database. Double check tier name",
            )
            .await?;
            return Err(e.into());
        }
    };
    let roles = tier.get_discord_roles(&conn)?;
    let trainings = tier.get_trainings(&conn)?;
    drop(conn);

    let (created, open, closed, started, finished) =
        trainings
            .iter()
            .fold((0, 0, 0, 0, 0), |(cr, o, cl, s, f), t| match t.state {
                db::TrainingState::Created => (cr + 1, o, cl, s, f),
                db::TrainingState::Open => (cr, o + 1, cl, s, f),
                db::TrainingState::Closed => (cr, o, cl + 1, s, f),
                db::TrainingState::Started => (cr, o, cl, s + 1, f),
                db::TrainingState::Finished => (cr, o, cl, s, f + 1),
            });

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

    let conn = db::connect();
    for r in roles {
        r.delete(&conn)?;
    }
    for t in trainings {
        t.set_tier(&conn, None)?;
    }
    tier.delete(&conn)?;

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

    let conn = db::connect();
    let tier = db::get_tier(&conn, &tier);
    let tier = match tier {
        Ok(t) => t,
        Err(_) => {
            msg.reply(ctx, "Failed to load tier. Check spelling")
                .await?;
            return Ok(());
        }
    };
    let discord_roles = tier.get_discord_roles(&conn)?;
    if discord_roles
        .iter()
        .any(|d| RoleId::from(d.discord_role_id as u64) == role)
    {
        msg.reply(ctx, "That discord role is already part of that tier")
            .await?;
        return Ok(());
    }

    let added = tier.add_discord_role(&conn, *role.as_u64());
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

    let conn = db::connect();
    let tier = db::get_tier(&conn, &tier);
    let tier = match tier {
        Ok(t) => t,
        Err(_) => {
            msg.reply(ctx, "Failed to load tier. Check spelling")
                .await?;
            return Ok(());
        }
    };
    let discord_roles = tier.get_discord_roles(&conn)?;
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

    let removed = to_remove.delete(&conn);
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
