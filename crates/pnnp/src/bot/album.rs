use super::Context;
use super::Error;
use poise::CreateReply;
use poise::serenity_prelude::CreateActionRow;
use poise::serenity_prelude::CreateSelectMenu;
use poise::serenity_prelude::CreateSelectMenuKind;
use poise::serenity_prelude::CreateSelectMenuOption;
use unicode_ellipsis::truncate_str;

/// searches for albums and gives you the option to download them
#[poise::command(slash_command, prefix_command)]
pub async fn album(
    ctx: Context<'_>,
    #[description = "album name to search for"] query: String,
) -> Result<(), Error> {
    let client = &ctx.data().client;
    ctx.defer().await?;
    let albums = client.search_albums(query).await?;
    if albums.is_empty() {
        ctx.say("no albums found").await?;
        return Ok(());
    }
    // let options = albums
    //     .iter()
    //     .map(|album| {

    //     })
    //     .collect();

    let menu = CreateSelectMenu::new(
        "album_select",
        CreateSelectMenuKind::String {
            options: albums
                .iter()
                .map(|album| {
                    CreateSelectMenuOption::new(
                        truncate_str(
                            &format!(
                                "{} - {}",
                                album
                                    .artists
                                    .iter()
                                    .map(|a| a.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                album.title
                            ),
                            100,
                        ),
                        album.id.to_string(),
                    )
                })
                .collect(),
        },
    )
    .placeholder("select an album...")
    .max_values(1)
    .min_values(1);

    ctx.send(
        CreateReply::default()
            .content(format!(
                "found {} album{}! please select one to be downloaded.",
                albums.len(),
                if albums.len() == 1 { "" } else { "s" }
            ))
            .components(vec![CreateActionRow::SelectMenu(menu)]),
    )
    .await?;

    Ok(())
}
