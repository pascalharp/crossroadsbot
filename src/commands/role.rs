use super::SQUADMAKER_ROLE_CHECK;
use crate::{
    components, conversation::ConversationError, data::ConfigValuesData, db, embeds, embeds::*,
    log::*, utils::CHECK_EMOJI, utils::DEFAULT_TIMEOUT,
};
use serenity::builder::CreateEmbed;
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::futures::StreamExt;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::collections::HashSet;

#[group]
#[prefix = "role"]
#[commands(add, remove, list)]
pub struct Role;

#[command]
#[checks(squadmaker_role)]
#[description = "Add a role by providing a full role name and a role short identifier (without spaces)"]
#[example = "\"Power DPS\" pdps"]
#[usage = "full_name identifier"]
#[only_in("guild")]
#[num_args(2)]
pub async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let author = &msg.author;
        let role_name = args.single_quoted::<String>().log_reply(msg)?;
        let role_repr = args.single_quoted::<String>().log_reply(msg)?;

        if role_repr.contains(' ') {
            return LogError::new("Identifier must not contain spaces", msg).into();
        }

        // load all active roles from db
        let roles = db::Role::all_active(ctx).await.log_reply(msg)?;
        // check if repr already used
        if roles.iter().any(|r| r.repr.eq(&role_repr)) {
            return LogError::new(
                "A role with the same repr already exists. The repr has to be unique",
                msg,
            )
            .into();
        }

        // collecting into a HashSet removes duplicates
        let db_emojis: HashSet<EmojiId> = roles
            .iter()
            .map(|r| EmojiId::from(r.emoji as u64))
            .collect();

        // load all emojis from discord emoji guild
        let gid = ctx
            .data
            .read()
            .await
            .get::<ConfigValuesData>()
            .unwrap()
            .emoji_guild_id;
        let emoji_guild = Guild::get(ctx, gid).await.log_reply(msg)?;

        // Load emojis from emoji guild for non nitro users to select
        // Emojis from other servers are also allowed
        let available: Vec<Emoji> = emoji_guild
            .emojis
            .into_iter()
            .map(|(_, v)| v)
            .collect();

        let mut emb = CreateEmbed::default();
        emb.xstyle();
        emb.description("New Role");
        emb.field("Full role name", &role_name, true);
        emb.field("Short role identifier", &role_repr, true);
        emb.field(
            "Emojis already in use",
            if db_emojis.is_empty() {
                "_None_".to_string()
            } else {
                db_emojis.into_iter().map( |e| Mention::from(e).to_string() ).collect::<Vec<_>>().join("|")
            },
            false);
        emb.footer(|f| {
            f.text(
                format!(
                "{}\n{}\n{}",
                "Emojis can be used multiple times, but should be avoided for roles that can appear in the same training",
                "Only custom emojis are allowed. Make sure that the bot has access to the emoji!",
                "Choose an emoji (no necessarily a listed one)"))
        });

        let mut msg = msg
            .channel_id
            .send_message(ctx, |m| m.set_embed(emb.clone()))
            .await?;

        // Present all available emojis
        // Not using buttons here, since there is a limited amount for them
        // and there might be a lot of emojis
        // Also doing this in parallel so emojis can already be selected while bot still suggests
        // them
        let ctx_task = ctx.clone();
        let msg_task = msg.clone();
        let cancel = std::sync::Arc::new(tokio::sync::Mutex::new(false));
        let cancel_task = cancel.clone();
        tokio::spawn(async move {
            for (i, e) in available.into_iter().enumerate() {
                if *cancel_task.lock().await { return }
                // limit is 20 reactions, leave one for user
                if i >= 19 { return }
                msg_task.react(&ctx_task, e).await.ok();
            }
        });

        // Wait for emoji
        let emoji = msg
            .await_reaction(ctx)
            .timeout(DEFAULT_TIMEOUT)
            .author_id(author.id)
            .filter(move |r| {
                matches!(r.emoji, ReactionType::Custom {
                        animated: _,
                        id: _,
                        name: _,
                    })
            })
            .await;

        let emoji_id = match emoji {
            None => {
                return Err(ConversationError::TimedOut).log_reply(&msg);
            }
            Some(r) => {
                match &r.as_inner_ref().emoji {
                    ReactionType::Custom {
                        animated: _,
                        id,
                        name: _,
                    } => *id,
                    // Should never occur since filtered already filtered
                    _ => return LogError::new("Invalid emoji. Only custom emojis are allowed for roles", &msg).into(),
                }
            }
        };

        *cancel.lock().await = true;

        msg.delete_reactions(ctx).await?;

        emb.field("New Role Emoji", Mention::from(emoji_id), true);
        emb.footer(|f| f);

        msg.edit(ctx, |m| {
            m.set_embed(emb.clone());
            m.components(|c| {
                c.add_action_row(components::role_priority_select_action_row());
                c.add_action_row(components::confirm_abort_action_row(false));
                c
            })
        })
        .await?;

        let mut interaction = msg
            .await_component_interactions(ctx)
            .author_id(author.id)
            .filter(|f| f.kind == InteractionType::MessageComponent)
            .timeout(DEFAULT_TIMEOUT)
            .await;

        let mut role_priority: Option<components::SelectionRolePriority> = None;

        loop {
            match interaction.next().await {
                None => {
                    msg.edit(ctx, |m| {
                        emb.footer(|f| f.text("Timed out"));
                        m.set_embed(emb);
                        m.components(|c| c)
                    })
                    .await?;
                    return Err(ConversationError::TimedOut).log_reply(&msg);
                }
                Some(i) => {
                    if i.data.custom_id.eq(components::SelectionRolePriority::select_menu_id_str()) {
                        // we only expect one value from select menu
                        let val = i.data.values.get(0).ok_or_else(|| LogError::new_silent("No value selected on priority selection"))?;
                        role_priority = Some(val.parse::<components::SelectionRolePriority>()?);
                            i.create_interaction_response(ctx, |r| {
                                r.kind(InteractionResponseType::DeferredUpdateMessage)
                            }).await?;
                        continue
                    }
                    // now check buttons
                    match components::resolve_button_response(&i) {
                        components::ButtonResponse::Confirm => {
                            i.create_interaction_response(ctx, |r| {
                                r.kind(InteractionResponseType::UpdateMessage);
                                r.interaction_response_data(|d| {
                                    emb.footer(|f| f.text("Confirmed"));
                                    d.create_embed(|e| {
                                        e.0 = emb.0;
                                        e
                                    });
                                    d.components(|c| c)
                                })
                            })
                            .await?;
                            break;
                        }
                        components::ButtonResponse::Abort => {
                            i.create_interaction_response(ctx, |r| {
                                r.kind(InteractionResponseType::UpdateMessage);
                                r.interaction_response_data(|d| {
                                    emb.footer(|f| f.text("Aborted"));
                                    d.create_embed(|e| {
                                        e.0 = emb.0;
                                        e
                                    });
                                    d.components(|c| c)
                                })
                            })
                            .await?;
                            return Err(ConversationError::Canceled).log_reply(&msg);
                        }
                        _ => {
                            i.create_interaction_response(ctx, |r| {
                                r.kind(InteractionResponseType::UpdateMessage);
                                r.interaction_response_data(|d| {
                                    emb.footer(|f| f.text("Error"));
                                    d.create_embed(|e| {
                                        e.0 = emb.0;
                                        e
                                    });
                                    d.components(|c| c)
                                })
                            })
                            .await?;
                            return Err(ConversationError::InvalidInput).log_reply(&msg);
                        }
                    }
                },
            }
        }

        db::Role::insert(
            ctx,
            role_name.clone(),
            role_repr.clone(),
            *emoji_id.as_u64(),
            role_priority.map(|p| p.to_i16()),
        )
        .await?;

        msg.reply(
            ctx,
            format!(
                "Role added {} {} ({})",
                Mention::from(emoji_id),
                &role_name,
                &role_repr
            ),
        )
        .await?;
        Ok(())
    })
    .await
}

#[command]
#[aliases("rm")]
#[checks(squadmaker_role)]
#[description = "Remove (deactivate) a role by providing the short role identifier"]
#[example = "pdps"]
#[usage = "identifier"]
#[only_in("guild")]
#[num_args(1)]
pub async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let role_repr = args.single::<String>()?;
        let role = db::Role::by_repr(ctx, role_repr).await.log_reply(msg)?;
        role.deactivate(ctx).await.log_unexpected_reply(msg)?;
        msg.react(ctx, ReactionType::from(CHECK_EMOJI)).await?;
        Ok(())
    })
    .await
}

#[command]
#[checks(squadmaker_role)]
#[aliases("ls")]
#[description = "Lists all currently available roles"]
#[usage = ""]
#[only_in("guild")]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, mut _args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let roles = db::Role::all_active(ctx).await.log_unexpected_reply(msg)?;

        if roles.is_empty() {
            return LogError::new("No active roles set up", msg).into();
        }

        let mut embed = CreateEmbed::default();
        embed.xstyle();
        embeds::embed_add_roles(&mut embed, &roles, false, true);

        msg.channel_id
            .send_message(ctx, |m| m.set_embed(embed))
            .await?;

        Ok(())
    })
    .await
}
