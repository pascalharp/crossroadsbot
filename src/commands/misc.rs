use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::interactions::message_component::ButtonStyle;
use serenity::model::prelude::*;
use serenity::prelude::*;

use crate::utils::{CHECK_EMOJI, CONSTRUCTION_SITE_EMOJI, CROSS_EMOJI};

#[group]
#[commands(ping, dudu, button)]
struct Misc;

#[command]
pub async fn ping(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    msg.channel_id.say(&ctx.http, "pong").await?;
    Ok(())
}

#[command]
pub async fn dudu(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    msg.channel_id.say(&ctx.http, "BONK").await?;
    Ok(())
}

#[command]
pub async fn button(ctx: &Context, msg: &Message, _args: Args) -> CommandResult {
    let msg = msg
        .channel_id
        .send_message(ctx, |m| {
            m.content("Uhhhh look. Fancy buttons =D");
            m.embed(|e| {
                e.description("A description");
                e.field("A field", "A value", false)
            });
            m.components(|c| {
                c.create_action_row(|a| {
                    a.create_button(|b| {
                        b.style(ButtonStyle::Primary);
                        b.custom_id("ok");
                        b.label("OK");
                        b.emoji(ReactionType::from(CHECK_EMOJI));
                        b
                    });
                    a.create_button(|b| {
                        b.style(ButtonStyle::Secondary);
                        b.custom_id("abort");
                        b.label("Abort");
                        b.emoji(ReactionType::from(CROSS_EMOJI));
                        b
                    })
                });
                c.create_action_row(|a| {
                    a.create_button(|b| {
                        b.style(ButtonStyle::Danger);
                        b.custom_id("busy");
                        b.label("Busy");
                        b.emoji(ReactionType::from(CONSTRUCTION_SITE_EMOJI));
                        b
                    })
                })
            })
        })
        .await?;
    let i = msg.await_component_interaction(ctx).await;
    println!("{:?}", i);
    msg.reply(ctx, format!("{:?}", i.unwrap().data)).await?;
    Ok(())
}
