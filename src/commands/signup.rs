use crate::{components::*, conversation::*, data, db, embeds::*, log::*, utils};
use regex::Regex;
use serenity::builder::CreateEmbed;
use serenity::framework::standard::{
    macros::{command, group},
    Args, CommandResult,
};
use serenity::futures::future;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::collections::{HashMap, HashSet};

#[group]
#[commands(register, join, leave, edit, list)]
struct Signup;

#[command]
#[description = "Register or update your GW2 account name with the bot"]
#[example = "AccountName.1234"]
#[usage = "account_name"]
#[num_args(1)]
pub async fn register(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let acc_name = args.single::<String>().log_reply(&msg)?;
        let re = Regex::new("^[a-zA-Z]{3,27}\\.[0-9]{4}$").unwrap();
        if !re.is_match(&acc_name) {
            return LogError::new("Invalid gw2 account name format", msg).into();
        }

        // this is an update on conflict
        let new_user = db::User::upsert(ctx, *msg.author.id.as_u64(), acc_name)
            .await
            .log_unexpected_reply(msg)?;
        msg.reply(ctx, format!("Gw2 account name set to: {}", new_user.gw2_id))
            .await?;
        Ok(())
    })
    .await
}

#[command]
#[description = "Join a training with the provided id"]
#[example = "103"]
#[usage = "training_id"]
#[num_args(1)]
pub async fn join(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let id = args.single_quoted::<i32>().log_reply(msg)?;
        let db_user = match db::User::by_discord_id(ctx, msg.author.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                let embed = not_registered_embed();
                msg.channel_id
                    .send_message(ctx, |m| {
                        m.reference_message(msg);
                        m.add_embed(|e| {
                            e.0 = embed.0;
                            e
                        })
                    })
                    .await?;
                return LogError::new_silent(NOT_REGISTERED).into();
            }
            Err(e) => return Err(e).log_unexpected_reply(msg),
        };

        let training = match db::Training::by_id_and_state(ctx, id, db::TrainingState::Open).await {
            Ok(t) => t,
            Err(diesel::NotFound) => {
                let reply = format!("No **open** training with id: {}", id);
                return LogError::new(&reply, msg).into();
            }
            Err(e) => return Err(e).log_unexpected_reply(msg),
        };

        let emb = training_base_embed(&training);

        let mut conv = Conversation::init(ctx, &msg.author, emb)
            .await
            .log_reply(msg)?;

        // if not already in dms give a hint
        if !msg.is_private() {
            msg.reply(ctx, format!("Check DM's {}", utils::ENVELOP_EMOJI))
                .await
                .ok();
        }

        // verify if tier requirements pass
        match utils::verify_tier(ctx, &training, &conv.user).await {
            Ok((pass, tier)) => {
                if !pass {
                    conv.msg
                        .edit(ctx, |m| {
                            m.content("");
                            m.embed(|e| {
                                e.description("Tier requirement not fulfilled");
                                e.field("Missing tier:", tier, false)
                            })
                        })
                        .await?;
                    return LogError::new("Tier requirement not fulfilled", msg).into();
                }
            }
            Err(e) => return Err(e).log_unexpected_reply(msg),
        };

        // Check if signup already exist
        match db::Signup::by_user_and_training(ctx, &db_user, &training).await {
            Ok(_) => {
                conv.msg
                    .edit(ctx, |m| {
                        m.add_embed(|e| {
                            e.xstyle();
                            e.description("Already signed up for this training");
                            e.field(
                                "You can edit your signup with:",
                                format!("`{}edit {}`", data::GLOB_COMMAND_PREFIX, training.id),
                                false,
                            );
                            e.field(
                                "You can remove your signup with:",
                                format!("`{}leave {}`", data::GLOB_COMMAND_PREFIX, training.id),
                                false,
                            )
                        })
                    })
                    .await?;
                return LogError::new_silent("Already signed up").into();
            }
            Err(diesel::NotFound) => (), // This is what we want
            Err(e) => return Err(e).log_unexpected_reply(&conv.msg),
        };

        let roles = training
            .active_roles(ctx)
            .await
            .log_unexpected_reply(&conv.msg)?;
        let roles_lookup: HashMap<String, &db::Role> =
            roles.iter().map(|r| (String::from(&r.repr), r)).collect();

        // Gather selected roles
        let selected: HashSet<String> = HashSet::with_capacity(roles.len());
        let selected = utils::select_roles(ctx, &mut conv.msg, &conv.user, &roles, selected)
            .await
            .log_reply(&conv.msg)?;

        let signup = db::Signup::insert(ctx, &db_user, &training)
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
    })
    .await
}

