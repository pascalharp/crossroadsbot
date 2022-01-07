use std::{borrow::Cow, collections::HashMap};

use crate::{
    components, data,
    db::{self, TrainingState},
    embeds::CrossroadsEmbeds,
    log::*,
    signup_board, status,
    utils::DEFAULT_TIMEOUT,
};
use chrono::{NaiveDate, NaiveDateTime};
use serde::{
    ser::{SerializeSeq, SerializeStruct},
    Serialize,
};
use serenity::model::interactions::{
    application_command::{
        ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
        ApplicationCommandOptionType,
    },
    InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
};
use serenity::{
    builder::{CreateApplicationCommand, CreateEmbed},
    client::Context,
    futures::future,
    http::AttachmentType,
    model::{
        guild::{Member, PartialGuild, Role},
        id::RoleId,
        misc::Mention,
    },
};

type MessageFlags = InteractionApplicationCommandCallbackDataFlags;

pub const CMD_TRAINING: &str = "training";

pub fn create() -> CreateApplicationCommand {
    let mut app = CreateApplicationCommand::default();
    app.name(CMD_TRAINING);
    app.description("Manage trainings");
    app.default_permission(false);
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("set");
        o.description("Change the state of one or multiple training(s)");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("state");
            o.description("The state to set to");
            o.required(true);
            o.add_string_choice("created", "created");
            o.add_string_choice("open", "open");
            o.add_string_choice("closed", "closed");
            o.add_string_choice("started", "started");
            o.add_string_choice("finished", "finished")
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("day");
            o.description(
                "Select all trainings from that day. Format: yyyy-mm-dd. Comma separated list",
            )
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("ids");
            o.description("Select training(s) with the specified id. Comma separated list")
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("download");
        o.description("Download one or multiple training(s)");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("day");
            o.description(
                "Select all trainings from that day. Format: yyyy-mm-dd. Comma separated list",
            )
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("ids");
            o.description("Select training(s) with the specified id. Comma separated list")
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("format");
            o.description("Select the download format. Default: csv");
            o.add_string_choice("json", "json");
            o.add_string_choice("csv", "csv")
        })
    });
    app
}

pub async fn handle(ctx: &Context, aci: &ApplicationCommandInteraction) {
    log_slash(ctx, aci, || async {
        if let Some(sub) = aci.data.options.get(0) {
            match sub.name.as_ref() {
                "set" => set(ctx, aci, sub).await,
                "download" => download(ctx, aci, sub).await,
                _ => Err(LogError::new_slash("Not yet handled", aci.clone())),
            }
        } else {
            Err(LogError::new_slash("Invalid command", aci.clone()))
        }
    })
    .await;
}

async fn trainings_from_days(ctx: &Context, value: &str) -> LogResult<Vec<db::Training>> {
    let days: Vec<NaiveDate> = value
        .split(',')
        .map(|s| s.parse())
        .collect::<Result<Vec<_>, _>>()
        .log_only()?;

    let trainings_fut = days
        .into_iter()
        .map(|d| db::Training::by_date(ctx, d))
        .collect::<Vec<_>>();

    Ok(future::try_join_all(trainings_fut)
        .await
        .log_only()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>())
}

async fn trainings_from_ids(ctx: &Context, value: &str) -> LogResult<Vec<db::Training>> {
    let i: Vec<i32> = value
        .split(',')
        .map(|s| s.parse())
        .collect::<Result<Vec<_>, _>>()
        .log_only()?;

    let trainings_fut = i
        .into_iter()
        .map(|i| db::Training::by_id(ctx, i))
        .collect::<Vec<_>>();

    Ok(future::try_join_all(trainings_fut).await.log_only()?)
}

