use crate::track_or_album::TrackOrAlbum;

use super::{Context, Error};
use poise::CreateReply;
use poise::serenity_prelude::CreateActionRow;
use poise::serenity_prelude::CreateSelectMenu;
use poise::serenity_prelude::CreateSelectMenuKind;
use poise::serenity_prelude::CreateSelectMenuOption;
use unicode_ellipsis::truncate_str;

pub enum SearchKind {
    Single,
    Album,
}

impl SearchKind {
    pub fn as_id(&self) -> &'static str {
        match self {
            SearchKind::Single => "track_select",
            SearchKind::Album => "album_select",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            SearchKind::Single => "track",
            SearchKind::Album => "album",
        }
    }
}

pub async fn search_command(ctx: Context<'_>, query: &str, kind: SearchKind) -> Result<(), Error> {
    let client = &ctx.data().client;
    ctx.defer().await?;

    let music: Vec<TrackOrAlbum> = match kind {
        SearchKind::Single => client
            .search_tracks(query)
            .await?
            .into_iter()
            .map(|t| TrackOrAlbum::Track(t))
            .collect(),

        SearchKind::Album => client
            .search_albums(query)
            .await?
            .into_iter()
            .map(|a| TrackOrAlbum::Album(a.into()))
            .collect(),
    };

    if music.is_empty() {
        ctx.say(format!("no {}s found", kind.name())).await?;
        return Ok(());
    }

    let menu = CreateSelectMenu::new(
        kind.as_id(),
        CreateSelectMenuKind::String {
            options: music
                .iter()
                .map(|music| {
                    CreateSelectMenuOption::new(
                        truncate_str(
                            &format!(
                                "{} - {}",
                                music
                                    .artists()
                                    .into_iter()
                                    .map(|a| a.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                music.title()
                            ),
                            100,
                        ),
                        music.album_id().to_string(),
                    )
                })
                .collect(),
        },
    )
    .placeholder(format!(
        "select a{} {}...",
        vowel_helper(kind.name()),
        kind.name()
    ))
    .max_values(1)
    .min_values(1);

    ctx.send(
        CreateReply::default()
            .content(format!(
                "found {} {}{}! please select one to be downloaded.",
                music.len(),
                kind.name(),
                if music.len() == 1 { "" } else { "s" }
            ))
            .components(vec![CreateActionRow::SelectMenu(menu)]),
    )
    .await?;

    Ok(())
}

fn vowel_helper(s: &str) -> &str {
    match s.chars().next() {
        Some(c) if "aeiouAEIOU".contains(c) => "n",
        _ => "",
    }
}