#[command]
#[description = "Leave a training you have already signed up up for. Only possible if the training is still open for sign ups"]
#[example = "103"]
#[usage = "training_id"]
#[num_args(1)]
pub async fn leave(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let training_id = args.single_quoted::<i32>().log_reply(msg)?;

        let db_user = match db::User::by_discord_id(ctx, msg.author.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                let emb = not_registered_embed();
                msg.channel_id
                    .send_message(ctx, |m| {
                        m.reference_message(msg);
                        m.embed(|e| {
                            e.0 = emb.0;
                            e
                        })
                    })
                    .await?;
                return LogError::new_silent(NOT_REGISTERED).into();
            }
            Err(e) => return Err(e).log_unexpected_reply(msg),
        };

        let training =
            match db::Training::by_id_and_state(ctx, training_id, db::TrainingState::Open).await {
                Ok(t) => t,
                Err(diesel::NotFound) => {
                    return Err(LogError::new(
                        &format!("No **open** training with id {} found", training_id),
                        msg,
                    ));
                }
                Err(e) => return Err(e).log_unexpected_reply(msg),
            };

        let signup = match db::Signup::by_user_and_training(ctx, &db_user, &training).await {
            Ok(s) => s,
            Err(diesel::NotFound) => {
                msg.channel_id
                    .send_message(ctx, |m| {
                        m.reference_message(msg);
                        m.set_embed(not_signed_up_embed(&training))
                    })
                    .await?;
                return Err(LogError::new_silent(NOT_SIGNED_UP));
            }
            Err(e) => return Err(e).log_unexpected_reply(&msg),
        };

        match signup.remove(ctx).await {
            Ok(1) => (),
            Ok(a) => {
                return Err(LogError::new_custom(
                    format!("Unexpected Error"),
                    format!("Unexpected amount of signups removed: {}", a),
                    msg,
                ))
            }
            Err(e) => return Err(e).log_unexpected_reply(&msg),
        }

        msg.channel_id
            .send_message(ctx, |m| {
                m.reference_message(msg);
                m.content("");
                m.embed(|e| {
                    e.0 = signed_out_embed(&training).0;
                    e
                });
                m.components(|c| c.add_action_row(join_action_row(training.id)));
                m
            })
            .await?;
        Ok(())
    })
    .await
}

