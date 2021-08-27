use super::SQUADMAKER_ROLE_CHECK;
use crate::{
    components,
    data::ConfigValuesData,
    db,
    log::*,
    utils::{CROSS_EMOJI, DEFAULT_TIMEOUT},
};
use serenity::builder::CreateEmbed;
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::prelude::*;
use serenity::prelude::*;

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
    LogResult::command(ctx, msg, || async {
        let author = &msg.author;
        let role_name = args.single_quoted::<String>()?;
        let role_repr = args.single_quoted::<String>()?;

        if role_repr.contains(" ") {
            return Err("Identifier must not contain spaces".into());
        }

        // load all active roles from db
        let roles = db::Role::all_active(ctx).await?;
        // check if repr already used
        if roles.iter().any(|r| r.repr.eq(&role_repr)) {
            return Err(
                "A role with the same repr already exists. The repr has to be unique".into(),
            );
        }

        let db_emojis: Vec<EmojiId> = roles
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
        let emoji_guild = Guild::get(ctx, gid).await?;

        // Remove already used emojis
        let available: Vec<Emoji> = emoji_guild
            .emojis
            .values()
            .cloned()
            .filter(|e| !db_emojis.contains(&e.id))
            .collect();

        if available.is_empty() {
            return Ok(Some("No more emojis for roles available".into()));
        }

        let mut emb = CreateEmbed::default();
        emb.description("New Role");
        emb.field("Full role name", &role_name, true);
        emb.field("Short role identifier", &role_repr, true);
        emb.footer(|f| f.text(format!("Loading emojis, please wait....")));

        let mut msg = msg
            .channel_id
            .send_message(ctx, |m| m.set_embed(emb.clone()))
            .await?;

        // Present all available emojis
        // Not using buttons here, since there is a limited amount for them
        // and there might be a lot of emojis
        for e in available.clone() {
            msg.react(ctx, e).await?;
        }
        msg.react(ctx, CROSS_EMOJI).await?;

        emb.footer(|f| {
            f.text(format!(
                "Choose an emoji for the role. {} to abort",
                CROSS_EMOJI
            ))
        });

        msg.edit(ctx, |m| m.set_embed(emb.clone())).await?;

        // Wait for emoji
        let emoji = msg
            .await_reaction(ctx)
            .timeout(DEFAULT_TIMEOUT)
            .author_id(author.id)
            .filter(move |r| {
                if r.emoji == ReactionType::from(CROSS_EMOJI) {
                    return true;
                }
                match r.emoji {
                    ReactionType::Custom {
                        animated: _,
                        id,
                        name: _,
                    } => available
                        .iter()
                        .map(|e| e.id)
                        .collect::<Vec<EmojiId>>()
                        .contains(&id),
                    _ => false,
                }
            })
            .await;

        let emoji_id = match emoji {
            None => {
                return Ok(Some("Timed out".into()));
            }
            Some(r) => {
                match &r.as_inner_ref().emoji {
                    ReactionType::Custom {
                        animated: _,
                        id,
                        name: _,
                    } => *id,
                    ReactionType::Unicode(s) => {
                        if *s == String::from(CROSS_EMOJI) {
                            return Ok(Some("Aborted".into()));
                        }
                        // Should never occur since filtered already filtered
                        return Err("Unexpected emoji".into());
                    }
                    // Should never occur since filtered already filtered
                    _ => return Err("Unexpected emoji".into()),
                }
            }
        };

        msg.delete_reactions(ctx).await?;

        emb.field("Role Emoji", Mention::from(emoji_id), true);
        emb.footer(|f| f);

        msg.edit(ctx, |m| {
            m.set_embed(emb.clone());
            m.components(|c| c.add_action_row(components::confirm_abort_action_row()))
        })
        .await?;

        let interaction = msg
            .await_component_interaction(ctx)
            .author_id(author.id)
            .filter(|f| f.kind == InteractionType::MessageComponent)
            .timeout(DEFAULT_TIMEOUT)
            .await;

        match interaction {
            None => {
                msg.edit(ctx, |m| {
                    emb.footer(|f| f.text("Timed out"));
                    m.set_embed(emb);
                    m.components(|c| c)
                })
                .await?;
                return Err("Timed out".into());
            }
            Some(i) => match components::resolve_button_response(&i) {
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
                    return Err("Aborted".into());
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
                    return Err("Unexpected interaction".into());
                }
            },
        }

        db::Role::insert(ctx, role_name.clone(), role_repr, *emoji_id.as_u64()).await?;

        Ok(format!("Role added {} {}", Mention::from(emoji_id), role_name).into())
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
    let role_repr = args.single::<String>()?;
    let role = match db::Role::by_repr(ctx, role_repr).await {
        Ok(r) => r,
        Err(e) => match e {
            diesel::result::Error::NotFound => {
                let res: LogResult = Ok(Some("Role not found".into()));
                res.reply(ctx, msg).await?;
                res.log(ctx, LogType::Command(&msg.content), &msg.author)
                    .await;
                return Ok(());
            }
            _ => return Err(e.into()),
        },
    };

    role.deactivate(ctx).await?;

    let res: LogResult = Ok(Some("Role removed".into()));
    res.reply(ctx, msg).await?;
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    Ok(())
}

#[command]
#[checks(squadmaker_role)]
#[aliases("ls")]
#[description = "Lists all currently available roles"]
#[usage = ""]
#[only_in("guild")]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, mut _args: Args) -> CommandResult {
    let roles = db::Role::all_active(ctx).await?;

    msg.channel_id
        .send_message(ctx, |m| {
            m.embed(|e| {
                e.title("Roles");
                for r in roles {
                    e.field(
                        format!(
                            "{} {}",
                            Mention::from(EmojiId::from(r.emoji as u64)),
                            r.repr
                        ),
                        r.title,
                        true,
                    );
                }
                e
            })
        })
        .await?;

    let res: LogResult = Ok(Some("Success".into()));
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    Ok(())
}
