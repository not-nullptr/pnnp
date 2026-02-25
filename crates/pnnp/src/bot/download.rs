use super::{Context, Error};
use crate::bot::search::{self, SearchKind};

/// searches for and downloads singles and albums
#[poise::command(slash_command, prefix_command)]
pub async fn download(
    ctx: Context<'_>,
    #[description = "album or single name to search for"] query: String,
    #[description = "what to search for"] kind: SearchKind,
) -> Result<(), Error> {
    search::search_command(ctx, &query, kind).await
}
