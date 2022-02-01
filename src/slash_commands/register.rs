use anyhow::{anyhow, Context as ErrContext};
use regex::Regex;
use serenity::{
    builder::{CreateApplicationCommand, CreateEmbed},
    client::Context,
    model::interactions::{
        application_command::{ApplicationCommandInteraction, ApplicationCommandOptionType},
        InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
    },
};
use serenity_tools::{
    builder::{CreateComponentsExt, CreateEmbedExt},
    collectors::MessageCollectorExt,
    components::Button,
    interactions::{ApplicationCommandInteractionExt, MessageComponentInteractionExt},
};
use std::time::Duration;

use crate::{
    db,
    logging::{self, log_discord, ReplyHelper},
};

pub(super) const CMD_REGISTER: &'static str = "register";

pub fn create_reg() -> CreateApplicationCommand {
    let mut app = CreateApplicationCommand::default();
    app.name(CMD_REGISTER);
    app.description("Register with to bot to sign up for training's");
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::String);
        o.name("gw2_account");
        o.required(true);
        o.description(
            "\
            Your Guild Wars 2 Account Name existing of your Name and four digits. \
            Example: My Account.1234",
        )
    });
    app
}

pub async fn handle_reg(ctx: &Context, aci: &ApplicationCommandInteraction) {
    log_discord(ctx, aci, |trace| async move {
        trace.step("Parsing command");
        let name = aci
            .data
            .options
            .get(0) // only one option anyway
            .and_then(|v| v.value.as_ref())
            .and_then(|v| v.as_str())
            .ok_or(anyhow!("Unexpected! Missing Guild Wars 2 Account field"))
            .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
            .await?;

        trace.step("Checking for valid format");
        let regex = Regex::new("^[a-zA-Z\\s]{3,27}\\.[0-9]{4}$").unwrap();

        if !regex.is_match(&name) {
            Err(anyhow!("Regex does not match"))
                .context("Invalid Guild Wars 2 Account Name format.\nIt should look something like this: My Account.1234")
                .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
                .await?;
        }

        trace.step("Saving to db");
        let entry = db::User::upsert(ctx, aci.user.id.0, String::from(name))
            .await
            .context("Unexpected error saving your account name =(")
            .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
            .await?;

        aci.create_quick_success(ctx, format!("Guild Wars 2 Account Name set to: {}", entry.gw2_id), true).await?;

        Ok(())
    })
    .await;
}

pub(super) const CMD_UNREGISTER: &'static str = "unregister";

pub fn create_unreg() -> CreateApplicationCommand {
    let mut app = CreateApplicationCommand::default();
    app.name(CMD_UNREGISTER);
    app.description("Unregister with the bot. Removing all information about you");
    app
}

pub async fn handle_unreg(ctx: &Context, aci: &ApplicationCommandInteraction) {
    log_discord(ctx, aci, |trace| async move {
        trace.step("Parsing command");
        trace.step("Looking for user");
        let db_user = match db::User::by_discord_id(ctx, aci.user.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                Err(diesel::NotFound)
                    .context(logging::InfoError::NotRegistered)
                    .context("You were not even registered 😕")
                    .map_err_reply(|what| aci.create_quick_info(ctx, what, true))
                    .await?;
                return Ok(());
            }
            Err(e) => {
                Err(e)
                    .context("Unexpected error fetching user information")
                    .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
                    .await?;
                return Ok(());
            }
        };

        trace.step("Wait for confirmation");
        aci.create_interaction_response(ctx, |r| {
            r.kind(InteractionResponseType::ChannelMessageWithSource);
            r.interaction_response_data(|d| {
                d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
                d.add_embed(CreateEmbed::info_box(
                    "This will remove all of your sign-ups. Are you sure?",
                ));
                d.components(|c| c.confirm_abort_row())
            })
        })
        .await?;

        let msg = aci.get_interaction_response(ctx).await?;

        match msg
            .await_confirm_abort_interaction(ctx)
            .timeout(Duration::from_secs(60))
            .await
        {
            Some(b) => match b.parse_button() {
                Ok(Button::Confirm) => (),
                Ok(Button::Abort) => {
                    Err(logging::InfoError::Aborted)
                        .map_err_reply(|what| aci.edit_quick_info(ctx, what))
                        .await?;
                }
                _ => return Err(anyhow!("Unexpected interaction")),
            },
            None => {
                Err(logging::InfoError::TimedOut)
                    .map_err_reply(|what| aci.edit_quick_info(ctx, what))
                    .await?;
            }
        };

        trace.step("Deleting user entry");
        db_user
            .delete(ctx)
            .await
            .context("Unexpected error deleting your information =(")
            .map_err_reply(|what| aci.edit_quick_error(ctx, what))
            .await?;

        aci.edit_quick_success(ctx, "Your information was deleted")
            .await?;

        Ok(())
    })
    .await;
}
