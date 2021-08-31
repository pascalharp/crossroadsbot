use super::ADMIN_ROLE_CHECK;
use crate::{
    components::*,
    conversation::ConversationError,
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
    log_command(ctx, msg, || async {
        let tiers = db::Tier::all(ctx).await?;

        if tiers.is_empty() {
            return LogError::new("No Tiers set up", msg).into();
        }

        let mut tier_roles: Vec<(db::Tier, Vec<db::TierMapping>)> = vec![];

        for t in tiers {
            let m = t.get_discord_roles(ctx).await.log_unexpected_reply(msg)?;
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
                                    Mention::from(RoleId::from(r.discord_role_id as u64))
                                        .to_string()
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
    })
    .await
}

#[command]
#[checks(admin_role)]
#[description = "Add a tier permission that can be selected for trainings. The tier name may not contain any spaces"]
#[example = "tierII @TierII @TierIII"]
#[usage = "tier_name discord_role [ discord_role ... ]"]
#[only_in("guild")]
#[min_args(2)]
pub async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let tier_name = args.single_quoted::<String>().log_reply(msg)?;
        if tier_name.contains(" ") {
            return LogError::new("Tier name may not contain spaces", msg).into();
        } else if tier_name.to_lowercase().eq("none") {
            return LogError::new("_none_ is a reserved keyword and can not be used", msg).into();
        }

        let roles: Result<Vec<RoleId>, _> = args.iter::<RoleId>().collect();
        let roles = roles.log_reply(msg)?;

        let mut msg = msg
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
                    e
                });
                m.components(|c| c.add_action_row(confirm_abort_action_row()))
            })
            .await?;

        let interaction = msg
            .await_component_interaction(ctx)
            .timeout(DEFAULT_TIMEOUT)
            .await;

        match interaction {
            None => {
                msg.edit(ctx, |m| m.components(|c| c)).await?;
                return LogError::from(ConversationError::TimedOut)
                    .with_reply(&msg)
                    .into();
            }
            Some(i) => match resolve_button_response(&i) {
                ButtonResponse::Confirm => {
                    utils::clear_components(ctx, &i, &mut msg)
                        .await
                        .log_unexpected_reply(&msg)?;
                }
                ButtonResponse::Abort => {
                    utils::clear_components(ctx, &i, &mut msg)
                        .await
                        .log_unexpected_reply(&msg)?;
                    return LogError::from(ConversationError::Canceled)
                        .with_reply(&msg)
                        .into();
                }
                _ => {
                    utils::clear_components(ctx, &i, &mut msg)
                        .await
                        .log_unexpected_reply(&msg)?;
                    return LogError::from(ConversationError::Canceled)
                        .with_reply(&msg)
                        .into();
                }
            },
        };

        let tier = db::Tier::insert(ctx, tier_name)
            .await
            .log_unexpected_reply(&msg)?;

        for r in roles {
            tier.add_discord_role(ctx, *r.as_u64())
                .await
                .log_unexpected_reply(&msg)?;
        }

        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI))
            .await?;

        Ok(())
    })
    .await
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
    log_command(ctx, msg, || async {
        let tier_name = args.single_quoted::<String>().log_reply(msg)?;

        let tier = db::Tier::by_name(ctx, tier_name).await.log_reply(msg)?;
        let roles = tier.get_discord_roles(ctx).await.log_unexpected_reply(msg)?;
        let trainings = tier.get_trainings(ctx).await.log_unexpected_reply(msg)?;

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

        let mut msg = msg.channel_id.send_message(ctx, |m| {
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
            });
            m.components( |c| {
                c.add_action_row(confirm_abort_action_row())
            })
        }).await?;

        let interaction = msg.await_component_interaction(ctx)
            .timeout(DEFAULT_TIMEOUT)
            .await;

        match interaction {
            None => return LogError::from(ConversationError::TimedOut).with_reply(&msg).into(),
            Some(i) => {
                match resolve_button_response(&i) {
                    ButtonResponse::Confirm => {
                        utils::clear_components(ctx, &i, &mut msg).await.log_unexpected_reply(&msg)?;
                        ()
                    },
                    ButtonResponse::Abort => {
                        utils::clear_components(ctx, &i, &mut msg).await.log_unexpected_reply(&msg)?;
                        return LogError::from(ConversationError::Canceled).with_reply(&msg).into();
                    }
                    _ => {
                        utils::clear_components(ctx, &i, &mut msg).await.log_unexpected_reply(&msg)?;
                        return LogError::from(ConversationError::InvalidInput).with_reply(&msg).into();
                    }
                }
            }
        }

        for r in roles {
            r.delete(ctx).await?;
        }
        for t in trainings {
            t.set_tier(ctx, None).await?;
        }
        tier.delete(ctx).await.log_unexpected_reply(&msg)?;

        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI)).await?;
        Ok(())
    }).await
}

#[command]
#[sub_commands(edit_add, edit_remove)]
#[checks(admin_role)]
#[description = "Edit a tier by adding or removing a discord role"]
#[example = "add @TierII"]
#[usage = "(add | remove) @TierII"]
#[only_in("guild")]
#[num_args(0)]
pub async fn edit(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        LogError::from(ConversationError::InvalidInput)
            .with_reply(msg)
            .into()
    })
    .await
}

#[command("add")]
#[checks(admin_role)]
#[description = "Add a discord role to an already existing tier"]
#[example = "tierII @TierIII"]
#[usage = "tier_name discord_role"]
#[only_in("guild")]
#[num_args(2)]
pub async fn edit_add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let tier = args.single_quoted::<String>().log_reply(msg)?;
        let role = args.single_quoted::<RoleId>().log_reply(msg)?;

        let tier = db::Tier::by_name(ctx, tier).await.log_reply(msg)?;
        let discord_roles = tier.get_discord_roles(ctx).await.log_reply(msg)?;
        if discord_roles
            .iter()
            .any(|d| RoleId::from(d.discord_role_id as u64) == role)
        {
            return LogError::new("Discord role is already part of this tier", msg).into();
        }

        tier.add_discord_role(ctx, *role.as_u64()).await?;
        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI))
            .await?;

        Ok(())
    })
    .await
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
    log_command(ctx, msg, || async {
        let tier = args.single_quoted::<String>().log_reply(msg)?;
        let role = args.single_quoted::<RoleId>().log_reply(msg)?;

        let tier = db::Tier::by_name(ctx, tier).await.log_reply(msg)?;

        let discord_roles = tier.get_discord_roles(ctx).await?;
        let to_remove = discord_roles
            .into_iter()
            .find(|d| RoleId::from(d.discord_role_id as u64) == role);

        let to_remove = match to_remove {
            None => {
                return LogError::new("The discord role is not part of the provided tier", msg)
                    .into()
            }
            Some(i) => i,
        };

        to_remove.delete(ctx).await.log_unexpected_reply(&msg)?;

        msg.react(ctx, ReactionType::from(utils::CHECK_EMOJI))
            .await?;
        Ok(())
    })
    .await
}
