use super::{ConfigValuesData, ADMIN_ROLE_CHECK, CHECK_EMOJI, CROSS_EMOJI, DEFAULT_TIMEOUT};
use crate::db;
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
#[checks(admin_role)]
#[description = "Add a role by providing a full role name and a role short identifier (without spaces)"]
#[example = "\"Power DPS\" pdps"]
#[usage = "full_name identifier"]
#[only_in("guild")]
#[num_args(2)]
pub async fn add(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let author = &msg.author;

    let role_name = args.single_quoted::<String>()?;
    let role_repr = args.single_quoted::<String>()?;

    if role_repr.contains(" ") {
        msg.reply(ctx, "Identifier must not contain spaces").await?;
        return Ok(());
    }

    // load all roles from db
    let roles = db::get_roles(&db::connect())?;
    let db_emojis: Vec<EmojiId> = roles
        .iter()
        .map(|r| EmojiId::from(r.emoji as u64))
        .collect();

    // load all roles from discord guild
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
        msg.reply(ctx, "No more emojis for roles available").await?;
        return Ok(());
    }

    let mut msg = msg
        .channel_id
        .send_message(ctx, |m| {
            m.embed(|e| {
                e.description("New Role");
                e.field("Full role name", &role_name, true);
                e.field("Short role emoji identifier", &role_repr, true);
                e.footer(|f| {
                    f.text(format!(
                        "Choose an emoji for the role. {} to abort",
                        CROSS_EMOJI
                    ))
                });
                e
            })
        })
        .await?;

    msg.react(ctx, CROSS_EMOJI).await?;
    // Present all available emojis
    for e in available.clone() {
        msg.react(ctx, e).await?;
    }

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
            msg.reply(ctx, "Timed out").await?;
            return Ok(());
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
                        msg.reply(ctx, "Aborted").await?;
                        return Ok(());
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

    msg.edit(ctx, |m| {
        m.embed(|e| {
            e.description("New Role");
            e.field("Full role name", &role_name, true);
            e.field("Short role emoji identifier", &role_repr, true);
            e.field("Role Emoji", Mention::from(emoji_id), true);
            e.footer(|f| {
                f.text(format!(
                    "{} to finish. {} to abort",
                    CHECK_EMOJI, CROSS_EMOJI
                ))
            });
            e
        })
    })
    .await?;

    msg.react(ctx, CHECK_EMOJI).await?;
    msg.react(ctx, CROSS_EMOJI).await?;

    let react = msg
        .await_reaction(ctx)
        .author_id(author.id)
        .filter(|r| {
            r.emoji == ReactionType::from(CHECK_EMOJI) || r.emoji == ReactionType::from(CROSS_EMOJI)
        })
        .timeout(DEFAULT_TIMEOUT)
        .await;

    if let Some(e) = react {
        if e.as_inner_ref().emoji == ReactionType::from(CHECK_EMOJI) {
            // Save to database
            let res = {
                let db_conn = db::connect();
                db::add_role(&db_conn, &role_name, &role_repr, *emoji_id.as_u64())
            };
            match res {
                Ok(_) => {
                    msg.reply(ctx, "Role added to database").await?;
                }
                Err(e) => {
                    msg.reply(ctx, format!("Error adding role to database:\n{}", e))
                        .await?;
                }
            }
        } else if e.as_inner_ref().emoji == ReactionType::from(CROSS_EMOJI) {
            msg.reply(ctx, "Aborted").await?;
            return Ok(());
        }
    } else {
        msg.reply(ctx, "Timed out").await?;
        return Ok(());
    }

    Ok(())
}

#[command]
#[aliases("rm")]
#[checks(admin_role)]
#[description = "Remove (deactivate) a role by providing the short role identifier"]
#[example = "pdps"]
#[usage = "identifier"]
#[only_in("guild")]
#[num_args(1)]
pub async fn remove(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let role_repr = args.single::<String>()?;
    let role = db::get_role_by_repr(&db::connect(), &role_repr);
    let role = match role {
        Ok(r) => r,
        Err(e) => match e {
            diesel::result::Error::NotFound => {
                msg.reply(ctx, format!("Role not found: {}", &role_repr))
                    .await?;
                return Ok(());
            }
            _ => return Err(e.into()),
        },
    };

    role.deactivate(&db::connect())?;
    msg.react(ctx, CHECK_EMOJI).await?;
    Ok(())
}

#[command]
#[aliases("ls")]
#[description = "Lists all currently available roles"]
#[usage = ""]
#[only_in("guild")]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, mut _args: Args) -> CommandResult {
    let roles = db::get_roles(&db::connect())?;

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

    Ok(())
}