async fn set(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    option: &ApplicationCommandInteractionDataOption,
) -> LogResult<()> {
    // Get subcommands
    let cmds = option
        .options
        .iter()
        .map(|o| (o.name.clone(), o))
        .collect::<HashMap<_, _>>();

    // required and pre defined so fine to unwrap
    let state: TrainingState = cmds
        .get("state")
        .unwrap()
        .value
        .as_ref()
        .unwrap()
        .as_str()
        .unwrap()
        .parse()
        .log_slash_reply(aci)?;

    // Although loading full trainings is a bit overhead
    // it also guarantees they exist
    let mut trainings: Vec<db::Training> = Vec::new();

    if let Some(days) = cmds
        .get("day")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str())
    {
        trainings.append(&mut trainings_from_days(ctx, days).await.log_slash_reply(aci)?);
    }

    if let Some(ids) = cmds
        .get("ids")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str())
    {
        trainings.append(&mut trainings_from_ids(ctx, ids).await.log_slash_reply(aci)?);
    }

    if trainings.is_empty() {
        return Err(LogError::new_slash(
            "Select at least one training",
            aci.clone(),
        ));
    }

    // filter out multiple
    trainings.sort_by_key(|t| t.id);
    trainings.dedup_by_key(|t| t.id);
    trainings.sort_by_key(|t| t.date);

    let mut te = CreateEmbed::xdefault();
    te.title("Change training state");
    te.description(format!("Setting the following trainings to: **{}**", state));
    te.fields(trainings.iter().map(|id| {
        (
            format!("{} | {}", id.id, id.title),
            format!("<t:{}>", id.date.timestamp()),
            true,
        )
    }));

    aci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(MessageFlags::EPHEMERAL);
            d.add_embed(te.clone());
            d.components(|c| c.add_action_row(components::confirm_abort_action_row(false)))
        })
    })
    .await?;

    let msg = aci.get_interaction_response(ctx).await?;
    match msg
        .await_component_interaction(ctx)
        .timeout(DEFAULT_TIMEOUT)
        .await
    {
        Some(response) => {
            match components::resolve_button_response(&response) {
                components::ButtonResponse::Confirm => {
                    response
                        .create_interaction_response(ctx, |r| {
                            r.kind(InteractionResponseType::UpdateMessage);
                            r.interaction_response_data(|d| {
                                d.flags(MessageFlags::EPHEMERAL);
                                d.components(|c| c)
                            })
                        })
                        .await?;

                    let update_futs: Vec<_> = trainings
                        .into_iter()
                        .map(|t| t.set_state(ctx, state.clone()))
                        .collect();
                    let trainings = future::try_join_all(update_futs).await?;

                    response
                        .create_followup_message(ctx, |m| {
                            m.flags(MessageFlags::EPHEMERAL);
                            m.add_embed(te.clone());
                            m.create_embed(|e| {
                                e.title("Trainings updated");
                                e.description("Updating Signup Board")
                            })
                        })
                        .await?;

                    let mut se = CreateEmbed::xdefault();
                    se.title("Signup board updates");
                    for id in trainings.iter().map(|t| t.id) {
                        let res = signup_board::SignupBoard::update_training(ctx, id).await;
                        match res {
                            Ok(some) => match some {
                                Some(msg) => {
                                    se.field(
                                        format!("Training id: {}", id),
                                        format!("[Message on Board]({})", msg.link()),
                                        true,
                                    );
                                }
                                None => {
                                    se.field(
                                        format!("Training id: {}", id),
                                        "_Message removed_".to_string(),
                                        true,
                                    );
                                }
                            },
                            Err(err) => {
                                se.field(
                                    format!("Training id: {}", id),
                                    format!("_Error_: {}", err),
                                    true,
                                );
                            }
                        }
                    }

                    response
                        .create_followup_message(ctx, |m| {
                            m.flags(MessageFlags::EPHEMERAL);
                            m.add_embed(te);
                            m.add_embed(se)
                        })
                        .await?;

                    status::update_status(ctx).await;
                }
                components::ButtonResponse::Abort => {
                    response
                        .create_interaction_response(ctx, |r| {
                            r.kind(InteractionResponseType::UpdateMessage);
                            r.interaction_response_data(|d| {
                                d.flags(MessageFlags::EPHEMERAL);
                                d.content("Aborted");
                                d.embeds(std::iter::empty());
                                d.components(|c| c)
                            })
                        })
                        .await?;
                }
                // Should not be possible
                _ => unimplemented!(),
            }
        }
        None => {
            aci.edit_followup_message(ctx, msg.id, |m| {
                m.flags(MessageFlags::EPHEMERAL);
                m.content("Timed out");
                m.create_embed(|e| e);
                m.components(|c| c)
            })
            .await?;
        }
    };

    Ok(())
}

