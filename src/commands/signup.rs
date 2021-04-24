use serenity::prelude::*;
use serenity::model::prelude::*;
use serenity::framework::standard::{
    Args,
    CommandResult,
    macros::command,
};
use super::Conversation;
use tokio::time::{sleep, Duration};
use crate::db;

#[command]
pub async fn register(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {

    // if the account name was provided immediately no nee to start a conversation
    if let Ok(acc_name) = args.single::<String>() {
        let conn = db::connect();
        db::add_user(&conn, 1234, &acc_name);
    }

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
