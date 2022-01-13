use serenity::{
    builder::CreateApplicationCommand,
    client::Context,
    model::interactions::application_command::{
        ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
        ApplicationCommandOptionType,
    },
};

use crate::log::{log_slash, LogError, LogResult};

pub const CMD_TRAINING_BOSS: &str = "training_boss";

pub fn create() -> CreateApplicationCommand {
    let mut app = CreateApplicationCommand::default();
    app.name(CMD_TRAINING_BOSS);
    app.description("Manage bosses for training");
    app.default_permission(false);
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("add");
        o.description("Add a boss");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("name");
            o.description("The full name of the boss");
            o.required(true)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("repr");
            o.description(
                "A short identifier for the boss. Has to be unique. Will be exported on download",
            );
            o.required(true)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::Integer);
            o.name("wing");
            o.description("The wing the boss belongs to");
            o.required(true);
            o.add_int_choice("Wing 1", 1);
            o.add_int_choice("Wing 2", 2);
            o.add_int_choice("Wing 3", 3);
            o.add_int_choice("Wing 4", 4);
            o.add_int_choice("Wing 5", 5);
            o.add_int_choice("Wing 6", 6);
            o.add_int_choice("Wing 7", 7)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::Integer);
            o.name("boss");
            o.description("Which boss it is in the specified wing");
            o.required(true)
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("remove");
        o.description("Remove a boss");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("repr");
            o.description("The unique identifier of the boss");
            o.required(true)
        })
    });
    app
}

pub async fn handle(ctx: &Context, aci: &ApplicationCommandInteraction) {
    log_slash(ctx, aci, || async {
        if let Some(sub) = aci.data.options.get(0) {
            match sub.name.as_ref() {
                "add" => add(ctx, aci, sub).await,
                "remove" => remove(ctx, aci, sub).await,
                _ => Err(LogError::new_slash("Not yet handled", aci.clone())),
            }
        } else {
            Err(LogError::new_slash("Invalid command", aci.clone()))
        }
    })
    .await;
}

async fn add(
    _ctx: &Context,
    _aci: &ApplicationCommandInteraction,
    _option: &ApplicationCommandInteractionDataOption,
) -> LogResult<()> {
    Ok(())
}

async fn remove(
    _ctx: &Context,
    _aci: &ApplicationCommandInteraction,
    _option: &ApplicationCommandInteractionDataOption,
) -> LogResult<()> {
    Ok(())
}
