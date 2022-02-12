use std::{
    fmt::{Display, Error as FmtError},
    str::FromStr,
    time::Duration, sync::Arc, cmp::Reverse,
};

use anyhow::{anyhow, bail, Context as ErrContext, Error, Result};
use itertools::Itertools;
use serenity::{
    builder::{CreateButton, CreateEmbed, CreateSelectMenu},
    client::Context,
    model::{
        channel::{Message, ReactionType},
        id::{EmojiId, RoleId},
        interactions::{
            message_component::{ButtonStyle, MessageComponentInteraction},
            InteractionResponseType,
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
    logging::{self, LogTrace, ReplyHelper}, signup_board::title_sort_value,
};

enum Buttons {
    Join,
    Leave,
    EditRoles,
    EditPreferences,
    AddComment,
    BackToSelection,
}

impl Display for Buttons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), FmtError> {
        match self {
            Self::Join => write!(f, "Sign Up"),
            Self::Leave => write!(f, "Sign Out"),
            Self::EditRoles => write!(f, "Edit Roles"),
            Self::EditPreferences => write!(f, "Edit Boss Preferences (soon TM)"),
            Self::AddComment => write!(f, "Add/Edit a Comment"),
            Self::BackToSelection => write!(f, "Back to Selection"),
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
            "overview_st_back_to_selection" => Ok(Self::BackToSelection),
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
            Self::BackToSelection => "overview_st_back_to_selection",
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
            Self::BackToSelection => b.style(ButtonStyle::Secondary).emoji(ReactionType::Unicode("⬅️".to_string())),
        };

        b
    }
}

