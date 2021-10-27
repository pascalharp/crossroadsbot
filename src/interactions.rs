use crate::{components::*, conversation::*, db, embeds::*, log::*, utils::*};

use serenity::{
    collector::MessageCollectorBuilder,
    futures::{future, StreamExt},
    model::{
        interactions::{
            message_component::MessageComponentInteraction,
            InteractionApplicationCommandCallbackDataFlags as CallbackDataFlags,
            InteractionResponseType,
        },
        misc::Mention,
    },
    prelude::*,
};

use std::collections::{HashMap, HashSet};

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
            } else {
                // Just confirm the button interaction
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

async fn edit_button_interaction(
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
    let futs = selected
        .iter()
        .filter_map(|r| roles_lookup.get(r).map(|r| signup.add_role(ctx, *r)));
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

async fn leave_button_interaction(
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

async fn comment_button_interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    tid: i32,
    db_user: &db::User,
) -> LogResult<()> {
    if in_public_channel(ctx, mci).await {
        return mci
            .create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource);
                r.interaction_response_data(|d| {
                    d.flags(CallbackDataFlags::EPHEMERAL);
                    d.content("This can not be used in public channels");
                    d
                })
            })
            .await
            .log_only();
    }

    let training = match db::Training::by_id_and_state(ctx, tid, db::TrainingState::Open).await {
        Ok(t) => t,
        Err(diesel::NotFound) => {
            return mci
                .create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
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

    // Open conversation since we have to wait for input
    let conv =
        match Conversation::init(ctx, &mci.user, signup_add_comment_embed(&training)).await {
            Ok(conv) => {
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::DeferredUpdateMessage)
                })
                .await
                .ok();
                conv
            }
            Err(e) => {
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        d.content(format!("{} {}", Mention::from(&mci.user), e.to_string()));
                        d
                    })
                })
                .await
                .ok();
                return Err(e).log_only();
            }
        };

    match MessageCollectorBuilder::new(ctx)
        .channel_id(conv.chan.id)
        .author_id(conv.user.id)
        .timeout(DEFAULT_TIMEOUT)
        .collect_limit(1)
        .await
        .next()
        .await
    {
        Some(msg) => {
            signup.update_comment(ctx, Some(msg.content.clone())).await.log_unexpected_reply(&msg)?;
            msg.reply(ctx, "Comment saved").await.log_unexpected_reply(&msg)?;
        }
        None => {
            conv.msg.reply(ctx, "Timed out").await?;
            return Err(ConversationError::TimedOut.into());
        }
    }

    Ok(())
}

async fn button_training_interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    bti: &ButtonTrainingInteraction,
) -> LogResult<()> {
    let in_pub = in_public_channel(ctx, mci).await;
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
            join_button_interaction(ctx, mci, *id, &db_user).await?
        }
        ButtonTrainingInteraction::Edit(id) => {
            edit_button_interaction(ctx, mci, *id, &db_user).await?
        }
        ButtonTrainingInteraction::Leave(id) => {
            leave_button_interaction(ctx, mci, *id, &db_user).await?
        }
        ButtonTrainingInteraction::Comment(id) => {
            comment_button_interaction(ctx, mci, *id, &db_user).await?
        }
    }
    Ok(())
}

async fn button_list_interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
) -> LogResult<()> {
    let db_user = match db::User::by_discord_id(ctx, mci.user.id).await {
        Ok(u) => u,
        Err(diesel::NotFound) => {
            mci.create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource);
                r.interaction_response_data(|d| {
                    d.flags(CallbackDataFlags::EPHEMERAL);
                    d.content("Not registered. Please register first")
                })
            })
            .await?;
            return Err("Not yet registered".to_string()).log_only();
        }
        Err(e) => return Err(e).log_only(),
    };

    let signups = db_user.active_signups(ctx).await?;
    let mut roles: HashMap<i32, Vec<db::Role>> = HashMap::with_capacity(signups.len());
    for (s, _) in &signups {
        let signup_roles = s.clone().get_roles(ctx).await.log_only()?;
        roles.insert(s.id, signup_roles);
    }
    let emb = signup_list_embed(&signups, &roles);

    let msg = mci.user.direct_message(ctx, |m| m.set_embed(emb)).await;

    match msg {
        Ok(_) => {
            if in_public_channel(ctx, mci).await {
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::DeferredUpdateMessage);
                    r
                })
                .await?;
            } else {
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::DeferredUpdateMessage);
                    r
                })
                .await?;
            }
        }
        Err(e) => {
            mci.create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource);
                r.interaction_response_data(|d| {
                    d.flags(CallbackDataFlags::EPHEMERAL);
                    d.content("I was unable to DM you =(")
                });
                r
            })
            .await?;
            return Err(e).log_only();
        }
    };
    Ok(())
}

async fn button_register_interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
) -> LogResult<()> {
    let msg = mci
        .user
        .direct_message(ctx, |m| m.set_embed(register_instructions_embed()))
        .await;
    match msg {
        Ok(m) => {
            if in_public_channel(ctx, mci).await {
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::ChannelMessageWithSource);
                    r.interaction_response_data(|d| {
                        d.flags(CallbackDataFlags::EPHEMERAL);
                        d.content(format!(
                            "{} Check [DM's]({})",
                            Mention::from(mci.user.id),
                            m.link()
                        ))
                    });
                    r
                })
                .await?;
            } else {
                mci.create_interaction_response(ctx, |r| {
                    r.kind(InteractionResponseType::DeferredUpdateMessage);
                    r
                })
                .await?;
            }
        }
        Err(e) => {
            mci.create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource);
                r.interaction_response_data(|d| {
                    d.flags(CallbackDataFlags::EPHEMERAL);
                    d.content("I was unable to DM you =(")
                });
                r
            })
            .await?;
            return Err(e).log_only();
        }
    };
    Ok(())
}

async fn button_general_interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    bgi: &ButtonGeneralInteraction,
) -> LogResult<()> {
    match bgi {
        ButtonGeneralInteraction::List => button_list_interaction(ctx, mci).await,
        ButtonGeneralInteraction::Register => button_register_interaction(ctx, mci).await,
    }
}

pub async fn button_interaction(ctx: &Context, mci: &MessageComponentInteraction) {
    // Check what interaction to handle
    if let Ok(bi) = mci.data.custom_id.parse::<ButtonInteraction>() {
        log_interaction(ctx, mci, &bi, || async {
            match &bi {
                ButtonInteraction::Training(bti) => {
                    button_training_interaction(ctx, mci, bti).await
                }
                ButtonInteraction::General(bgi) => button_general_interaction(ctx, mci, bgi).await,
            }
        })
        .await;
    };
}

// helper
async fn in_public_channel(ctx: &Context, mci: &MessageComponentInteraction) -> bool {
    mci.channel_id
        .to_channel(ctx)
        .await
        .map_or(false, |c| c.private().is_none())
}
