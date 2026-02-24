use super::Context;
use super::Error;
use crate::bot::search;
use crate::bot::search::SearchKind;

/// searches for albums and gives you the option to download them
#[poise::command(slash_command, prefix_command)]
pub async fn album(
    ctx: Context<'_>,
    #[description = "album name to search for"] query: String,
) -> Result<(), Error> {
    search::search_command(ctx, &query, SearchKind::Album).await
}
