use serenity::prelude::*;
use serenity::model::prelude::*;
use serenity::framework::standard::{
    Args,
    CommandResult,
    macros::command,
};
use serenity::futures::prelude::*;
use super::{
    Conversation,
    CHECK_EMOJI,
};

#[command]
pub async fn add_role(ctx: &Context, msg: &Message, mut _args: Args) -> CommandResult {

    let mut role_name = String::new();
    let mut role_repr = String::new();

    let conv = Conversation::start(ctx, &msg.author).await?;
    // Ask for Role name
    conv.chan.send_message(&ctx.http, |m| {
        m.content("Please enter the full name of the Role");
        m
    }).await?;

    // Get role name
    if let Some(reply) = conv.await_reply(ctx).await {
        role_name.push_str(&reply.content);
        reply.react(ctx, ReactionType::from(CHECK_EMOJI)).await?;
    } else {
        conv.timeout_msg(ctx).await?;
        return Ok(());
    }

    // Ask for repr
    conv.chan.send_message(&ctx.http, |m| {
        m.content("Please enter the representing name for the role (no spaces allowed)")
    }).await?;

    // Get repr
    let mut replies = conv.await_replies(ctx).await;
    loop {
        if let Some(reply) = replies.next().await {
            if reply.content.contains(" ") {
                conv.chan.send_message(&ctx.http, |m| {
                    m.content("I said no spaces !!!!")
                }).await?;
            } else {
                role_repr.push_str(&reply.content);
                reply.react(ctx, ReactionType::from(CHECK_EMOJI)).await?;
                break;
            }
        } else {
            conv.timeout_msg(ctx).await?;
            return Ok(());
        }
    }

    Ok(())
}
