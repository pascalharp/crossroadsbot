use serenity::{builder::CreateApplicationCommand, client::Context, model::interactions::application_command::{ApplicationCommandInteraction, ApplicationCommandOptionType, ApplicationCommandInteractionDataOption}};
use crate::log::*;

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
            o.description("Select all trainings from that day. Format: yyyy-mm-dd")
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
                _ => Err(LogError::new_slash("Not yet handled", aci.clone()))
            }
        } else {
            Err(LogError::new_slash("Invalid command", aci.clone()))
        }
    }).await;
}

async fn set(ctx: &Context, aci: &ApplicationCommandInteraction, option: &ApplicationCommandInteractionDataOption) -> LogResult<()> {
   Err(LogError::new_slash("TODO", aci.clone()))
}
