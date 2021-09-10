use crate::{components::*, conversation::*, db, embeds::*, log::*, utils::*};

use serenity::{
    futures::future,
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

pub async fn button_training_interaction(
    ctx: &Context,
    mci: &MessageComponentInteraction,
    bti: ButtonTrainingInteraction,
) {
    log_interaction(ctx, mci, &bti, || async {
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

pub async fn button_interaction(ctx: &Context, mci: &MessageComponentInteraction) {
    // Check what interaction to handle
    match mci.data.custom_id.parse::<ButtonInteraction>() {
        Err(_) => {}
        Ok(ButtonInteraction::Training(bti)) => button_training_interaction(ctx, mci, bti).await,
        Ok(ButtonInteraction::General(_)) => unimplemented!(),
    };
}

// helper
async fn in_public_channel(ctx: &Context, mci: &MessageComponentInteraction) -> bool {
    mci.channel_id
        .to_channel(ctx)
        .await
        .map_or(false, |c| c.private().is_none())
}
