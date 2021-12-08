use std::collections::HashMap;

use crate::{
    components,
    db::{self, TrainingState},
    log::*,
};
use chrono::NaiveDate;
use serenity::{
    builder::CreateApplicationCommand,
    client::Context,
    futures::future,
    model::interactions::{
        application_command::{
            ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
            ApplicationCommandOptionType,
        },
        InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
    },
};

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
            o.add_string_choice("running", "running");
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
    app
}

pub async fn handle(ctx: &Context, aci: &ApplicationCommandInteraction) {
    log_slash(ctx, aci, || async {
        if let Some(sub) = aci.data.options.get(0) {
            match sub.name.as_ref() {
                "set" => set(ctx, aci, sub).await,
                _ => Err(LogError::new_slash("Not yet handled", aci.clone())),
            }
        } else {
            Err(LogError::new_slash("Invalid command", aci.clone()))
        }
    })
    .await;
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
    let mut ids: Vec<db::Training> = Vec::new();

    if let Some(day_str) = cmds
        .get("day")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str().to_owned())
        .and_then(|d| Some(d.split(",")))
    {
        let days: Vec<NaiveDate> = day_str
            .into_iter()
            .map(|s| s.parse())
            .collect::<Result<Vec<_>, _>>()
            .log_slash_reply(aci)?;

        let trainings_fut = days
            .into_iter()
            .map(|d| db::Training::by_date(ctx, d))
            .collect::<Vec<_>>();

        ids.append(
            &mut future::try_join_all(trainings_fut)
                .await
                .log_slash_reply(aci)?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>(),
        );
    }

    if let Some(id_str) = cmds
        .get("ids")
        .and_then(|i| i.value.as_ref())
        .and_then(|i| i.as_str().to_owned())
        .and_then(|i| Some(i.split(",")))
    {
        let i: Vec<i32> = id_str
            .into_iter()
            .map(|s| s.parse())
            .collect::<Result<Vec<_>, _>>()
            .log_slash_reply(aci)?;

        let trainings_fut = i
            .into_iter()
            .map(|i| db::Training::by_id(ctx, i))
            .collect::<Vec<_>>();

        ids.append(
            &mut future::try_join_all(trainings_fut)
                .await
                .log_slash_reply(aci)?,
        );
    }

    // filter out multiple
    ids.sort_by_key(|t| t.id);
    ids.dedup_by_key(|t| t.id);
    ids.sort_by_key(|t| t.date);

    aci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
            d.create_embed(|e| {
                e.title("Change training state");
                e.description(format!("Setting the following trainings to: **{}**", state));
                e.fields(ids.iter().map(|id| {
                    (
                        id.title.to_string(),
                        format!("<t:{}>", id.date.timestamp()),
                        true,
                    )
                }))
            });
            d.components(|c| c.add_action_row(components::confirm_abort_action_row(false)))
        })
    })
    .await?;

    //FIXME is this the correct follow up?
    //aci.get_interaction_response(ctx).await?;

    Ok(())
}