pub(crate) async fn interaction(
    ctx: &Context,
    mut mci: Arc<MessageComponentInteraction>,
    trace: LogTrace,
) -> Result<()> {
    trace.step("Preparing interaction");
    // Not super elegent but gives as a message to work with right away
    mci.create_quick_info(ctx, "Loading...", true).await?;
    let mut msg = mci.get_interaction_response(ctx).await?;

    let guild_id = {
        ctx.data
            .read()
            .await
            .get::<data::ConfigValuesData>()
            .unwrap()
            .clone()
            .main_guild_id
    };

    loop {
        trace.step("Refreshing information");
        let db_user = match db::User::by_discord_id(ctx, mci.user.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                return Err(diesel::NotFound)
                    .context("Not yet registered. Please register first")
                    .map_err_reply(|_| mci.edit_original_interaction_response(ctx, |r| {
                        r.add_embed(embeds::register_instructions_embed())
                    }))
                    .await
            }
            Err(e) => bail!(e),
        };
        let trainings_all = db::Training::all_active(ctx).await?;
        let mut trainings: Vec<db::Training> = Vec::with_capacity(trainings_all.len());

        for training in trainings_all {

            // filter for trainings user can join
            let tier = training.get_tier(ctx).await.transpose()?;
            let tier_roles = match &tier {
                Some(t) => Some(t.get_discord_roles(ctx).await?),
                None => None,
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
                        if b { cj = true; break; }
                    }
                }
                cj
            } else {
                true
            };

            if !can_join { continue }

            // Add training to selection options
            trainings.push(training);
        };

        if trainings.is_empty() {
            trace.step("No training's available");
            mci.edit_quick_info(ctx, "There currently are no training options available for you =(").await?;
            return Ok(());
        }

        // Sort trainings -> splitted trainings will also be sorted
        trainings.sort_by_key(|b| Reverse(title_sort_value(b)));
        trainings.sort_by_key(|t| t.date);

        let signups = db_user.active_signups(ctx).await?;
        let mut joined: Vec<&db::Training> = Vec::with_capacity(trainings.len());
        let mut not_joined: Vec<&db::Training> = Vec::with_capacity(trainings.len());

        for t in &trainings {
            if signups.iter().map(|s| s.training_id).contains(&t.id) {
                joined.push(t);
            } else {
                // only show trainings still open for not yet joined
                if t.state == db::TrainingState::Open {
                    not_joined.push(t);
                }
            }
        }

        // Build embed
        let mut emb = CreateEmbed::xdefault();
        emb.title("Manage your Sign-Ups");
        emb.description("**Feel free to dismiss this message once your are done**");

        let mut joined_str = String::new();
        for (d, v) in joined
            .iter()
            .group_by(|t| t.date.date())
            .into_iter()
        {
            joined_str.push_str(&format!("```\n{}\n\n", d.format("%A, %v")));
            for t in v {
                    if t.state == db::TrainingState::Open {
                        joined_str.push_str(&format!("> {}\n", &t.title));
                    } else {
                        joined_str.push_str(&format!("> {} 🔒\n", &t.title));
                    }
            }
            joined_str.push_str("```");
        }
        if !joined_str.is_empty() {
            emb.field("**✅ Already signed up for**", joined_str, false);
        };

        // now filter out non open training's to not offer them in the select menu
        joined = joined.into_iter().filter(|t| t.state == db::TrainingState::Open).collect();

        let mut not_joined_str = String::new();
        for (d, v) in not_joined
            .iter()
            .group_by(|t| t.date.date())
            .into_iter()
        {
            not_joined_str.push_str(&format!("```\n{}\n\n", d.format("%A, %v")));
            for t in v {
                    not_joined_str.push_str(&format!("> {}\n", &t.title));
            }
            not_joined_str.push_str("```");
        }
        if !not_joined_str.is_empty() {
            emb.field("**❌ Not yet signed up for**", not_joined_str, false);
        };

        emb.field("🤔 How to",
            "```To sign up, sign out or to edit your sign-up simply select the training from the select menu below\n\n\
            📝 => Sign out or edit your existing sign-up\n\
            🟢 => Sign up for this training\n```",
            false);

        let mut select_menu = CreateSelectMenu::default();
        select_menu.custom_id("_user_training_select");
        select_menu.placeholder("Select a training to continue");
        select_menu.options(|opts| {
            for t in &joined {
                opts.create_option(|o| {
                    o.label(format!("| {} {}", t.date.date().format("%A"), t.title));
                    o.emoji(ReactionType::from('📝'));
                    o.value(t.id);
                    o
                });
            }
            for t in &not_joined {
                opts.create_option(|o| {
                    o.label(format!("| {} {}", t.date.date().format("%A"), t.title));
                    o.emoji(ReactionType::from('🟢'));
                    o.value(t.id);
                    o
                });
            }
            opts
        });

        mci.edit_original_interaction_response(ctx, |r| {
            r.add_embed(emb);
            r.components(|c| c.create_action_row(|ar| ar.add_select_menu(select_menu)))
        })
        .await?;

        mci = msg.await_component_interaction(ctx)
            .timeout(Duration::from_secs(60 * 3))
            .await
            .context(logging::InfoError::TimedOut)
            .map_err_reply(|what| mci.edit_quick_info(ctx, what))
            .await?;

        mci.create_interaction_response(ctx, |r| {
            r.kind(InteractionResponseType::UpdateMessage);
            r.interaction_response_data(|d| {
                d.add_embed(CreateEmbed::info_box("Loading ..."));
                d.components(|c| c)
            })
        }).await?;

        let selected_id = mci
            .data
            .values
            .get(0)
            .context("Unexpected missing value on training select menu. Aborted")
            .map_err_reply(|what| mci.edit_quick_error(ctx, what))
            .await?
            .parse::<i32>()
            .context("Unexpected value found on interaction. Aborted")
            .map_err_reply(|what| mci.edit_quick_error(ctx, what))
            .await?;

        let selected = trainings
            .iter()
            .find(|t| t.id == selected_id)
            .context("Unexpected mismatch of selected training and available ones. Aborted")
            .map_err_reply(|what| mci.edit_quick_error(ctx, what))
            .await?;

        if joined.iter().any(|t| t.id == selected.id) {
            let signup = signups
                .into_iter()
                .find(|s| s.training_id == selected.id)
                .context("Unexpected missing signup. Aborted")
                .map_err_reply(|what| mci.edit_quick_info(ctx, what))
                .await?;
            mci = edit(ctx, mci.clone(), &mut msg, selected, signup, trace.clone()).await?;

        } else if not_joined.iter().any(|t| t.id == selected.id) {
            // Technically shouldn't be required to check here
            join(ctx, mci.clone(), &mut msg, &db_user, selected, trace.clone()).await?;

        } else {
        }

        mci.edit_quick_info(ctx, "Loading ...").await?;
    }
}

