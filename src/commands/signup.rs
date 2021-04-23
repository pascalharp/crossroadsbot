use serenity::prelude::*;
use serenity::model::prelude::*;
use serenity::framework::standard::{
    Args,
    CommandResult,
    macros::command,
};
use super::Conversation;
use tokio::time::{sleep, Duration};

#[command]
pub async fn register(ctx: &Context, msg: &Message, args: Args) -> CommandResult {

    // TODO change later. Currently for testing
    if let Ok(conv) = Conversation::start(ctx, &msg.author).await {
        conv.chan.say(&ctx.http, "Uh we are getting private now ;)").await;
        conv.chan.say(&ctx.http, "I will sleep now").await;
        sleep(Duration::from_secs(60)).await;
        conv.chan.say(&ctx.http, "I am done sleeping. Goodbye =)").await;
    } else {
        msg.channel_id.say(&ctx.http, "We already have a conversation").await;
    }

    Ok(())
}
