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
        id::{EmojiId, RoleId},
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
    data, db,
    embeds::{self, CrossroadsEmbeds},
    logging::{self, LogTrace, ReplyHelper},
};

enum Buttons {
    Join,
    Leave,
    EditRoles,
    EditPreferences,
    AddComment,
    Dismiss,
}

impl Display for Buttons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), FmtError> {
        match self {
            Self::Join => write!(f, "Sign up"),
            Self::Leave => write!(f, "Remove Signup"),
            Self::EditRoles => write!(f, "Edit Roles"),
            Self::EditPreferences => write!(f, "Edit Boss Preferences (soon TM)"),
            Self::AddComment => write!(f, "Add a comment"),
            Self::Dismiss => write!(f, "Dismiss"),
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
            "overview_st_add_comment" => Ok(Self::AddComment),
            "overview_st_dismiss" => Ok(Self::Dismiss),
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
            Self::AddComment => "overview_st_add_comment",
            Self::Dismiss => "overview_st_dismiss",
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
            Self::AddComment => b.style(ButtonStyle::Primary),
            Self::Dismiss => b.style(ButtonStyle::Secondary),
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
    let tier = training.get_tier(ctx).await.transpose()?;
    let tier_roles = match &tier {
        Some(t) => Some(t.get_discord_roles(ctx).await?),
        None => None,
    };

    let guild_id = {
        ctx.data
            .read()
            .await
            .get::<data::ConfigValuesData>()
            .unwrap()
            .clone()
            .main_guild_id
            .clone()
    };

    // check if user can join the training
    let can_join = if let Some(tier_roles) = tier_roles {
        let mut cj = false;
        for tr in tier_roles {
            if let Ok(b) = mci
                .user
                .has_role(ctx, guild_id, RoleId::from(tr.discord_role_id as u64))
                .await
            {
                if b {
                    cj = true;
                    break;
                }
            }
        }
        cj
    } else {
        true
    };

    let mut emb = CreateEmbed::xdefault();
    emb.title(&training.title);
    if can_join {
        emb.description("You are not yet signed up");
    } else {
        emb.description("You do not have the required tier to join");
    }
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

    let mut join_button = Buttons::Join.button();
    if !can_join {
        join_button.disabled(true);
    }

    mci.edit_original_interaction_response(ctx, |d| {
        d.add_embed(emb);
        d.components(|c| c.create_action_row(|ar| ar.add_button(join_button)))
    })
    .await?;

    if !can_join {
        trace.step("No options available");
        return Ok(());
    }

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
                    join_fresh(ctx, mci, msg, db_user, training, roles, bosses, trace).await?;
                }
                _ => bail!("Unexpected button"),
            }
        }
        None => {
            logging::InfoError::TimedOut
                .err()
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
    bosses: Vec<db::TrainingBoss>,
    trace: LogTrace,
) -> Result<()> {
    let mut selector = UpdatAbleMessage::ComponentInteraction(mci, &mut msg);
    let mut selector_conf = PagedSelectorConfig::default();
    selector_conf
        .items_per_row(4)
        .rows_per_page(3)
        .min_select(1);

    let mut emb = CreateEmbed::xdefault();
    emb.title("Select your role(s)");
    emb.field(
        &training.title,
        format!("<t:{}>", training.date.timestamp()),
        false,
    );
    selector_conf.base_embed(emb);

    trace.step("Role selection");
    let selected = selector
        .paged_selector(ctx, selector_conf, &roles, |r| {
            (
                ReactionType::from(EmojiId::from(r.emoji as u64)),
                r.title.to_string(),
            )
        })
        .await?;

    let selected = match selected {
        None => {
            logging::InfoError::AbortedOrTimedOut
                .err()
                .map_err_reply(|what| mci.edit_quick_info(ctx, what))
                .await?;
            return Ok(());
        }
        Some(s) => s.into_iter().collect::<Vec<_>>(),
    };

    trace.step("Creating signup");
    let signup = db::Signup::insert(ctx, &db_user, &training)
        .await
        .context("Failed to create signup")
        .map_err_reply(|what| mci.edit_quick_error(ctx, what))
        .await?;

    trace.step("Saving selected roles");
    for r in selected {
        signup
            .add_role(ctx, r)
            .await
            .with_context(|| format!("Failed to add role: {}", r.title))
            .map_err_reply(|what| mci.edit_quick_error(ctx, what))
            .await?;
    }
    trace.step("Signup completed");
    edit_signup(ctx, mci, db_user, signup, training, roles, bosses, trace).await?;
    Ok(())
}