// The Interactions already were responded to. So always edit
// The returned interaction will also be responded to already
async fn edit(
    ctx: &Context,
    mut mci: Arc<MessageComponentInteraction>,
    msg: &mut Message,
    training: &db::Training,
    mut signup: db::Signup,
    trace: LogTrace,
) -> Result<Arc<MessageComponentInteraction>> {

    trace.step("Signup edit");
    let bosses = training.all_training_bosses(ctx).await?;
    let roles = training.all_roles(ctx).await?;

    // Current selected roles by user
    let mut curr_roles: Vec<_> = signup
        .get_roles(ctx)
        .await?
        .into_iter()
        .map(|r| r.id)
        .collect();

    // TODO load already selected preferred bosses once we support it

    let mut base_emb = CreateEmbed::xdefault();
    base_emb.title(&training.title);
    let (a, b, c) = embeds::field_training_date(training);
    base_emb.field(a, b, c);
    base_emb.description("✅ You are signed up\n**Feel free to dismiss this message**");

    loop {
        let mut emb = base_emb.clone();
        if let Some(comment) = &signup.comment {
            emb.field("Comment", &comment, false);
        }
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
                c.create_action_row(|ar| {
                    ar.add_button(Buttons::Leave.button());
                    ar.add_button(Buttons::BackToSelection.button())
                })
            })
        })
        .await?;

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(60 * 5)) => {
                logging::InfoError::TimedOut
                    .err()
                    .map_err_reply(|what| mci.edit_quick_info(ctx, what))
                    .await?;
                //return Ok(mci);
            },
            reaction = msg.await_component_interaction(ctx) => {
                // No timeout set on collector so fine to unwrap
                mci = reaction.unwrap();
                mci.defer(ctx).await?;

                match Buttons::from_str(&mci.data.custom_id)? {
                    Buttons::Leave => {
                        signup
                            .remove(ctx)
                            .await
                            .context("Something went wrong while removing your signup =(")
                            .map_err_reply(|what| mci.edit_quick_error(ctx, what))
                            .await?;
                        return Ok(mci);
                    },
                    Buttons::EditRoles => {
                        let pre_sel: Vec<&db::Role> = roles
                            .iter()
                            .filter(|r| curr_roles.contains(&r.id))
                            .collect();

                        trace.step("Edit roles");
                        let mut selector = UpdatAbleMessage::ComponentInteraction(&mci, msg);
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
                                let err = anyhow!(logging::InfoError::AbortedOrTimedOut);
                                mci.edit_quick_info(ctx, err.to_string()).await?;
                                return Err(err);
                            },
                            Some(s) => s.into_iter().collect::<Vec<_>>(),
                        };

                        signup.clear_roles(ctx).await?;
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
                                let err = anyhow!(e)
                                    .context("I was unable to DM you. Please make sure that I can send you direct Messages");
                                mci.edit_quick_error(ctx, err.to_string()).await?;
                                return Err(err);
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

                                let err = anyhow!(logging::InfoError::TimedOut);
                                mci.edit_quick_error(ctx, err.to_string()).await?;
                                return Err(err);
                            },
                        }.unwrap();

                        signup = signup.update_comment(ctx, Some(reply.content.clone()))
                            .await
                            .context("Unexpected error updating your comment =(")
                            .map_err_reply(|what| dm.edit(ctx, |m| m.set_embed(CreateEmbed::error_box(what))))
                            .await?;

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
                    Buttons::BackToSelection => {
                        trace.step("Back");
                        return Ok(mci);
                    }
                    _ => bail!("Unexpected interaction"),
                }
            }
        }
    }
}

// The Interactions already were responded to. So always edit
// The returned interaction will also be responded to already
async fn join(
    ctx: &Context,
    mci: Arc<MessageComponentInteraction>,
    msg: &mut Message,
    db_user: &db::User,
    training: &db::Training,
    trace: LogTrace,
) -> Result<Arc<MessageComponentInteraction>> {

    trace.step("New Signup");
    let roles = training.all_roles(ctx).await?;
    let mut selector = UpdatAbleMessage::ComponentInteraction(&mci, msg);
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
            return Ok(mci);
        }
        Some(s) => s.into_iter().collect::<Vec<_>>(),
    };

    let signup = db::Signup::insert(ctx, db_user, training)
        .await
        .context("Failed to create signup")
        .map_err_reply(|what| mci.edit_quick_error(ctx, what))
        .await?;

    for r in selected {
        signup
            .add_role(ctx, r)
            .await
            .with_context(|| format!("Failed to add role: {}", r.title))
            .map_err_reply(|what| mci.edit_quick_error(ctx, what))
            .await?;
    }

    let mci = edit(ctx, mci, msg, training, signup, trace).await?;
    Ok(mci)
}