#[command]
#[description = "Edit your sign up"]
#[example = "103"]
#[usage = "training_id"]
#[num_args(1)]
pub async fn edit(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let training_id = args.single_quoted::<i32>().log_reply(msg)?;

        let training =
            match db::Training::by_id_and_state(ctx, training_id, db::TrainingState::Open).await {
                Ok(t) => t,
                Err(diesel::NotFound) => {
                    let reply = format!("No **open** training with id {} found", training_id);
                    return Err(LogError::new(reply, msg));
                }
                Err(e) => return Err(e).log_unexpected_reply(msg),
            };

        let signup =
            match db::Signup::by_discord_user_and_training(ctx, &msg.author.id, &training).await {
                Ok(s) => s,
                Err(diesel::NotFound) => {
                    msg.channel_id
                        .send_message(ctx, |m| {
                            m.reference_message(msg);
                            m.set_embed(not_signed_up_embed(&training))
                        })
                        .await?;
                    return Err(LogError::new_silent(NOT_SIGNED_UP));
                }
                Err(e) => return Err(e).log_unexpected_reply(msg),
            };

        let emb = training_base_embed(&training);

        let mut conv = Conversation::init(ctx, &msg.author, emb)
            .await
            .log_reply(msg)?;

        // if not already in dms give a hint
        if !msg.is_private() {
            msg.reply(ctx, format!("Check DM's {}", utils::ENVELOP_EMOJI))
                .await
                .ok();
        }

        let roles = training.active_roles(ctx).await?;
        let roles_lookup: HashMap<String, &db::Role> =
            roles.iter().map(|r| (String::from(&r.repr), r)).collect();

        // Get new roles from user
        let mut selected: HashSet<String> = HashSet::with_capacity(roles.len());
        let already_selected = signup.get_roles(ctx).await?;
        for r in already_selected {
            selected.insert(r.repr);
        }
        let selected = utils::select_roles(ctx, &mut conv.msg, &conv.user, &roles, selected)
            .await
            .log_reply(&conv.msg)?;

        // Save new roles
        signup
            .clear_roles(ctx)
            .await
            .log_unexpected_reply(&conv.msg)?;
        // We inserted all roles into the HashMap, so it is save to unwrap
        let futs = selected.iter().filter_map(|r| {
            roles_lookup
                .get(r)
                .and_then(|r| Some(signup.add_role(ctx, *r)))
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
    })
    .await
}

#[command]
#[description = "Lists all active trainings you are currently signed up for"]
#[example = ""]
#[usage = ""]
#[num_args(0)]
pub async fn list(ctx: &Context, msg: &Message, _: Args) -> CommandResult {
    log_command(ctx, msg, || async {
        let db_user = match db::User::by_discord_id(ctx, msg.author.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                let embed = not_registered_embed();
                msg.channel_id
                    .send_message(ctx, |m| {
                        m.reference_message(msg);
                        m.add_embed(|e| {
                            e.0 = embed.0;
                            e
                        })
                    })
                    .await?;
                return Err(LogError::new_silent(NOT_REGISTERED));
            }
            Err(e) => return Err(e).log_unexpected_reply(msg),
        };

        let mut emb = CreateEmbed::default();
        emb.xstyle();
        emb.description(format!("User information"));
        emb.field("Guild Wars 2 account name", &db_user.gw2_id, false);

        let mut conv = Conversation::init(ctx, &msg.author, emb)
            .await
            .log_reply(msg)?;

        // if not already in dms give a hint
        if !msg.is_private() {
            msg.reply(ctx, format!("Check DM's {}", utils::ENVELOP_EMOJI))
                .await
                .ok();
        }

        let signups = db_user.active_signups(ctx).await?;
        let mut roles: HashMap<i32, Vec<db::Role>> = HashMap::with_capacity(signups.len());
        for (s, _) in &signups {
            let signup_roles = s
                .clone()
                .get_roles(ctx)
                .await
                .log_unexpected_reply(&conv.msg)?;
            roles.insert(s.id, signup_roles);
        }

        conv.msg
            .edit(ctx, |m| {
                m.add_embed(|e| {
                    e.xstyle();
                    e.description("All current active signups");
                    if signups.is_empty() {
                        e.field(
                            "Zero sign ups found",
                            "You should join some trainings ;)",
                            false,
                        );
                    }
                    for (s, t) in signups {
                        e.field(
                            &t.title,
                            format!(
                                "`Date (YYYY-MM-DD)`\n{}\n\
                                `Time (Utc)       `\n{}\n\
                                `Training Id      `\n{}\n\
                                `Roles            `\n{}\n",
                                t.date.date(),
                                t.date.time(),
                                t.id,
                                match roles.get(&s.id) {
                                    Some(r) => r
                                        .iter()
                                        .map(|r| r.repr.clone())
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                    None => String::from("Failed to load roles =("),
                                }
                            ),
                            true,
                        );
                    }
                    e.footer(|f| {
                        f.text(format!(
                            "To edit or remove your sign up reply with:\n\
                            {0}edit <training id>\n\
                            {0}leave <training id>",
                            data::GLOB_COMMAND_PREFIX
                        ))
                    });
                    e
                })
            })
            .await?;
        Ok(())
    })
    .await
}
