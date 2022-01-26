use std::collections::HashMap;

use crate::{
    db, embeds,
    logging::{LogTrace, ReplyHelper},
};
use anyhow::{bail, Context as ErrContext, Result};
use serenity::{
    client::Context,
    model::interactions::{
        message_component::MessageComponentInteraction,
        InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
    },
};
use serenity_tools::interactions::MessageComponentInteractionExt;

pub(super) async fn interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    trace: LogTrace,
) -> Result<()> {
    trace.step("Loading user from database");
    let db_user = match db::User::by_discord_id(ctx, mci.user.id).await {
        Ok(u) => u,
        Err(diesel::NotFound) => {
            return Err(diesel::NotFound)
                .context("Not yet registered. Please register first")
                .map_err_reply(|what| mci.create_quick_info(ctx, what, true))
                .await
        }
        Err(e) => bail!(e),
    };

    trace.step("Loading sign ups for active training(s)");
    let signups = db_user.active_signups(ctx).await?;
    let mut roles: HashMap<i32, Vec<db::Role>> = HashMap::with_capacity(signups.len());
    for (s, _) in &signups {
        let signup_roles = s.clone().get_roles(ctx).await?;
        roles.insert(s.id, signup_roles);
    }

    trace.step("Replying to user with result");
    let emb = embeds::signup_list_embed(&signups, &roles);
    mci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
            d.add_embed(emb)
        })
    })
    .await?;

    Ok(())
}
