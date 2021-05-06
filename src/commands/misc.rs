use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;

#[group]
#[commands(ping, dudu)]
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