enum DonwloadFormat {
    Json,
    Csv,
}

impl std::fmt::Display for DonwloadFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Csv => write!(f, "csv"),
            Self::Json => write!(f, "json"),
        }
    }
}

#[derive(Serialize)]
struct SignupData {
    user: db::User,
    member: Member,
    roles: Vec<String>, // we only need the repr here
    comment: Option<String>,
}

// since csv is all row based edit on the fly
#[derive(Serialize)]
struct SignupDataCsv<'a> {
    #[serde(rename = "Gw2 Account")]
    gw2_acc: &'a str,
    #[serde(rename = "Discord Account")]
    discord_acc: &'a str,
    #[serde(rename = "Discord Ping")]
    discord_ping: &'a str,
    #[serde(rename = "Training Name")]
    training_name: &'a str,
    #[serde(rename = "Roles")]
    roles: &'a str,
    #[serde(rename = "Comment")]
    comment: Option<&'a str>,
}

#[derive(Serialize)]
struct TrainingData {
    training: db::Training,
    available_roles: Vec<db::Role>,
    signups: Vec<SignupData>,
}

#[derive(Serialize)]
struct TierData {
    id: i32,
    name: String,
    includes: Vec<Role>,
}

struct DownloadData {
    output: DonwloadFormat,
    created: NaiveDateTime,
    trainings: Vec<TrainingData>,
    tiers: Vec<TierData>,
}

impl DownloadData {
    fn serialize_csv<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.trainings.len()))?;

        for t in &self.trainings {
            for s in &t.signups {
                let elem = SignupDataCsv {
                    gw2_acc: &s.user.gw2_id,
                    discord_acc: &s.member.user.tag(),
                    discord_ping: &Mention::from(&s.member).to_string(),
                    training_name: &t.training.title,
                    roles: &s
                        .roles
                        .iter()
                        .map(|r| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    comment: s.comment.as_deref(),
                };

                seq.serialize_element(&elem)?;
            }
        }

        seq.end()
    }

    fn serialize_json<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut elem = serializer.serialize_struct("data", 2)?;
        elem.serialize_field("created", &self.created)?;
        elem.serialize_field("trainings", &self.trainings)?;
        elem.serialize_field("tiers", &self.tiers)?;
        elem.end()
    }
}

impl Serialize for DownloadData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.output {
            DonwloadFormat::Csv => self.serialize_csv(serializer),
            DonwloadFormat::Json => self.serialize_json(serializer),
        }
    }
}

