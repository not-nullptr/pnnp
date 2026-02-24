use super::{Data, Error};
use crate::{
    bot::progress::{self, ProgressTaskMessage},
    config::Config,
    pipeline::Pipeline,
    track_or_album::TrackOrAlbum,
};
use monochrome::{Monochrome, album::Album};
use poise::serenity_prelude::{
    self as serenity, ComponentInteractionDataKind, CreateInteractionResponseFollowup,
};
use std::sync::Arc;
use tokio::sync::mpsc;

pub async fn handle_interaction(
    ctx: &serenity::Context,
    interaction: &serenity::Interaction,
    data: &Data,
) -> Result<(), Error> {
    match interaction {
        serenity::Interaction::Component(i) => {
            if let Some(m) = i.message.interaction_metadata.as_deref() {
                if !is_from(m, i.user.id) {
                    i.create_followup(
                        &ctx.http,
                        CreateInteractionResponseFollowup::new()
                            .content("sorry, you can't interact with this")
                            .ephemeral(true),
                    )
                    .await?;
                    return Ok(());
                }
            }
            match i.data.custom_id.as_str() {
                "album_select" | "track_select" => {
                    tracing::info!("handling music select interaction");
                    let ComponentInteractionDataKind::StringSelect { values } = &i.data.kind else {
                        tracing::error!("unexpected interaction data kind");
                        return Ok(());
                    };

                    let Some(music_id) = values.first().map(|s| s.parse::<u64>().ok()).flatten()
                    else {
                        tracing::error!("no music id found in interaction data");
                        // i.create_response(&ctx.http, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content("no album id found in interaction data... this shouldn't happen!"))).await?;
                        i.create_followup(
                            &ctx.http,
                            CreateInteractionResponseFollowup::new().content(
                                "no music id found in interaction data... this shouldn't happen!",
                            ),
                        )
                        .await?;
                        return Ok(());
                    };

                    tracing::info!(%music_id, "selected music id");

                    i.defer(&ctx.http).await?;

                    tracing::info!(kind = %i.data.custom_id, "deferring interaction response");

                    let album = data.client.album(music_id).await;

                    let album = match album {
                        Ok(music) => music,
                        Err(e) => {
                            tracing::error!(error = %e, "failed to fetch music for selected id");
                            // i.edit_response(
                            //     &ctx.http,
                            //     EditInteractionResponse::new()
                            //         .content("failed to fetch music for selected id"),
                            // )
                            // .await?;
                            i.create_followup(
                                &ctx.http,
                                CreateInteractionResponseFollowup::new()
                                    .content(format!("failed to fetch music for selected id: {e}")),
                            )
                            .await?;
                            return Ok(());
                        }
                    };

                    if i.data.custom_id == "track_select" && album.kind != "SINGLE" {
                        tracing::error!(kind = %album.kind, "selected track is not a single");
                        i.create_followup(
                            &ctx.http,
                            CreateInteractionResponseFollowup::new().content(
                                format!("the selected track is not a single, please use /album to download the whole album ({} - {})", album.artist.name, album.title),
                            ),
                        )
                        .await?;

                        return Ok(());
                    }

                    let msgs = progress::done_msgs(&album);

                    i
                        .create_followup(
                            &ctx.http,
                            CreateInteractionResponseFollowup::new().content(
                                format!(
                                    "your download (**{} - {}**) will start soon! check <#{}> for progress updates",
                                    album.artist.name, album.title,
                                    data.config.bot.progress_channel
                                )
                            )
                        )
                        .await?;

                    if let Err(e) =
                        handle_download(&data.client, data.config.clone(), album, data).await
                    {
                        tracing::error!(error = %e, "failed to download album");

                        for msg in msgs {
                            data.progress_tx.send(msg)?;
                        }

                        i.create_followup(
                            &ctx.http,
                            CreateInteractionResponseFollowup::new()
                                .content(format!("failed to download: {e}")),
                        )
                        .await?;
                    }
                }

                _ => {}
            }
        }

        _ => {}
    }

    Ok(())
}

async fn handle_download(
    client: &Monochrome,
    config: Arc<Config>,
    album: Album,
    data: &Data,
) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();

    // data.progress_tx
    //     .send(ProgressTaskMessage::DiscoverAlbum(id, music.clone()))?;

    data.progress_tx.send(ProgressTaskMessage::DiscoverAlbum(
        album.id.into(),
        album.clone(),
    ))?;

    let msgs = progress::done_msgs(&album);

    let pipeline = Pipeline::new(
        client.clone(),
        album,
        tx,
        data.track_semaphore.clone(),
        data.chunk_semaphore.clone(),
        config,
    );

    let handle = tokio::spawn(pipeline.begin());

    while let Some(update) = rx.recv().await {
        data.progress_tx
            .send(ProgressTaskMessage::Progress(update))?;
    }

    for handle in handle.await? {
        handle.await??;
    }

    for msg in msgs {
        data.progress_tx.send(msg)?;
    }

    Ok(())
}

fn is_from(metadata: &serenity::MessageInteractionMetadata, user_id: serenity::UserId) -> bool {
    let original = match metadata {
        serenity::MessageInteractionMetadata::Command(c) => &c.user,
        serenity::MessageInteractionMetadata::Component(c) => &c.user,
        serenity::MessageInteractionMetadata::ModalSubmit(m) => &m.user,
        _ => return false,
    };

    original.id == user_id
}
