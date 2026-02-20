use std::{collections::HashMap, sync::Arc};

use crate::{
    config::Config,
    pipeline::{Pipeline, ProgressUpdate},
};

use super::{Data, Error};
use monochrome::{Monochrome, id::TrackId};
use poise::serenity_prelude::{
    self as serenity, ComponentInteractionDataKind, CreateInteractionResponse,
    CreateInteractionResponseFollowup, CreateInteractionResponseMessage, EditMessage,
};
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
                    i.defer(&ctx.http).await?;

                    let mut msg = i.create_followup(
                        &ctx.http,
                        CreateInteractionResponseFollowup::new()
                            .content("beginning download... this may take a while! todo: progress updates :)"),
                    )
                    .await?;

                    if let Err(e) = handle_download(
                        &data.client,
                        data.config.clone(),
                        album_id,
                        &mut msg,
                        ctx.http.as_ref(),
                    )
                    .await
                    {
                        tracing::error!(error = %e, "failed to download album");
                        msg.edit(
                            &ctx.http,
                            EditMessage::new().content(format!("failed to download album: {e}")),
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

struct Progress {
    track_name: String,
    track_sort: (u32, u32),
    track_progress: TrackProgress,
}

enum TrackProgress {
    Waiting,
    Downloading(u64),
    Transcoding,
    Finished,
}

async fn handle_download(
    client: &Monochrome,
    config: Arc<Config>,
    album_id: u64,
    msg: &mut serenity::Message,
    http: &serenity::http::Http,
) -> anyhow::Result<()> {
    let album = client.album(album_id).await?;

    let mut tracks = album
        .tracks
        .iter()
        .map(|t| {
            (
                t.id,
                Progress {
                    track_name: t.title.clone(),
                    track_sort: (t.volume_number, t.track_number),
                    track_progress: TrackProgress::Waiting,
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let (tx, mut rx) = mpsc::channel(128);
    let pipeline = Pipeline::new(client.clone(), album, config, tx);

    let handles = pipeline.begin().await;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));

    let create_str = |tracks: &HashMap<TrackId, Progress>| {
        let mut sorted = tracks.values().collect::<Vec<_>>();
        sorted.sort_by_key(|t| t.track_sort);

        sorted
            .into_iter()
            .map(|t| {
                let progress_str = match &t.track_progress {
                    TrackProgress::Waiting => "waiting".to_string(),
                    TrackProgress::Downloading(p) => {
                        format!("downloading... {}", bytesize::ByteSize(*p))
                    }
                    TrackProgress::Transcoding => "transcoding...".to_string(),
                    TrackProgress::Finished => "finished!".to_string(),
                };

                format!("**{}** - {}", t.track_name, progress_str)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    loop {
        tokio::select! {
            update = rx.recv() => {
                let Some(update) = update else {
                    break;
                };

                match update {
                    ProgressUpdate::Downloading { track_id, bytes_downloaded } => {
                        if let Some(track) = tracks.get_mut(&track_id) {
                            track.track_progress = TrackProgress::Downloading(bytes_downloaded);
                        }
                    }

                    ProgressUpdate::Transcoding { track_id } => {
                        if let Some(track) = tracks.get_mut(&track_id) {
                            track.track_progress = TrackProgress::Transcoding;
                        }
                    }

                    ProgressUpdate::Finished { track_id } => {
                        if let Some(track) = tracks.get_mut(&track_id) {
                            track.track_progress = TrackProgress::Finished;
                        }
                    }
                }
            }

            _ = interval.tick() => {

                let curr_msg = create_str(&tracks);

                msg.edit(
                    http,
                    EditMessage::new().content(format!("downloading album... this may take a while!\n\n{curr_msg}")),
                )
                .await?;
            }
        }
    }

    for handle in handles {
        handle.await??;
    }

    // set all to complete just in case
    for track in tracks.values_mut() {
        track.track_progress = TrackProgress::Finished;
    }

    let curr_msg = create_str(&tracks);

    msg.edit(
        http,
        EditMessage::new().content(format!(
            "**download complete!** ask sophie or maddie to refresh navidrome if necessary ^_^\n\n{curr_msg}"
        )),
    )
    .await?;

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
