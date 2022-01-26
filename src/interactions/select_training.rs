use anyhow::{bail, Context as ErrContext, Result};
use serenity::{
    builder::CreateEmbed,
    client::Context,
    model::{
        id::EmojiId,
        interactions::{
            message_component::MessageComponentInteraction,
            InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
        },
        misc::Mention,
    },
};
use serenity_tools::{builder::CreateEmbedExt, interactions::MessageComponentInteractionExt};

use crate::{
    db, embeds,
    logging::{LogTrace, ReplyHelper},
};

pub(crate) async fn interaction(
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

    trace.step("Loading selected training");
    let training_id: i32 = mci.data.values.get(0).unwrap().parse()?;
    let training =
        match db::Training::by_id_and_state(ctx, training_id, db::TrainingState::Open).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                return Err(diesel::NotFound)
                    .context("The selected training is not open")
                    .map_err_reply(|what| mci.create_quick_info(ctx, what, true))
                    .await
            }
            Err(e) => bail!(e),
        };

    // Most important stuff handled. Ack interaction at this point
    mci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
            d.add_embed(CreateEmbed::info_box("Loading..."))
        })
    })
    .await?;

    trace.step("Looking for signup");
    match db::Signup::by_user_and_training(ctx, &db_user, &training).await {
        Ok(s) => signed_up(ctx, mci, db_user, training, s, trace).await,
        Err(diesel::NotFound) => not_signed_up(ctx, mci, db_user, training, trace).await,
        Err(e) => bail!(e),
    }
}

async fn not_signed_up(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    db_user: db::User,
    training: db::Training,
    trace: LogTrace,
) -> Result<()> {
    trace.step("No signup found");

    let bosses = training.all_training_bosses(ctx).await?;
    let roles = training.all_roles(ctx).await?;

    let mut emb = embeds::training_base_embed(&training);
    emb.fields_chunked_fmt(
        &bosses,
        |b| {
            let boss_link = match &b.url {
                Some(l) => format!("[{}]({})", b.name, l),
                None => b.name.to_string(),
            };
            format!(
                "{} | {}",
                Mention::from(EmojiId::from(b.emoji as u64)),
                boss_link
            )
        },
        "Boss Pool",
        false,
        20,
    );
    emb.fields_chunked_fmt(
        &roles,
        |r| {
            format!(
                "{} | {}",
                Mention::from(EmojiId::from(r.emoji as u64)),
                r.title
            )
        },
        "Available Roles",
        false,
        20,
    );

    mci.edit_original_interaction_response(ctx, |d| d.add_embed(emb))
        .await?;

    Ok(())
}

async fn signed_up(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    db_user: db::User,
    training: db::Training,
    signup: db::Signup,
    trace: LogTrace,
) -> Result<()> {
    trace.step("Signup found");

    let bosses = training.all_training_bosses(ctx).await?;
    let roles = training.all_roles(ctx).await?;
    let curr_roles: Vec<_> = signup
        .get_roles(ctx)
        .await?
        .into_iter()
        .map(|r| r.id)
        .collect();

    let mut emb = embeds::training_base_embed(&training);
    emb.fields_chunked_fmt(
        &bosses,
        |b| {
            let boss_link = match &b.url {
                Some(l) => format!("[{}]({})", b.name, l),
                None => b.name.to_string(),
            };
            // TODO highlight already preferred bosses
            format!(
                "{} | {}",
                Mention::from(EmojiId::from(b.emoji as u64)),
                boss_link
            )
        },
        "Boss Pool",
        false,
        20,
    );
    emb.fields_chunked_fmt(
        &roles,
        |r| {
            if curr_roles.contains(&r.id) {
                format!(
                    "{} | __**{}**__",
                    Mention::from(EmojiId::from(r.emoji as u64)),
                    r.title
                )
            } else {
                format!(
                    "{} | {}",
                    Mention::from(EmojiId::from(r.emoji as u64)),
                    r.title
                )
            }
        },
        "Available Roles",
        false,
        20,
    );
    emb.description("You are already up");

    mci.edit_original_interaction_response(ctx, |d| d.add_embed(emb))
        .await?;

    Ok(())
}
