use std::{
    fmt::{Display, Error as FmtError},
    str::FromStr,
    time::Duration,
};

use anyhow::{anyhow, bail, Context as ErrContext, Error, Result};
use serenity::{
    builder::{CreateButton, CreateEmbed},
    client::Context,
    model::{
        channel::{Message, ReactionType},
        id::EmojiId,
        interactions::{
            message_component::{ButtonStyle, MessageComponentInteraction},
            InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
        },
        misc::Mention,
    },
};
use serenity_tools::{
    builder::CreateEmbedExt,
    collectors::{PagedSelectorConfig, UpdatAbleMessage},
    interactions::MessageComponentInteractionExt,
};

use crate::{
    db,
    embeds::{self, CrossroadsEmbeds},
    logging::{LogTrace, ReplyHelper},
};

enum Buttons {
    Join,
    Leave,
    EditRoles,
    EditPreferences,
}

impl Display for Buttons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), FmtError> {
        match self {
            Self::Join => write!(f, "Join"),
            Self::Leave => write!(f, "Leave"),
            Self::EditRoles => write!(f, "Edit Roles"),
            Self::EditPreferences => write!(f, "Edit Boss Preferences (soon TM)"),
        }
    }
}

impl FromStr for Buttons {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "overview_st_join" => Ok(Self::Join),
            "overview_st_leave" => Ok(Self::Leave),
            "overview_st_edit_roles" => Ok(Self::EditRoles),
            "overview_st_edit_preferences" => Ok(Self::EditPreferences),
            _ => Err(anyhow!("Unknown interaction: {}", s)),
        }
    }
}

impl Buttons {
    fn custom_id(&self) -> &'static str {
        match self {
            Self::Join => "overview_st_join",
            Self::Leave => "overview_st_leave",
            Self::EditRoles => "overview_st_edit_roles",
            Self::EditPreferences => "overview_st_edit_preferences",
        }
    }

    fn button(&self) -> CreateButton {
        let mut b = CreateButton::default();
        b.label(self);
        b.custom_id(self.custom_id());

        match self {
            Self::Join => b.style(ButtonStyle::Success),
            Self::Leave => b.style(ButtonStyle::Danger),
            Self::EditRoles => b.style(ButtonStyle::Primary),
            Self::EditPreferences => b.style(ButtonStyle::Primary).disabled(true),
        };

        b
    }
}

pub(crate) async fn interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    trace: LogTrace,
) -> Result<()> {
    if mci.data.values.get(0).unwrap() == "clear" {
        trace.step("Clear");
        mci.defer(ctx).await?;
        return Ok(());
    }

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

    let mut emb = CreateEmbed::xdefault();
    emb.title(&training.title);
    let (a, b, c) = embeds::field_training_date(&training);
    emb.field(a, b, c);
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

    mci.edit_original_interaction_response(ctx, |d| {
        d.add_embed(emb);
        d.components(|c| c.create_action_row(|ar| ar.add_button(Buttons::Join.button())))
    })
    .await?;

    trace.step("Waiting for interaction");

    let msg = mci.get_interaction_response(ctx).await?;
    match msg
        .await_component_interaction(ctx)
        .timeout(Duration::from_secs(60 * 5))
        .await
    {
        Some(r) => {
            r.defer(ctx).await?;
            match r.data.custom_id.parse::<Buttons>()? {
                Buttons::Join => {
                    trace.step("Join selected");
                    join_fresh(ctx, mci, msg, db_user, training, roles, trace).await?;
                }
                _ => bail!("Unexpected button"),
            }
        }
        None => {
            Err(anyhow!("Timed out"))
                .map_err_reply(|what| mci.edit_quick_info(ctx, what))
                .await?;
        }
    }

    Ok(())
}

async fn join_fresh(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    mut msg: Message,
    db_user: db::User,
    training: db::Training,
    roles: Vec<db::Role>,
    trace: LogTrace,
) -> Result<()> {
    let mut selector = UpdatAbleMessage::ComponentInteraction(mci, &mut msg);
    let mut selector_conf = PagedSelectorConfig::default();
    selector_conf.set_items_per_row(4).set_rows_per_page(3);

    let mut emb = CreateEmbed::xdefault();
    emb.title("Select your role(s)");
    emb.field(
        training.title,
        format!("<t:{}>", training.date.timestamp()),
        false,
    );
    selector_conf.set_base_embed(emb);

    trace.step("Role selection");
    let selected = selector
        .paged_selector(ctx, selector_conf, &roles, |r| {
            (
                ReactionType::from(EmojiId::from(r.emoji as u64)),
                r.title.to_string(),
            )
        })
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

    let mut emb = CreateEmbed::xdefault();
    emb.title(&training.title);
    let (a, b, c) = embeds::field_training_date(&training);
    emb.field(a, b, c);
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
        true,
        10,
    );
    emb.description("You are already signed up");

    mci.edit_original_interaction_response(ctx, |d| {
        d.add_embed(emb);
        d.components(|c| {
            c.create_action_row(|ar| {
                ar.add_button(Buttons::EditRoles.button());
                ar.add_button(Buttons::EditPreferences.button())
            });
            c.create_action_row(|ar| ar.add_button(Buttons::Leave.button()))
        })
    })
    .await?;

    Ok(())
}
