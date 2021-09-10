use crate::{components::*, conversation::*, data::*, db, embeds::*, log::*};

use serenity::{
    builder::CreateEmbed,
    futures::{future, StreamExt},
    model::{
        channel::Message,
        id::RoleId,
        interactions::{
            message_component::MessageComponentInteraction,
            InteractionApplicationCommandCallbackDataFlags as CallbackDataFlags,
            InteractionResponseType, InteractionType,
        },
        misc::Mention,
        user::User,
    },
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
    sync::Arc,
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60 * 3);
pub const CHECK_EMOJI: char = '✅';
pub const CROSS_EMOJI: char = '❌';
pub const X_EMOJI: char = '✖';
pub const ENVELOP_EMOJI: char = '✉';
pub const WARNING_EMOJI: char = '⚠';
pub const DIZZY_EMOJI: char = '😵';
pub const RUNNING_EMOJI: char = '🏃';
pub const GREEN_CIRCLE_EMOJI: char = '🟢';
pub const RED_CIRCLE_EMOJI: char = '🔴';
pub const CONSTRUCTION_SITE_EMOJI: char = '🚧';
pub const MEMO_EMOJI: char = '📝';
pub const GREEN_SQUARE_EMOJI: char = '🟩';
pub const RED_SQUARE_EMOJI: char = '🟥';
pub const ALARM_CLOCK_EMOJI: char = '⏰';
pub const RIGHT_ARROW_EMOJI: char = '➡';
pub const LEFT_ARROW_EMOJI: char = '⬅';

async fn join_button_interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    tid: i32,
    db_user: &db::User,
) -> LogResult<()> {
    let in_pub = in_public_channel(ctx, mci).await;
    let training = match db::Training::by_id_and_state(ctx, tid, db::TrainingState::Open).await {
        Ok(t) => t,
        Err(diesel::NotFound) => {
            return mci
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        if in_pub {
                            d.flags(CallbackDataFlags::EPHEMERAL);
                        }
                        d.content(Mention::from(&mci.user));
                        d.content(format!(
                            "{} This training is not open for sign up right now",
                            Mention::from(&mci.user)
                        ));
                        d
                    })
                })
                .await
                .log_only();
        }
        Err(e) => return Err(e).log_only(),
    };

    // check that there is no signup yet
    match db::Signup::by_user_and_training(ctx, db_user, &training).await {
        Err(diesel::NotFound) => (),
        Ok(_) => {
            return mci
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        if in_pub {
                            d.flags(CallbackDataFlags::EPHEMERAL);
                        }
                        d.content(Mention::from(&mci.user));
                        d.add_embed(already_signed_up_embed(&training));
                        d.components(|c| c.add_action_row(edit_leave_action_row(training.id)));
                        d
                    })
                })
                .await
                .log_only();
        }
        Err(e) => return Err(e).log_only(),
    };

    // verify if tier requirements pass
    match verify_tier(ctx, &training, &mci.user).await {
        Ok((pass, tier)) => {
            if !pass {
                return mci
                    .create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::ChannelMessageWithSource);
                        r.interaction_response_data(|d| {
                            if in_pub {
                                d.flags(CallbackDataFlags::EPHEMERAL);
                            }
                            d.content(format!(
                                "{} Tier requirement not passed! Required tier: {}",
                                Mention::from(&mci.user),
                                tier
                            ));
                            d
                        })
                    })
                    .await
                    .log_only();
            }
        }
        Err(e) => return Err(e).log_only(),
    };

    let mut conv = match Conversation::init(ctx, &mci.user, training_base_embed(&training)).await {
        Ok(conv) => {
            if in_pub {
                // Give user hint
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        d.flags(CallbackDataFlags::EPHEMERAL);
                        d.content(format!(
                            "{} Check [DM's]({}) {}",
                            Mention::from(&mci.user),
                            conv.msg.link(),
                            ENVELOP_EMOJI
                        ));
                        d
                    })
                })
                .await
                .ok();
            } else
            // Just confirm the button interaction
            {
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::DeferredUpdateMessage)
                })
                .await
                .ok();
            }
            conv
        }
        Err(e) => {
            mci.create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource);
                r.interaction_response_data(|d| {
                    if in_pub {
                        d.flags(CallbackDataFlags::EPHEMERAL);
                    }
                    d.content(format!("{} {}", Mention::from(&mci.user), e.to_string()));
                    d
                })
            })
            .await
            .ok();
            return Err(e).log_only();
        }
    };

    let roles = training
        .active_roles(ctx)
        .await
        .log_unexpected_reply(&conv.msg)?;
    let roles_lookup: HashMap<String, &db::Role> =
        roles.iter().map(|r| (String::from(&r.repr), r)).collect();

    // Gather selected roles
    let selected: HashSet<String> = HashSet::with_capacity(roles.len());
    let selected = select_roles(ctx, &mut conv.msg, &conv.user, &roles, selected)
        .await
        .log_reply(&conv.msg)?;

    let signup = db::Signup::insert(ctx, db_user, &training)
        .await
        .log_unexpected_reply(&conv.msg)?;

    // Save roles
    // We inserted all roles into the HashMap, so it is save to unwrap
    let futs = selected
        .iter()
        .map(|r| signup.add_role(ctx, roles_lookup.get(r).unwrap()));
    future::try_join_all(futs).await?;

    conv.msg
        .edit(ctx, |m| {
            m.add_embed(|e| {
                e.0 = success_signed_up(&training).0;
                e
            });
            m.components(|c| c.add_action_row(edit_leave_action_row(training.id)));
            m
        })
        .await?;

    Ok(())
}

