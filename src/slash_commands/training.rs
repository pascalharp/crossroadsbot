use std::{collections::HashMap, borrow::Cow};

use crate::{
    components,
    db::{TrainingState, self},
    embeds::CrossroadsEmbeds,
    log::*,
    signup_board, status,
    utils::DEFAULT_TIMEOUT,
};
use chrono::{NaiveDate, NaiveDateTime};
use serde::{ser::{SerializeSeq, SerializeStruct}, Serialize};
use serenity::{
    builder::{CreateApplicationCommand, CreateEmbed},
    client::Context,
    futures::future, model::{guild::Member, misc::Mention}, http::AttachmentType,
};
use serenity::model::interactions::{
        application_command::{
            ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
            ApplicationCommandOptionType,
        },
        InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
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

    Ok(
        future::try_join_all(trainings_fut)
            .await
            .log_only()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
    )
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

        Ok(
            future::try_join_all(trainings_fut)
                .await
                .log_only()?
        )
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
        trainings.append(
            &mut trainings_from_days(ctx, days).await.log_slash_reply(aci)?
        );
    }

    if let Some(ids) = cmds
        .get("ids")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str())
    {
        trainings.append(
            &mut trainings_from_ids(ctx, ids).await.log_slash_reply(aci)?
        );
    }

    if trainings.is_empty() {
        return Err(LogError::new_slash("Select at least one training", aci.clone()));
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

#[derive(Serialize)]
struct SignupData<'a> {
    user: db::User,
    member: Member,
    roles: Vec<&'a db::Role>,
    comment: Option<String>
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
struct TrainingData<'a> {
    training: db::Training,
    available_roles: Vec<db::Role>,
    signups: Vec<SignupData<'a>>,
}

struct DownloadData<'a> {
    output: DonwloadFormat,
    created: NaiveDateTime,
    trainings: Vec<TrainingData<'a>>,
}

impl DownloadData<'_> {

    fn serialize_csv<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {

        let mut seq = serializer.serialize_seq(Some(self.trainings.len()))?;

        for t in &self.trainings {
            for s in &t.signups {

                let elem = SignupDataCsv {
                    gw2_acc: &s.user.gw2_id,
                    discord_acc: &s.member.user.tag(),
                    discord_ping: &Mention::from(&s.member).to_string(),
                    training_name: &t.training.title,
                    roles: &s.roles
                        .iter()
                        .map(|r| r.repr.clone())
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
        S: serde::Serializer {
            let mut elem = serializer.serialize_struct("data", 2)?;
            elem.serialize_field("created", &self.created)?;
            elem.serialize_field("trainings", &self.trainings)?;
            elem.end()
    }
}

impl Serialize for DownloadData<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
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
        trainings.append(
            &mut trainings_from_days(ctx, days).await.log_slash_reply(aci)?
        );
    }

    if let Some(ids) = cmds
        .get("ids")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str())
    {
        trainings.append(
            &mut trainings_from_ids(ctx, ids).await.log_slash_reply(aci)?
        );
    }

    if trainings.is_empty() {
        return Err(LogError::new_slash("Select at least one training", aci.clone()));
    }


    // What to parse to
    let format = if let Some(f) = cmds
        .get("format")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str()) {
        match f {
            "csv" => DonwloadFormat::Csv,
            "json" => DonwloadFormat::Json,
            _ => unimplemented!()
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
    }).await?;

    let _msg = aci.get_interaction_response(ctx).await?;

    let data = DownloadData {
        output: format,
        created: chrono::Utc::now().naive_utc(),
        trainings: Vec::new(),
    };

    let data_bytes = match data.output {
        DonwloadFormat::Csv => {
            let mut wrt = csv::Writer::from_writer(vec![]);
            wrt.serialize(&data).log_slash_reply(aci)?;
            String::from_utf8(
                wrt.into_inner().log_slash_reply(aci)?
                )
                .log_slash_reply(aci)?
                .into_bytes()
        },
        DonwloadFormat::Json => {
            let json = serde_json::to_string(&data).log_slash_reply(aci)?;
            json.as_bytes().to_vec()
        }
    };

    let file = AttachmentType::Bytes {
        data: Cow::from(data_bytes),
        filename: String::from("signups.csv"),
    };

    aci.create_followup_message(ctx, |r| {
        r.content("Done");
        r.add_file(file)
    }).await?;

    Ok(())
}
