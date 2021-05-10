use super::{ConfigValuesData, ADMIN_ROLE_CHECK, CHECK_EMOJI, CROSS_EMOJI, DEFAULT_TIMEOUT};
use crate::db;
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::prelude::*;
use serenity::prelude::*;

#[group]
#[prefix = "tier"]
#[commands(list, add)]
pub struct Tier;

#[command]
#[checks(admin_role)]
#[description = "Lists all tiers and their corresponding discord roles"]
#[example = ""]
#[usage = ""]
#[only_in("guild")]
#[min_args(0)]
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
    tier_roles.sort_by( | (_,a), (_,b) | b.len().cmp(&a.len()));

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
#[min_args(1)]
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
                msg.reply(ctx, "none is a reserved keyword and can not be used").await?;
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

    let react = msg
        .await_reaction(ctx)
        .author_id(author_id)
        .timeout(DEFAULT_TIMEOUT)
        .filter(|r| {
            r.emoji == ReactionType::from(CHECK_EMOJI) || r.emoji == ReactionType::from(CROSS_EMOJI)
        })
        .await;

    match react {
        None => {
            msg.reply(ctx, "Timed out").await?;
            return Ok(());
        }
        Some(r) => {
            if r.as_inner_ref().emoji == ReactionType::from(CROSS_EMOJI) {
                msg.reply(ctx, "Aborted").await?;
                return Ok(());
            }
        }
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
