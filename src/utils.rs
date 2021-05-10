use crate::commands::{CHECK_EMOJI, CROSS_EMOJI, DEFAULT_TIMEOUT};
use serenity::{
    client::bridge::gateway::ShardMessenger,
    collector::reaction_collector::{ReactionAction},
    model::{
        channel::{Message, Reaction, ReactionType},
        id::UserId,
    },
    http::CacheHttp,
    Result,
};

pub enum YesOrNo {
    Yes,
    No,
}

/// Reacts with CHECK_EMOJI and CROSS_EMOJI on the provided message
pub async fn send_yes_or_no<'a>(
    cache_http: &'a impl CacheHttp,
    msg: &'a Message
) -> Result<(Reaction, Reaction)> {

    let check = msg.react(cache_http, CHECK_EMOJI).await?;
    let cross = msg.react(cache_http, CROSS_EMOJI).await?;
    Ok((check, cross))
}

/// Awaits the CHECK_EMOJI or CROSS_EMOJI reaction on a message using the default timeout
pub async fn await_yes_or_no<'a>(
    shard_messenger: &'a impl AsRef<ShardMessenger>,
    msg: &'a Message,
    user_id: Option<UserId>,
) -> Option<YesOrNo> {
    let collector = msg
        .await_reaction(shard_messenger)
        .timeout(DEFAULT_TIMEOUT)
        .filter(|r| {
            r.emoji == ReactionType::from(CHECK_EMOJI) || r.emoji == ReactionType::from(CROSS_EMOJI)
        });

    let collector = match user_id {
        Some(u) => collector.author_id(u),
        None => collector,
    };

    match collector.await {
        None => return None,
        Some(r) => match r.as_ref() {
            ReactionAction::Added(e) => {
                if e.emoji == ReactionType::from(CHECK_EMOJI) {
                    return Some(YesOrNo::Yes);
                }
                if e.emoji == ReactionType::from(CROSS_EMOJI) {
                    return Some(YesOrNo::No);
                }
                return None;
            }
            _ => return None,
        },
    }
}