pub async fn edit_button_interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    tid: i32,
    db_user: &db::User,
) -> LogResult<()> {
    let in_pub = in_public_channel(ctx, mci).await;
    let training = match db::Training::by_id_and_state(ctx, tid, db::TrainingState::Open).await {
        Ok(t) => t,
        Err(diesel::NotFound) => {
            return mci
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        if in_pub {
                            d.flags(CallbackDataFlags::EPHEMERAL);
                        }
                        d.content(Mention::from(&mci.user));
                        d.content(format!(
                            "{} This training is not open for sign up right now",
                            Mention::from(&mci.user)
                        ));
                        d
                    })
                })
                .await
                .log_only();
        }
        Err(e) => return Err(e).log_only(),
    };

    // check that there is a signup already
    let signup = match db::Signup::by_user_and_training(ctx, db_user, &training).await {
        Err(diesel::NotFound) => {
            return mci
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        if in_pub {
                            d.flags(CallbackDataFlags::EPHEMERAL);
                        }
                        d.content(Mention::from(&mci.user));
                        d.add_embed(not_signed_up_embed(&training));
                        d.components(|c| c.add_action_row(join_action_row(training.id)));
                        d
                    })
                })
                .await
                .log_only();
        }
        Ok(o) => o,
        Err(e) => return Err(e).log_only(),
    };

    let mut conv = match Conversation::init(ctx, &mci.user, training_base_embed(&training)).await {
        Ok(conv) => {
            if in_pub {
                // Give user hint
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        d.flags(CallbackDataFlags::EPHEMERAL);
                        d.content(format!(
                            "{} Check [DM's]({}) {}",
                            Mention::from(&mci.user),
                            conv.msg.link(),
                            ENVELOP_EMOJI
                        ));
                        d
                    })
                })
                .await
                .ok();
            } else {
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::DeferredUpdateMessage)
                })
                .await
                .ok();
            }
            conv
        }
        Err(e) => {
            mci.create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource);
                r.interaction_response_data(|d| {
                    if in_pub {
                        d.flags(CallbackDataFlags::EPHEMERAL);
                    }
                    d.content(format!("{} {}", Mention::from(&mci.user), e.to_string()));
                    d
                })
            })
            .await
            .ok();
            return Err(e).log_only();
        }
    };

    let roles = training
        .all_roles(ctx)
        .await
        .log_unexpected_reply(&conv.msg)?;
    let roles_lookup: HashMap<String, &db::Role> =
        roles.iter().map(|r| (String::from(&r.repr), r)).collect();

    // Get new roles from user
    let mut selected: HashSet<String> = HashSet::with_capacity(roles.len());
    let already_selected = signup.get_roles(ctx).await?;
    for r in already_selected {
        selected.insert(r.repr);
    }
    let selected = select_roles(ctx, &mut conv.msg, &conv.user, &roles, selected)
        .await
        .log_reply(&conv.msg)?;

    // Save new roles
    signup
        .clear_roles(ctx)
        .await
        .log_unexpected_reply(&conv.msg)?;
    let futs = selected.iter().filter_map(|r| {
        roles_lookup
            .get(r).map(|r| signup.add_role(ctx, *r))
    });
    future::try_join_all(futs).await?;

    conv.msg
        .edit(ctx, |m| {
            m.add_embed(|e| {
                e.0 = success_signed_up(&training).0;
                e
            });
            m.components(|c| c.add_action_row(edit_leave_action_row(training.id)));
            m
        })
        .await?;

    Ok(())
}

