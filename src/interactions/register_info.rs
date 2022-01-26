use crate::{embeds, logging::LogTrace};
use anyhow::Result;
use serenity::{
    client::Context,
    model::interactions::{
        message_component::MessageComponentInteraction,
        InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
    },
};

pub(crate) async fn interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    trace: LogTrace,
) -> Result<()> {
    let bot = ctx.cache.current_user().await;
    trace.step("Sending register information");
    mci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
            d.add_embed(embeds::register_instructions_embed(&bot))
        });
        r
    })
    .await?;

    Ok(())
}
