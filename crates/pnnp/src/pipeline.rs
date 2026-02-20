use crate::{
    config::Config,
    ffmpeg::{Metadata, TranscodeError, Transcoder},
};
use chrono::Datelike;
use futures::StreamExt;
use monochrome::{
    Monochrome, MonochromeError,
    album::Album,
    id::{AlbumId, TrackId},
};
use std::{path::PathBuf, sync::Arc};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt,
    sync::{Semaphore, mpsc},
    task::JoinHandle,
};
use tokio_retry::{
    Retry,
    strategy::{ExponentialBackoff, jitter},
};

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error(transparent)]
    Transcode(#[from] TranscodeError),

    #[error(transparent)]
    Monochrome(#[from] MonochromeError),

    #[error("failed to acquire semaphore permit")]
    Semaphore(#[from] tokio::sync::AcquireError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),

    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

pub struct Pipeline {
    client: Monochrome,
    album: Album,
    tx: mpsc::UnboundedSender<ProgressUpdate>,
    track_semaphore: Arc<Semaphore>,
    chunk_semaphore: Arc<Semaphore>,
    config: Arc<Config>,
}

impl Pipeline {
    pub fn new(
        client: Monochrome,
        album: Album,
        tx: mpsc::UnboundedSender<ProgressUpdate>,
        track_semaphore: Arc<Semaphore>,
        chunk_semaphore: Arc<Semaphore>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            client,
            album,
            tx,
            track_semaphore,
            chunk_semaphore,
            config,
        }
    }

    pub async fn begin(self) -> Vec<JoinHandle<Result<(), PipelineError>>> {
        let track_semaphore = self.track_semaphore;
        let chunk_semaphore = self.chunk_semaphore;

        let mut handles = Vec::new();
        let multidisc = self.album.tracks.iter().any(|t| t.volume_number > 1);
        let album_folder = PathBuf::from(&self.config.output.dir)
            .join(&path_compat(&self.album.artist.name))
            .join(&path_compat(&format!(
                "[{}] {}",
                self.album.release_date.year(),
                self.album.title
            )));

        if let Err(e) = tokio::fs::create_dir_all(&album_folder).await {
            tracing::error!(
                album_folder = %album_folder.display(),
                error = %e,
                "failed to create album folder"
            );
            return vec![tokio::spawn(async { Err(PipelineError::Io(e)) })];
        }

        let year = self.album.release_date.year() as u32;

        for track in self.album.tracks {
            let semaphore = track_semaphore.clone();
            let client = self.client.clone();
            let path = album_folder.join(&path_compat(&if multidisc {
                format!(
                    "{}.{:02}. {}.opus",
                    track.volume_number, track.track_number, track.title
                )
            } else {
                format!("{:02}. {}.opus", track.track_number, track.title)
            }));

            let tx = self.tx.clone();

            if let Some(parent) = path.parent() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    tracing::error!("failed to create directories for {}: {e}", path.display());
                    continue;
                }
            }

            if tokio::fs::metadata(&path).await.is_ok() {
                tracing::info!("skipping {} because it already exists", path.display());
                continue;
            }

            let artist = self.album.artist.clone();
            let chunk_semaphore = chunk_semaphore.clone();

            let permit = semaphore.clone().acquire_owned().await.unwrap();

            let handle: JoinHandle<Result<(), PipelineError>> = tokio::spawn(async move {
                let retry_strategy = ExponentialBackoff::from_millis(1000).map(jitter).take(5);
                let _permit = permit;

                Retry::spawn(retry_strategy, || async {
                    let path = path.to_string_lossy();
                    let dl_info = client.track(track.id).await?;
                    let stream = client
                        .download_track(&dl_info, chunk_semaphore.clone())
                        .await?;
                    let transcoder = Transcoder::new(
                        stream,
                        Metadata::from((&track, &artist, year)),
                        track.id,
                        self.album.id,
                        &path,
                    )?;
                    transcoder.run(&tx).await?;
                    Ok(())
                })
                .await
            });

            handles.push(handle);
        }

        {
            let client = self.client.clone();
            let title = self.album.title.clone();
            let album_art_handle: JoinHandle<Result<(), PipelineError>> =
                tokio::spawn(async move {
                    let retry_strategy = ExponentialBackoff::from_millis(1000).map(jitter).take(5);
                    Retry::spawn(retry_strategy, || async {
                        let path = album_folder.join("cover.jpg").to_string_lossy().to_string();
                        if tokio::fs::metadata(&path).await.is_ok() {
                            tracing::info!("skipping album art because it already exists");
                            return Ok(());
                        }

                        tracing::info!(album = %title, "downloading album art...");
                        let album = client.album(self.album.id).await?;
                        let mut stream = client.album_art(&album).await?;
                        let mut file = tokio::fs::File::create(&path).await?;
                        while let Some(chunk) = stream.next().await {
                            let chunk = chunk?;
                            file.write_all(&chunk).await?;
                        }

                        tracing::info!(album = %title, "finished downloading album art");

                        Ok(())
                    })
                    .await
                });

            handles.push(album_art_handle);
        }

        handles
    }
}

fn path_compat(s: &str) -> String {
    s.replace("/", "_")
        .replace("\\", "_")
        .replace(":", "_")
        .replace("*", "_")
        .replace("?", "_")
        .replace("\"", "_")
        .replace("<", "_")
        .replace(">", "_")
        .replace("|", "_")
}

// pub enum ProgressUpdate {
//     Downloading {
//         album_id: AlbumId,
//         track_id: TrackId,
//         bytes_downloaded: u64,
//     },
//     Transcoding {
//         album_id: AlbumId,
//         track_id: TrackId,
//     },
//     Finished {
//         album_id: AlbumId,
//         track_id: TrackId,
//     },
// }

pub struct ProgressUpdate {
    pub album_id: AlbumId,
    pub track_id: TrackId,
    pub state: ProgressState,
}

pub enum ProgressState {
    Downloading(u64),
    Transcoding,
    Finished,
}
