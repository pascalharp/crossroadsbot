use serenity::prelude::*;
use serenity::model::prelude::*;
use serenity::framework::standard::{
    Args,
    CommandResult,
    CommandOptions,
    Reason,
    macros::{command, check},
};
use serenity::futures::prelude::*;
use super::{
    Conversation,
    ConfigValuesData,
    CHECK_EMOJI,
    CROSS_EMOJI,
    DEFAULT_TIMEOUT,
};
use crate::db;

// --- Manager Guild Check ---
#[check]
#[name = "manager_guild"]
async fn manager_guild_check(ctx: &Context, msg: &Message, _: &mut Args, _: &CommandOptions) -> Result<(), Reason> {

    let msg_guild_id = match msg.guild_id {
        None => {
            return Err(Reason::Log("Manager command outside of manager guild".to_string()));
        },
        Some(g) => g,
    };

    let manager_guild_id = {
        ctx.data.read().await.get::<ConfigValuesData>().unwrap().manager_guild_id
    };

    if msg_guild_id != manager_guild_id {
        return Err(Reason::Log("Manager command outside of manager guild".to_string()));
    }

    Ok(())
}

#[command]
#[checks(manager_guild)]
pub async fn add_role(ctx: &Context, msg: &Message, mut _args: Args) -> CommandResult {

    let mut role_name = String::new();
    let mut role_repr = String::new();

    let conv = Conversation::start(ctx, &msg.author).await?;
    // Ask for Role name
    conv.chan.say(ctx, format!("{}\n{}",
            "Please enter the full name of the Role",
            "Example: Power DPS"
            )).await?;

    // Get role name
    if let Some(reply) = conv.await_reply(ctx).await {
        role_name.push_str(&reply.content);
        reply.react(ctx, ReactionType::from(CHECK_EMOJI)).await?;
    } else {
        conv.timeout_msg(ctx).await?;
        return Ok(());
    }

    // Ask for repr
    conv.chan.say(ctx, format!("{}\n{}",
            "Please enter the short representation for the role (no spaces allowed)",
            "Example: pdps"
            )).await?;

    // Get repr
    let mut replies = conv.await_replies(ctx).await;
    loop {
        if let Some(reply) = replies.next().await {
            if reply.content.contains(" ") {
                conv.chan.say(ctx, "I said no spaces!!!!\nTry again:").await?;
            } else {
                role_repr.push_str(&reply.content);
                reply.react(ctx, CHECK_EMOJI).await?;
                break;
            }
        } else {
            conv.timeout_msg(ctx).await?;
            return Ok(());
        }
    }

    let mut msg = conv.chan.say(ctx, "Loading available emojis....").await?;

    // load all roles from db
    let roles = db::get_roles(&db::connect())?;
    let db_emojis: Vec<EmojiId> = roles.iter()
        .map(|r| {
            EmojiId::from(r.emoji as u64)
        })
        .collect();

    // load all roles from discord guild
    let gid = ctx.data.read().await
        .get::<ConfigValuesData>()
        .unwrap()
        .manager_guild_id;
    let emoji_guild = Guild::get(ctx, gid).await?;

    // Remove already used emojis
    let available: Vec<Emoji> = emoji_guild.emojis.values()
        .cloned()
        .filter(|e| {
            !db_emojis.contains(&e.id)
        })
        .collect();

    if available.is_empty() {
        conv.abort(ctx, Some("No more emojis for roles available")).await?;
        return Ok(());
    }

    // Present all available emojis
    for e in available.clone() {
        msg.react(ctx, ReactionType::from(e)).await?;
    }

    // Ask for emoji to represent role
    msg.edit(ctx, |m| {
        m.content("Please react to this message with the emoji to represent this role (has to be a guild emoji)")
    }).await?;

    // Wait for emoji
    let emoji = msg.await_reaction(ctx)
        .timeout(DEFAULT_TIMEOUT)
        .filter(move |r| {
            match r.emoji {
                ReactionType::Custom {animated:_, id, name:_} => {
                    available.iter().map( |e| {
                        e.id
                    }).collect::<Vec<EmojiId>>()
                    .contains(&id)
                },
                _ => false,
            }
        }).await;

    let emoji_id = match emoji {
        None => {
            conv.timeout_msg(ctx).await?;
            return Ok(());
        },
        Some(r) => {
            match r.as_inner_ref().emoji {
                ReactionType::Custom {animated:_, id, name:_} => id,
                _ => return Ok(()), // Should never occur since filtered already
            }
        }
    };

    let msg = conv.chan.send_message(ctx, |m| {
        m.embed(|e| {
            e.title("Summary");
            e.field("Full Role Name", &role_name, false);
            e.field("Representing name", &role_repr, false);
            e.field("Role Emoji", &emoji_id, false);
            e.footer(|f| {
                f.text(format!("React with {} to add the role to the database or with {} to abort",
                               CHECK_EMOJI,
                               CROSS_EMOJI,
                               ))
            });
            e
        });
        m
    }).await?;

    msg.react(ctx, CHECK_EMOJI).await?;
    msg.react(ctx, CROSS_EMOJI).await?;

    let react = msg.await_reaction(ctx).filter( |r| {
        r.emoji == ReactionType::from(CHECK_EMOJI) || r.emoji == ReactionType::from(CROSS_EMOJI)
    }).timeout(DEFAULT_TIMEOUT).await;

    if let Some(e) = react {
        if e.as_inner_ref().emoji == ReactionType::from(CHECK_EMOJI) {
            // Save to database
            let res = {
                let db_conn = db::connect();
                db::add_role(&db_conn, &role_name, &role_repr, *emoji_id.as_u64())
            };
            match res {
                Ok(_) => {
                    conv.chan.say(ctx, "Role added to database").await?;
                }
                Err(e) => {
                    conv.chan.say(ctx, format!("Error adding role to database:\n{}", e)).await?;
                }
            }
        }
    } else {
        conv.timeout_msg(ctx).await?;
        return Ok(());
    }

    Ok(())
}

#[command]
#[checks(manager_guild)]
pub async fn rm_role(ctx: &Context, msg: &Message, mut _args: Args) -> CommandResult {
    Ok(())
}
