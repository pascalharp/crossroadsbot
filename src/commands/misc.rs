use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::model::interactions::InteractionResponseType;
use serenity::model::prelude::*;
use serenity::prelude::*;

use crate::components::*;
use crate::utils::{DEFAULT_TIMEOUT, ALARM_CLOCK_EMOJI};

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
    let mut msg = msg
        .channel_id
        .send_message(ctx, |m| {
            m.content("Uhhhh look. Fancy buttons =D");
            m.components(|c| {
                c.add_action_row(confirm_abort_action_row());
                c
            })
        })
        .await?;

    let i = msg
        .await_component_interaction(ctx)
        .timeout(DEFAULT_TIMEOUT)
        .await;

    match i {
        None => {
            msg.edit(ctx, |m| {
                m.content(&format!("Timed out {}", ALARM_CLOCK_EMOJI));
                m.components( |c| c);
                m
            }).await?;
        },
        Some(i) => {
            i.create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::UpdateMessage);
                r.interaction_response_data(|d| {
                    let reply = match i.data.custom_id.as_str() {
                        COMPONENT_ID_CONFIRM => COMPONENT_LABEL_CONFIRM,
                        COMPONENT_ID_ABORT => COMPONENT_LABEL_ABORT,
                        _ => "Unknown",
                    };
                    d.content(format!("You clicked: {}", reply));
                    d.components(|c| c)
                })
            })
            .await?;
        }
    }
    Ok(())
}
