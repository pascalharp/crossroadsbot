use crate::db;
use chrono::{Duration, NaiveDateTime};
use serenity::{
    builder::{CreateEmbed, CreateEmbedAuthor},
    model::{id::EmojiId, misc::Mention},
};

const EMBED_AUTHOR_ICON_URL: &str = "https://cdn.discordapp.com/avatars/512706205647372302/eb7a7f2de9a97006e8217b73ab5c7836.webp?size=128";
const EMBED_AUTHOR_NAME: &str = "Crossroads Bot";
const EMBED_THUMBNAIL: &str =
    "https://github.com/pascalharp/crossroadsbot/blob/main/resources/inn_bot_logo.png?raw=true";
const EMBED_STYLE_COLOR: (u8, u8, u8) = (99, 51, 45);

pub trait CrossroadsEmbeds {
    fn xdefault() -> Self;
    fn xstyle(&mut self) -> &mut Self;
}

fn xstyle_author() -> CreateEmbedAuthor {
    let mut author = CreateEmbedAuthor::default();
    author.name(EMBED_AUTHOR_NAME);
    author.icon_url(EMBED_AUTHOR_ICON_URL);
    author
}

impl CrossroadsEmbeds for CreateEmbed {
    fn xstyle(&mut self) -> &mut Self {
        self.set_author(xstyle_author());
        self.color(EMBED_STYLE_COLOR);
        self.thumbnail(EMBED_THUMBNAIL);
        self
    }
    fn xdefault() -> Self {
        let mut e = CreateEmbed::default();
        e.xstyle();
        e
    }
}

fn discord_timestamp(dt: &NaiveDateTime) -> String {
    format!("<t:{}:F>", dt.timestamp())
}

const GOOGLE_CALENDAR_TIME_FMT: &str = "%Y%m%dT%H%M%SZ";
pub(crate) fn google_calendar_link(training: &db::Training) -> String {
    let begin = training.date.format(GOOGLE_CALENDAR_TIME_FMT);
    let end = (training.date + Duration::hours(2)).format(GOOGLE_CALENDAR_TIME_FMT);
    format!(
        "https://calendar.google.com/calendar/event?action=TEMPLATE&dates={}/{}&text={}",
        begin,
        end,
        training.title.replace(' ', "%20")
    )
}

const TRAINING_TIME_FMT: &str = "%H:%M (UTC)";

pub(crate) fn field_training_date(training: &db::Training) -> (String, String, bool) {
    (
        "**Date**".to_string(),
        format!(
            "{} | [{}]({})",
            discord_timestamp(&training.date),
            training.date.format(TRAINING_TIME_FMT),
            google_calendar_link(training),
        ),
        false,
    )
}

pub fn embed_add_roles(e: &mut CreateEmbed, roles: &[db::Role], inline: bool, reprs: bool) {
    let title_width = roles
        .iter()
        .map(|r| r.title.len())
        .fold(usize::MIN, std::cmp::max);
    let paged = roles.chunks(10);
    for r in paged {
        let roles_text = r
            .iter()
            .map(|r| {
                if reprs {
                    let repr_width = roles
                        .iter()
                        .map(|r| r.repr.len())
                        .fold(usize::MIN, std::cmp::max);
                    format!(
                        "{} `| {:^rwidth$} |` `| {:^twidth$} |`",
                        Mention::from(EmojiId::from(r.emoji as u64)),
                        &r.repr,
                        &r.title,
                        rwidth = repr_width,
                        twidth = title_width
                    )
                } else {
                    format!(
                        "{} | {} ",
                        Mention::from(EmojiId::from(r.emoji as u64)),
                        &r.title,
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        e.field("Roles", roles_text, inline);
    }
}

fn internal_register_embed(e: &mut CreateEmbed) {
    e.description(
        "To register with the bot simply use the register slash command: `/register` in any channel \
        you have write permissions in.\n\
        It requires your in game account name which you can also find in game on your friends list at the top. \
        It consists of your chosen in game name followed by a dot and 4 digits.\n\n\
        If you want to remove all your information associated with the bot simply use the \
        unregister slash command: `/unregister`",
    );
    e.field("Example Account Name:", "Narturio.1234", false);
}

pub fn register_instructions_embed() -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.title("How to register");
    internal_register_embed(&mut e);
    e
}