pub async fn leave_button_interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    tid: i32,
    db_user: &db::User,
) -> LogResult<()> {
    let in_pub = in_public_channel(ctx, mci).await;
    let training = match db::Training::by_id_and_state(ctx, tid, db::TrainingState::Open).await {
        Ok(t) => t,
        Err(diesel::NotFound) => {
            return mci
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        if in_pub {
                            d.flags(CallbackDataFlags::EPHEMERAL);
                        }
                        d.content(Mention::from(&mci.user));
                        d.content(format!(
                            "{} This training is not open right now",
                            Mention::from(&mci.user)
                        ));
                        d
                    })
                })
                .await
                .log_only();
        }
        Err(e) => return Err(e).log_only(),
    };

    // check that there is a signup already
    let signup = match db::Signup::by_user_and_training(ctx, db_user, &training).await {
        Err(diesel::NotFound) => {
            return mci
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        if in_pub {
                            d.flags(CallbackDataFlags::EPHEMERAL);
                        }
                        d.content(Mention::from(&mci.user));
                        d.add_embed(not_signed_up_embed(&training));
                        d.components(|c| c.add_action_row(join_action_row(training.id)));
                        d
                    })
                })
                .await
                .log_only();
        }
        Ok(o) => o,
        Err(e) => return Err(e).log_only(),
    };

    signup.remove(ctx).await.log_only()?;
    mci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            if in_pub {
                d.flags(CallbackDataFlags::EPHEMERAL);
            }
            d.content(Mention::from(&mci.user));
            d.add_embed(signed_out_embed(&training));
            d.components(|c| c.add_action_row(join_action_row(training.id)));
            d
        })
    })
    .await
    .log_only()?;
    Ok(())
}

pub async fn button_interaction(ctx: &Context, mci: &MessageComponentInteraction) {
    // Check first if it is an interaction to handle

    let bti = match mci.data.custom_id.parse::<ButtonTrainingInteraction>() {
        Err(_) => return,
        Ok(a) => a,
    };

    let in_pub = in_public_channel(ctx, mci).await;

    log_interaction(ctx, mci, &bti, || async {
        // Check if user is registerd
        let db_user = match db::User::by_discord_id(ctx, mci.user.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                return mci
                    .create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::ChannelMessageWithSource);
                        r.interaction_response_data(|d| {
                            if in_pub {
                                d.flags(CallbackDataFlags::EPHEMERAL);
                            }
                            d.content(Mention::from(&mci.user));
                            d.add_embed(not_registered_embed())
                        })
                    })
                    .await
                    .log_only();
            }
            Err(e) => {
                return Err(LogError::from(e));
            }
        };

        match bti {
            ButtonTrainingInteraction::Join(id) => {
                join_button_interaction(ctx, mci, id, &db_user).await?
            }
            ButtonTrainingInteraction::Edit(id) => {
                edit_button_interaction(ctx, mci, id, &db_user).await?
            }
            ButtonTrainingInteraction::Leave(id) => {
                leave_button_interaction(ctx, mci, id, &db_user).await?
            }
        }

        Ok(())
    })
    .await
    .ok();
}

/// Verifies if the discord user has the required tier for a training
pub async fn verify_tier(
    ctx: &Context,
    training: &db::Training,
    user: &User,
) -> Result<(bool, String)> {
    let tier = training.get_tier(ctx).await;
    let tier = match tier {
        None => return Ok((true, "none".to_string())),
        Some(t) => Arc::new(t?),
    };
    let tier_mappings = tier.get_discord_roles(ctx).await?;
    let roles_set = {
        let guild = ctx
            .data
            .read()
            .await
            .get::<ConfigValuesData>()
            .unwrap()
            .main_guild_id;
        let roles = guild.member(ctx, user.id).await?.roles;
        HashSet::<RoleId>::from_iter(roles)
    };

    let passed = tier_mappings
        .iter()
        .any(|t| roles_set.contains(&RoleId::from(t.discord_role_id as u64)));
    Ok((passed, tier.name.clone()))
}

async fn in_public_channel(ctx: &Context, mci: &MessageComponentInteraction) -> bool {
    mci.channel_id
        .to_channel(ctx)
        .await
        .map_or(false, |c| c.private().is_none())
}

