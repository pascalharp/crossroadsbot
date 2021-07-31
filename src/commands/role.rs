use super::SQUADMAKER_ROLE_CHECK;
use crate::{
    data::ConfigValuesData,
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
#[prefix = "role"]
#[commands(add, remove, list)]
pub struct Role;

async fn _add(ctx: &Context, channel: ChannelId, author: UserId, mut args: Args) -> LogResult {
    let role_name = args.single_quoted::<String>()?;
    let role_repr = args.single_quoted::<String>()?;

    if role_repr.contains(" ") {
        return Ok("Identifier must not contain spaces".into());
    }

    // load all roles from db
    let roles = db::Role::all().await?;
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
        return Ok("No more emojis for roles available".into());
    }

    let mut msg = channel
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
        .author_id(author)
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
            return Ok("Timed out".into());
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
                        return Ok("Aborted".into());
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
                    "{} to confirm. {} to abort",
                    CHECK_EMOJI, CROSS_EMOJI
                ))
            });
            e
        })
    })
    .await?;

    utils::send_yes_or_no(ctx, &msg).await?;

    if let Some(e) = utils::await_yes_or_no(ctx, &msg, Some(author)).await {
        match e {
            utils::YesOrNo::Yes => {
                let new_role = db::NewRole {
                    title: String::from(role_name),
                    repr: String::from(role_repr),
                    emoji: *emoji_id.as_u64() as i64,
                };

                new_role.add().await?;
            }
            utils::YesOrNo::No => {
                return Ok("Aborted".into());
            }
        }
    } else {
        return Ok("Timed out".into());
    }

    Ok(format!("Role added {}", Mention::from(emoji_id)).into())
}

#[command]
#[checks(squadmaker_role)]
#[description = "Add a role by providing a full role name and a role short identifier (without spaces)"]
#[example = "\"Power DPS\" pdps"]
#[usage = "full_name identifier"]
#[only_in("guild")]
#[num_args(2)]
pub async fn add(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let res = _add(ctx, msg.channel_id, msg.author.id, args).await;
    res.reply(ctx, msg).await?;
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    res.cmd_result()
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
    let role = match db::Role::by_repr(role_repr.clone()).await {
        Ok(r) => r,
        Err(e) => match e {
            diesel::result::Error::NotFound => {
                let res: LogResult = Ok("Role not found".into());
                res.reply(ctx, msg).await?;
                res.log(ctx, LogType::Command(&msg.content), &msg.author)
                    .await;
                return res.cmd_result();
            }
            _ => return Err(e.into()),
        },
    };

    role.deactivate().await?;

    let res: LogResult = Ok("Role removed".into());
    res.reply(ctx, msg).await?;
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    res.cmd_result()
}

#[command]
#[checks(squadmaker_role)]
#[aliases("ls")]
#[description = "Lists all currently available roles"]
#[usage = ""]
#[only_in("guild")]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, mut _args: Args) -> CommandResult {
    let roles = db::Role::all().await?;

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

    let res: LogResult = Ok("Success".into());
    res.log(ctx, LogType::Command(&msg.content), &msg.author)
        .await;
    res.cmd_result()
}