async fn download(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    option: &ApplicationCommandInteractionDataOption,
) -> LogResult<()> {
    let guild_id = match ctx.data.read().await.get::<data::ConfigValuesData>() {
        Some(conf) => conf.main_guild_id,
        None => {
            return LogError::new_slash("Guild configuration could not be loaded", aci.clone())
                .into()
        }
    };

    let guild = PartialGuild::get(ctx, guild_id)
        .await
        .log_slash_reply(aci)?;

    // Get subcommands
    let cmds = option
        .options
        .iter()
        .map(|o| (o.name.clone(), o))
        .collect::<HashMap<_, _>>();

    // Although loading full trainings is a bit overhead
    // it also guarantees they exist
    let mut trainings: Vec<db::Training> = Vec::new();

    if let Some(days) = cmds
        .get("day")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str())
    {
        trainings.append(&mut trainings_from_days(ctx, days).await.log_slash_reply(aci)?);
    }

    if let Some(ids) = cmds
        .get("ids")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str())
    {
        trainings.append(&mut trainings_from_ids(ctx, ids).await.log_slash_reply(aci)?);
    }

    if trainings.is_empty() {
        return Err(LogError::new_slash(
            "Select at least one training",
            aci.clone(),
        ));
    }

    // filter out multiple
    trainings.sort_by_key(|t| t.id);
    trainings.dedup_by_key(|t| t.id);
    trainings.sort_by_key(|t| t.date);

    // What to parse to
    let format = if let Some(f) = cmds
        .get("format")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str())
    {
        match f {
            "csv" => DonwloadFormat::Csv,
            "json" => DonwloadFormat::Json,
            _ => unimplemented!(),
        }
    } else {
        DonwloadFormat::Csv // Default
    };

    aci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(MessageFlags::EPHEMERAL);
            d.content("Loading...")
        })
    })
    .await?;

    let msg = aci.get_interaction_response(ctx).await?;

    let mut log: Vec<String> = Vec::new();
    let mut tds: Vec<TrainingData> = Vec::with_capacity(trainings.len());

    for t in trainings {
        let signups = t.get_signups(ctx).await.log_slash_reply(aci)?;
        let mut sds: Vec<SignupData> = Vec::with_capacity(signups.len());

        for s in signups {
            let user = s.get_user(ctx).await.log_slash_reply(aci)?;

            let member = match guild.member(ctx, user.discord_id()).await {
                Ok(du) => du,
                Err(_) => {
                    log.push(format!(
                        "Did not find user with id {} in discord guild. Skipped",
                        user.discord_id()
                    ));
                    continue;
                }
            };

            let roles = s
                .get_roles(ctx)
                .await
                .log_slash_reply(aci)?
                .into_iter()
                .map(|r| r.repr)
                .collect::<Vec<_>>();

            sds.push(SignupData {
                user,
                member,
                roles,
                comment: s.comment,
            });
        }

        let available_roles = t.all_roles(ctx).await.log_slash_reply(aci)?;

        tds.push(TrainingData {
            training: t,
            available_roles,
            signups: sds,
        });
    }

    let dbtiers = db::Tier::all(ctx).await.log_slash_reply(aci)?;
    let mut tiers: Vec<TierData> = Vec::with_capacity(dbtiers.len());

    for t in dbtiers {
        let dr = t
            .get_discord_roles(ctx)
            .await
            .log_slash_reply(aci)?
            .iter()
            .map(|t| guild.roles.get(&RoleId::from(t.discord_role_id as u64)))
            .collect::<Vec<_>>();

        let mut includes: Vec<Role> = Vec::with_capacity(dr.len());

        for r in dr {
            match r {
                Some(r) => includes.push(r.clone()),
                None => log.push(format!(
                    "Failed to find a discord role for tier: {}",
                    t.name
                )),
            }
        }

        tiers.push(TierData {
            id: t.id,
            name: t.name,
            includes,
        })
    }

    let data = DownloadData {
        output: format,
        created: chrono::Utc::now().naive_utc(),
        trainings: tds,
        tiers,
    };

    let data_bytes = match data.output {
        DonwloadFormat::Csv => {
            let mut wrt = csv::Writer::from_writer(vec![]);
            wrt.serialize(&data).log_slash_reply(aci)?;
            String::from_utf8(wrt.into_inner().log_slash_reply(aci)?)
                .log_slash_reply(aci)?
                .into_bytes()
        }
        DonwloadFormat::Json => {
            let json = serde_json::to_string_pretty(&data).log_slash_reply(aci)?;
            json.as_bytes().to_vec()
        }
    };

    let file = AttachmentType::Bytes {
        data: Cow::from(data_bytes),
        filename: String::from("signups.csv"),
    };

    let msg = msg
        .channel_id
        .send_message(ctx, |m| {
            m.embed(|e| {
                e.title("Download");
                e.field(
                    "Details",
                    format!(
                        "Format: {}\nCreated: <t:{}>",
                        data.output,
                        data.created.timestamp()
                    ),
                    false,
                );
                e.field(
                    "Trainings",
                    data.trainings
                        .iter()
                        .map(|t| {
                            format!(
                                "\n__{}__\nId: {}\nData: <t:{}>",
                                t.training.title,
                                t.training.id,
                                t.training.date.timestamp()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    false,
                );
                e.field(
                    "Log",
                    match log.is_empty() {
                        true => String::from("No errors"),
                        false => log
                            .iter()
                            .map(|s| format!("`{}`", s))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    },
                    false,
                )
            });
            m.add_file(file)
        })
        .await?;

    aci.edit_original_interaction_response(ctx, |r| r.content(format!("[Done]({})", msg.link())))
        .await?;

    Ok(())
}