/// Looks at the user permissions and filters out trainings the user has not sufficient permissions
/// for
pub async fn filter_trainings(
    ctx: &Context,
    trainings: Vec<db::Training>,
    user: &User,
) -> Result<Vec<db::Training>> {
    let roles = {
        let guild = ctx
            .data
            .read()
            .await
            .get::<ConfigValuesData>()
            .unwrap()
            .main_guild_id;
        guild.member(ctx, user.id).await?.roles
    };

    let tiers = db::Tier::all(ctx).await?;

    let mut tier_map: HashMap<i32, Vec<db::TierMapping>> = HashMap::new();

    for t in tiers {
        let t = Arc::new(t);
        let discord_roles = t.get_discord_roles(ctx).await?;
        tier_map.insert(t.id, discord_roles);
    }

    Ok(trainings
        .into_iter()
        .filter(|tier| match tier.tier_id {
            None => true,
            Some(id) => match tier_map.get(&id) {
                None => false,
                Some(tm_vec) => tm_vec
                    .iter()
                    .any(|tm| roles.iter().any(|r| *r == (tm.discord_role_id as u64))),
            },
        })
        .collect())
}

// Using Deferred since updating the message and Interaction Response
// doesnt update the original message
pub async fn clear_components(
    ctx: &Context,
    interaction: &MessageComponentInteraction,
    msg: &mut Message,
) -> Result<()> {
    interaction
        .create_interaction_response(ctx, |r| {
            r.kind(InteractionResponseType::DeferredUpdateMessage)
        })
        .await?;

    msg.edit(ctx, |m| m.components(|c| c)).await?;

    Ok(())
}

pub async fn await_confirm_abort_interaction(ctx: &Context, msg: &mut Message) -> LogResult<()> {
    let interaction = msg
        .await_component_interaction(ctx)
        .timeout(DEFAULT_TIMEOUT)
        .await;
    match interaction {
        None => return Err(ConversationError::TimedOut).log_reply(msg),
        Some(i) => match resolve_button_response(&i) {
            ButtonResponse::Confirm => {
                clear_components(ctx, &i, msg).await.log_only()?;
            }
            ButtonResponse::Abort => {
                clear_components(ctx, &i, msg).await.log_only()?;
                return Err(ConversationError::Canceled).log_reply(msg);
            }
            _ => {
                clear_components(ctx, &i, msg).await.log_only()?;
                return Err(ConversationError::InvalidInput).log_reply(msg);
            }
        },
    }
    Ok(())
}

