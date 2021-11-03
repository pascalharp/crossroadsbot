use crate::{data::GLOB_COMMAND_PREFIX, db, utils::*};
use chrono::{Duration, NaiveDateTime};
use serenity::{
    builder::{CreateEmbed, CreateEmbedAuthor},
    model::{id::EmojiId, id::RoleId, misc::Mention},
};
use std::collections::{HashMap, HashSet};

const EMBED_AUTHOR_ICON_URL: &str = "https://cdn.discordapp.com/avatars/512706205647372302/eb7a7f2de9a97006e8217b73ab5c7836.webp?size=128";
const EMBED_AUTHOR_NAME: &str = "Crossroads Bot";
const EMBED_THUMBNAIL: &str =
    "https://cdn.discordapp.com/icons/226398442082140160/03fe915815e9dbb6cdd18fe577fc6dd9.webp";
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

// Helpers
fn discord_timestamp(dt: &NaiveDateTime) -> String {
    format!("<t:{}:F>", dt.timestamp())
}

const GOOGLE_CALENDAR_TIME_FMT: &str = "%Y%m%dT%H%M%SZ";
fn google_calendar_link(training: &db::Training) -> String {
    let begin = training.date.format(GOOGLE_CALENDAR_TIME_FMT);
    let end = (training.date + Duration::hours(2)).format(GOOGLE_CALENDAR_TIME_FMT);
    format!(
        "https://calendar.google.com/calendar/event?action=TEMPLATE&dates={}/{}&text={}",
        begin,
        end,
        training.title.replace(" ", "%20")
    )
}

const TRAINING_TIME_FMT: &str = "%H:%M (UTC)";
// common embed fields
fn field_training_date(training: &db::Training) -> (String, String, bool) {
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

pub fn select_roles_embed(
    roles: &[db::Role],    // all roles
    sel: &HashSet<String>, // selected roles
) -> CreateEmbed {
    let pages = roles.chunks(10);
    let mut e = CreateEmbed::xdefault();
    for (i, p) in pages.enumerate() {
        let field_str = p
            .iter()
            .map(|r| {
                format!(
                    "`{}` | {} | {}",
                    if sel.contains(&r.repr) {
                        CHECK_EMOJI
                    } else {
                        RED_SQUARE_EMOJI
                    },
                    Mention::from(EmojiId::from(r.emoji as u64)),
                    r.title,
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        e.field(format!("Roles (Page {})", i + 1), field_str, true);
    }
    e
}

// Does not display roles
pub fn training_base_embed(training: &db::Training) -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.description(format!(
        "{} {}",
        match training.state {
            db::TrainingState::Created => CONSTRUCTION_SITE_EMOJI,
            db::TrainingState::Open => GREEN_CIRCLE_EMOJI,
            db::TrainingState::Closed => RED_CIRCLE_EMOJI,
            db::TrainingState::Started => RUNNING_EMOJI,
            db::TrainingState::Finished => CROSS_EMOJI,
        },
        training.title
    ));
    let (a, b, c) = field_training_date(training);
    e.field(a, b, c);
    e.field("**State**", &training.state, true);
    e.field("**Training Id**", &training.id, true);
    e
}

pub fn signupboard_embed(
    training: &db::Training,
    roles: &[db::Role],
    tier: &Option<(db::Tier, Vec<db::TierMapping>)>,
) -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    let title = format!(
        "{} {}",
        match training.state {
            db::TrainingState::Created => CONSTRUCTION_SITE_EMOJI,
            db::TrainingState::Open => GREEN_CIRCLE_EMOJI,
            db::TrainingState::Closed => RED_CIRCLE_EMOJI,
            db::TrainingState::Started => RUNNING_EMOJI,
            db::TrainingState::Finished => CROSS_EMOJI,
        },
        training.title
    );
    e.title(title);
    let (a, b, c) = field_training_date(training);
    e.field(a, b, c);
    training_embed_add_tier(&mut e, tier, true);
    e.field("**State**", &training.state, true);
    e.field("**Training Id**", &training.id, true);
    embed_add_roles(&mut e, roles, true, false);
    e
}

// adds the required tiers to the embed field. Should only be used in
// guild channels since pinging wont work in DM's
pub fn training_embed_add_tier(
    e: &mut CreateEmbed,
    t: &Option<(db::Tier, Vec<db::TierMapping>)>,
    inline: bool,
) {
    match t {
        None => {
            e.field("No Tier required", "Open for everyone", inline);
        }
        Some((tier, roles)) => {
            e.field(
                format!("Requires: {}", tier.name),
                roles
                    .iter()
                    .map(|r| Mention::from(RoleId::from(r.discord_role_id as u64)).to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
                inline,
            );
        }
    }
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

pub fn training_embed_add_board_footer(e: &mut CreateEmbed, ts: &db::TrainingState) {
    match ts {
        db::TrainingState::Open => {
            e.footer(|f| {
                f.text(format!(
                    "{}\n{}\n{}",
                    format!("{} to signup", CHECK_EMOJI),
                    format!("{} to edit your signup", MEMO_EMOJI),
                    format!("{} to remove your signup", CROSS_EMOJI)
                ))
            });
        }
        _ => {
            e.footer(|f| f.text("Not open for signup"));
        }
    }
}

fn internal_register_embed(e: &mut CreateEmbed) {
    e.description(
        "To register with the bot simply use the register command (_possible in DM's_) with your \
        Guild Wars 2 account name.\n\
        This is your in game account name which you can also find on your friends list. It \
        consists of your chosen in game name followed by a dot and 4 digits\n\
        _If your account name contains spaces wrap it in:_ `\"...\"`",
    );
    e.field("Usage:", "register AccountName.1234", false);
    e.field("Example:", "register Narturio.1234", false);
    e.footer(|f| {
        f.text(format!(
            "Note: if you use this command outside of DM's please prefix it with `{}`",
            GLOB_COMMAND_PREFIX
        ))
    });
}

pub fn not_registered_embed() -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.title("Not yet registered");
    internal_register_embed(&mut e);
    e
}

pub fn register_instructions_embed() -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.title("How to register");
    internal_register_embed(&mut e);
    e
}

pub fn not_signed_up_embed(training: &db::Training) -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.color((255, 0, 0));
    e.title(format!(
        "{} No signup found for: {} {}",
        DIZZY_EMOJI,
        training.title,
        training.date.date()
    ));
    e.description("To sign up use the button below or one of the following commands");
    e.field(
        "If you want to join this training use:",
        format!("`{}join {}`", GLOB_COMMAND_PREFIX, training.id),
        false,
    );
    e
}

pub fn already_signed_up_embed(training: &db::Training) -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.color((255, 0, 0));
    e.title(format!(
        "Already signed up for: {} {}",
        training.title,
        training.date.date()
    ));
    e.description(
        "To edit your signup or to sign out use the buttons below or one of the following commands",
    );
    e.field(
        "You are already signed up for training:",
        &training.title,
        false,
    );
    e.field(
        "If you want to edit this training use:",
        format!("`{}edit {}`", GLOB_COMMAND_PREFIX, training.id),
        false,
    );
    e.field(
        "If you want to leave this training use:",
        format!("`{}leave {}`", GLOB_COMMAND_PREFIX, training.id),
        false,
    );
    e
}

