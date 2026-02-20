use super::{Data, Error};
use crate::{bot::progress::ProgressTaskMessage, config::Config, pipeline::Pipeline};
use monochrome::{Monochrome, album::Album};
use poise::serenity_prelude::{
    self as serenity, ComponentInteractionDataKind, CreateInteractionResponse,
    CreateInteractionResponseMessage, EditInteractionResponse,
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
                    i.create_response(
                        &ctx.http,
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .content("sorry, you can't interact with this")
                                .ephemeral(true),
                        ),
                    )
                    .await?;
                    return Ok(());
                }
            }
            match i.data.custom_id.as_str() {
                "album_select" => {
                    tracing::info!("handling album select interaction");
                    let ComponentInteractionDataKind::StringSelect { values } = &i.data.kind else {
                        tracing::error!("unexpected interaction data kind");
                        return Ok(());
                    };

                    let Some(album_id) = values.first().map(|s| s.parse::<u64>().ok()).flatten()
                    else {
                        tracing::error!("no album id found in interaction data");
                        i.create_response(&ctx.http, CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content("no album id found in interaction data... this shouldn't happen!"))).await?;
                        return Ok(());
                    };

                    tracing::info!(%album_id, "selected album id");

                    // let album = client.album(album_id).await?;

                    let album = match data.client.album(album_id).await {
                        Ok(album) => album,
                        Err(e) => {
                            tracing::error!(error = %e, "failed to fetch album for selected album id");
                            i.create_response(
                                &ctx.http,
                                CreateInteractionResponse::Message(
                                    CreateInteractionResponseMessage::new()
                                        .content("failed to fetch album for selected album id"),
                                ),
                            )
                            .await?;
                            return Ok(());
                        }
                    };

                    i
                        .create_response(
                            &ctx.http,
                            CreateInteractionResponse::Message(CreateInteractionResponseMessage::new().content(
                                format!(
                                    "your download (**{} - {}**) will start soon! check <#{}> for progress updates",
                                    album.artist.name, album.title,
                                    data.config.bot.progress_channel
                                )
                            )
                        ))
                        .await?;

                    if let Err(e) =
                        handle_download(&data.client, data.config.clone(), album, data).await
                    {
                        tracing::error!(error = %e, "failed to download album");

                        data.progress_tx
                            .send(ProgressTaskMessage::Done(album_id.into()))
                            .ok();

                        i.edit_response(
                            &ctx.http,
                            EditInteractionResponse::new()
                                .content(format!("failed to download album: {e}")),
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

    let id = album.id;

    data.progress_tx
        .send(ProgressTaskMessage::DiscoverAlbum(id, album.clone()))?;

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

    data.progress_tx.send(ProgressTaskMessage::Done(id))?;

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