pub async fn select_roles(
    ctx: &Context,
    msg: &mut Message,
    // The user who can select
    user: &User,
    // All roles
    roles: &Vec<db::Role>,
    // HashShet with unique reprs of roles
    mut selected: HashSet<String>,
) -> Result<HashSet<String>> {
    let role_pages = role_action_row(roles);
    if role_pages.is_empty() {
        return Err("No roles provided".into());
    }
    let mut role_page_curr: usize = 0;

    let orig_embeds = msg
        .embeds
        .clone()
        .into_iter()
        .map(CreateEmbed::from)
        .collect::<Vec<_>>();
    msg.edit(ctx, |m| {
        m.add_embed(|e| {
            e.0 = select_roles_embed(roles, &selected).0;
            e.footer(|f| {
                if selected.is_empty() {
                    f.text(format!(
                        "Page {}/{}\n{} Select at least one role",
                        role_page_curr + 1,
                        role_pages.len(),
                        WARNING_EMOJI
                    ))
                } else {
                    f.text(format!("Page {}/{}", role_page_curr + 1, role_pages.len()))
                }
            });
            e
        });
        m.components(|c| {
            c.set_action_rows(role_pages.get(role_page_curr).unwrap().to_vec());
            let mut ca_ar = confirm_abort_action_row();
            if role_pages.len() > 1 {
                if role_page_curr > 0 {
                    ca_ar.add_button(prev_button());
                }
                if role_page_curr < role_pages.len() - 1 {
                    ca_ar.add_button(next_button());
                }
            }
            c.add_action_row(ca_ar);
            c
        });
        m
    })
    .await?;

    let mut interactions = msg
        .await_component_interactions(ctx)
        .author_id(user.id)
        .filter(|f| f.kind == InteractionType::MessageComponent)
        .timeout(DEFAULT_TIMEOUT)
        .await;

    loop {
        let i = interactions.next().await;
        match i {
            None => {
                msg.edit(ctx, |m| {
                    m.set_embeds(orig_embeds.clone());
                    m.add_embed(|e| {
                        e.0 = select_roles_embed(roles, &selected).0;
                        e.footer(|f| {
                            f.text(format!("Role selection timed out {}", ALARM_CLOCK_EMOJI))
                        })
                    });
                    m.components(|c| c)
                })
                .await?;
                return Err(Box::new(ConversationError::TimedOut));
            }
            Some(i) => match resolve_button_response(&i) {
                ButtonResponse::Confirm => {
                    // only accept if at least one role selectec
                    if !selected.is_empty() {
                        i.create_interaction_response(ctx, |r| {
                            r.kind(InteractionResponseType::DeferredUpdateMessage)
                        })
                        .await?;
                        // Edit message with final selection
                        msg.edit(ctx, |m| {
                            m.set_embeds(orig_embeds);
                            m.add_embed(|e| {
                                e.0 = select_roles_embed(roles, &selected).0;
                                e
                            });
                            m.components(|c| c)
                        })
                        .await?;
                        break;
                    }
                }
                ButtonResponse::Abort => {
                    i.create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::DeferredUpdateMessage)
                    })
                    .await?;
                    // Edit message with final selection
                    msg.edit(ctx, |m| {
                        m.set_embeds(orig_embeds);
                        m.add_embed(|e| {
                            e.0 = select_roles_embed(roles, &selected).0;
                            e
                        });
                        m.components(|c| c)
                    })
                    .await?;
                    return Err(Box::new(ConversationError::Canceled));
                }
                ButtonResponse::Next => {
                    if role_page_curr < (role_pages.len() - 1) {
                        role_page_curr += 1;
                    }
                    i.create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::UpdateMessage);
                        r.interaction_response_data(|d| {
                            d.embeds(orig_embeds.clone());
                            d.create_embed(|e| {
                                e.0 = select_roles_embed(roles, &selected).0;
                                e.footer(|f| {
                                    if selected.is_empty() {
                                        f.text(format!(
                                            "Page {}/{}\n{} Select at least one role",
                                            role_page_curr + 1,
                                            role_pages.len(),
                                            WARNING_EMOJI
                                        ))
                                    } else {
                                        f.text(format!(
                                            "Page {}/{}",
                                            role_page_curr + 1,
                                            role_pages.len()
                                        ))
                                    }
                                });
                                e
                            });
                            d.components(|c| {
                                c.set_action_rows(role_pages.get(role_page_curr).unwrap().to_vec());
                                let mut ca_ar = confirm_abort_action_row();
                                if role_pages.len() > 1 {
                                    if role_page_curr > 0 {
                                        ca_ar.add_button(prev_button());
                                    }
                                    if role_page_curr < role_pages.len() - 1 {
                                        ca_ar.add_button(next_button());
                                    }
                                }
                                c.add_action_row(ca_ar);
                                c
                            })
                        })
                    })
                    .await?;
                }
                ButtonResponse::Prev => {
                    if role_page_curr > 0 {
                        role_page_curr -= 1;
                    }
                    i.create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::UpdateMessage);
                        r.interaction_response_data(|d| {
                            d.embeds(orig_embeds.clone());
                            d.create_embed(|e| {
                                e.0 = select_roles_embed(roles, &selected).0;
                                e.footer(|f| {
                                    if selected.is_empty() {
                                        f.text(format!(
                                            "Page {}/{}\n{} Select at least one role",
                                            role_page_curr + 1,
                                            role_pages.len(),
                                            WARNING_EMOJI
                                        ))
                                    } else {
                                        f.text(format!(
                                            "Page {}/{}",
                                            role_page_curr + 1,
                                            role_pages.len()
                                        ))
                                    }
                                });
                                e
                            });
                            d.components(|c| {
                                c.set_action_rows(role_pages.get(role_page_curr).unwrap().to_vec());
                                let mut ca_ar = confirm_abort_action_row();
                                if role_pages.len() > 1 {
                                    if role_page_curr > 0 {
                                        ca_ar.add_button(prev_button());
                                    }
                                    if role_page_curr < role_pages.len() - 1 {
                                        ca_ar.add_button(next_button());
                                    }
                                }
                                c.add_action_row(ca_ar);
                                c
                            })
                        })
                    })
                    .await?;
                }
                ButtonResponse::Other(repr) => {
                    if selected.contains(&repr) {
                        selected.remove(&repr);
                    } else {
                        selected.insert(repr);
                    }
                    i.create_interaction_response(ctx, |r| {
                        r.kind(InteractionResponseType::UpdateMessage);
                        r.interaction_response_data(|d| {
                            d.embeds(orig_embeds.clone());
                            d.create_embed(|e| {
                                e.0 = select_roles_embed(roles, &selected).0;
                                e.footer(|f| {
                                    if selected.is_empty() {
                                        f.text(format!(
                                            "Page {}/{}\n{} Select at least one role",
                                            role_page_curr + 1,
                                            role_pages.len(),
                                            WARNING_EMOJI
                                        ))
                                    } else {
                                        f.text(format!(
                                            "Page {}/{}",
                                            role_page_curr + 1,
                                            role_pages.len()
                                        ))
                                    }
                                });
                                e
                            })
                        })
                    })
                    .await?;
                }
            },
        }
    }

    Ok(selected)
}