pub fn signup_list_embed(
    signups: &[(db::Signup, db::Training)],
    roles: &HashMap<i32, Vec<db::Role>>,
) -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
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
                "`Date       `\n<t:{}:D>\n\
                `Time       `\n<t:{}:t>\n\
                `Training Id`\n{}\n\
                `Roles      `\n{}\n",
                t.date.timestamp(),
                t.date.timestamp(),
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
    e
}

pub fn signup_add_comment_embed(training: &db::Training) -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.title(format!("Add comment for training: {}", training.title));
    e.description("Add a comment to your sign up by replying to this message");
    //e.field(
    //    "Comment",
    //    "__to add a comment simply reply to this message__",
    //    false,
    //);
    e
}

pub fn success_signed_up(training: &db::Training) -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.xstyle();
    e.color((0, 255, 0));
    e.title(format!(
        "Successfully signed up for: {} {}",
        training.title,
        training.date.date()
    ));
    e.description(
        "To edit your signup or to sign out use the buttons below or one of the following commands",
    );
    e.field(
        "To edit your sign up:",
        format!("`{}edit {}`", GLOB_COMMAND_PREFIX, training.id),
        false,
    );
    e.field(
        "To remove your sign up:",
        format!("`{}leave {}`", GLOB_COMMAND_PREFIX, training.id),
        false,
    );
    e.field(
        "To list all your current sign ups:",
        format!("`{}list`", GLOB_COMMAND_PREFIX),
        false,
    );
    e
}

pub fn signed_out_embed(training: &db::Training) -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.xstyle();
    e.color((0, 255, 0));
    e.title(format!(
        "Successfully signed out from {} {}",
        training.title,
        training.date.date()
    ));
    e.description("To sign up again use the button below or one of the following commands");
    e.field(
        "To join the training:",
        format!("`{}join {}`", GLOB_COMMAND_PREFIX, training.id),
        false,
    );
    e.field(
        "To list all your current sign ups:",
        format!("`{}list`", GLOB_COMMAND_PREFIX),
        false,
    );
    e
}

pub fn welcome_post_embed() -> CreateEmbed {
    let mut e = CreateEmbed::xdefault();
    e.title("Greetings. Beep boop...");
    e.description(
        "Hello, You can use me to sign up for various trainings. Please make sure \
                   that I can send you DM's by allowing Direct Messages from server members",
    );
    e.field(
        "Register",
        format!(
            "To use this bot please register first with your Guild Wars 2 account name. \
        To do so please use the `{0}register` command. For more information on how to use this \
        command use the help command like so: `{0}help register` or click the button below \
        and I will send you the instructions to your DM's.\n_If you want to update your gw2 \
        account name just register again_",
            GLOB_COMMAND_PREFIX
        ),
        false,
    );
    e.field(
        "Sign up for a training",
        "To sign up for a training you can browse the *SIGNUPBOARD* category and use the buttons \
        on the corresponding messages. You can only sign up for a training that is marked as open \
        and you have the required tier for",
        false,
    );
    e.field(
        "List my signups",
        format!(
            "To list your current signups you can use the `{}list` command \
        or press the button below",
            GLOB_COMMAND_PREFIX
        ),
        false,
    );
    e
}