async fn edit_signup(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    _db_user: db::User,
    signup: db::Signup,
    training: db::Training,
    roles: Vec<db::Role>,
    bosses: Vec<db::TrainingBoss>,
    trace: LogTrace,
) -> Result<()> {
    trace.step("Signup edit offer");

    // Current selected roles by user
    let mut curr_roles: Vec<_> = signup
        .get_roles(ctx)
        .await?
        .into_iter()
        .map(|r| r.id)
        .collect();

    // TODO load already selected preferred bosses

    let mut base_emb = CreateEmbed::xdefault();
    base_emb.title(&training.title);
    let (a, b, c) = embeds::field_training_date(&training);
    base_emb.field(a, b, c);
    base_emb.description("You are signed up");

    let mut msg = mci.get_interaction_response(ctx).await?;

    loop {
        let mut emb = base_emb.clone();
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
        emb.footer(|f| f.text("Feel free to dismiss this message"));

        mci.edit_original_interaction_response(ctx, |r| {
            r.add_embed(emb);
            r.components(|c| {
                c.create_action_row(|ar| {
                    ar.add_button(Buttons::EditRoles.button());
                    ar.add_button(Buttons::EditPreferences.button());
                    ar.add_button(Buttons::AddComment.button())
                });
                c.create_action_row(|ar| ar.add_button(Buttons::Leave.button()))
            })
        })
        .await?;

        trace.step("Waiting for interaction");
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(60 * 5)) => {
                logging::InfoError::TimedOut
                    .err()
                    .map_err_reply(|what| mci.edit_quick_info(ctx, what))
                    .await?;
                return Ok(());
            },
            reaction = msg.await_component_interaction(ctx) => {
                // No timeout set on collector so fine to unwrap
                let reaction = reaction.unwrap();
                reaction.defer(ctx).await?;

                match Buttons::from_str(&reaction.data.custom_id)? {
                    Buttons::Leave => {
                        signup
                            .remove(ctx)
                            .await
                            .context("Something went wrong while removing your signup =(")
                            .map_err_reply(|what| mci.edit_quick_error(ctx, what))
                            .await?;
                        mci.edit_quick_info(ctx, "Signup removed").await?;
                        return Ok(());
                    },
                    Buttons::EditRoles => {
                        let pre_sel: Vec<&db::Role> = roles
                            .iter()
                            .filter_map(|r| {
                                if curr_roles.contains(&r.id) { Some(r) }
                                else { None }
                            })
                            .collect();

                        trace.step("Edit roles");
                        let mut selector = UpdatAbleMessage::ComponentInteraction(mci, &mut msg);
                        let mut selector_conf = PagedSelectorConfig::default();
                        let mut sel_emb = base_emb.clone();
                        sel_emb.description("Select new roles");
                        selector_conf
                            .base_embed(sel_emb)
                            .items_per_row(4)
                            .rows_per_page(3)
                            .min_select(1)
                            .pre_selected(&pre_sel);

                        let selected = selector
                            .paged_selector(ctx, selector_conf, &roles, |r| {
                                (
                                    ReactionType::from(EmojiId::from(r.emoji as u64)),
                                    r.title.to_string(),
                                )
                            })
                            .await?;

                        let selected = match selected {
                            None => {
                                logging::InfoError::AbortedOrTimedOut
                                    .err()
                                    .map_err_reply(|what| mci.edit_quick_info(ctx, what))
                                    .await?;
                                return Ok(());
                            },
                            Some(s) => s.into_iter().collect::<Vec<_>>(),
                        };

                        trace.step("Clear old roles");
                        signup.clear_roles(ctx).await?;

                        trace.step("Add new roles");
                        for r in &selected {
                            signup.add_role(ctx, r).await?;
                        }

                        curr_roles = selected
                            .into_iter()
                            .map(|r| r.id)
                            .collect();
                    }
                    Buttons::EditPreferences => {
                        todo!()
                    }
                    Buttons::AddComment => {
                        trace.step("Add comment");
                        let dm = mci.user.dm(ctx, |m| {
                            m.embed(|e| {
                                e.field(
                                    "Add Comment",
                                    "Please reply with your comment. (Times out after 5 min)",
                                    false)
                            })
                        }).await;

                        let mut dm = match dm {
                            Ok(dm) => {
                                mci.edit_original_interaction_response(ctx, |m| {
                                    let mut emb = base_emb.clone();
                                    emb.field(
                                        "Add comment",
                                        format!("[Waiting for your reply in DM's]({})", dm.link()),
                                        false);
                                    m.add_embed(emb);
                                    m.components(|c| c)
                                }).await?;
                                dm
                            },
                            Err(e) => {
                                Err(e)
                                    .context("I was unable to DM you. Please make sure that I can send you direct Messages")
                                    .map_err_reply(|what| mci.edit_quick_error(ctx, what))
                                    .await?;
                                return Ok(())
                            }
                        };

                        let reply = tokio::select! {
                            reply = dm.channel_id.await_reply(ctx) => {
                                reply
                            },
                            _ = tokio::time::sleep(Duration::from_secs(60 * 5)) => {
                                dm.edit(ctx, |m| {
                                    m.set_embed(CreateEmbed::info_box("Timed out"))
                                }).await?;

                                logging::InfoError::TimedOut
                                    .err()
                                    .map_err_reply(|what| mci.edit_quick_info(ctx, what))
                                    .await?;
                                return Ok(());
                            },
                        }.unwrap();

                        signup.update_comment(ctx, Some(reply.content.clone()))
                            .await
                            .context("Unexpected error updating your comment =(")
                            .map_err_reply(|what| dm.edit(ctx, |m| m.set_embed(CreateEmbed::error_box(what))))
                            .await?;

                        trace.step("comment saved");

                        reply.channel_id.send_message(ctx, |r| {
                            r.reference_message(reply.as_ref());
                            r.embed(|e| {
                                e.field(
                                    "Saved",
                                    format!("Your comment was saved. [Go back.]({})", msg.link()),
                                    true)
                            })
                        }).await?;
                    },
                    _ => bail!("Unexpected interaction"),
                }
            }
        }
    }
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

    edit_signup(ctx, mci, db_user, signup, training, roles, bosses, trace).await?;

    Ok(())
}
