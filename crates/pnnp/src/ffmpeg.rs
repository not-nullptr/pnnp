use futures::{Stream, StreamExt};
use monochrome::{id::TrackId, track::TrackResult};
use std::{borrow::Cow, process::Stdio};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
    sync::mpsc,
};

use crate::pipeline::ProgressUpdate;

#[derive(Debug, Error)]
pub enum TranscodeError {
    #[error("failed to open ffmpeg stdin")]
    StdinOpen,

    #[error("failed to write to ffmpeg stdin")]
    StdinWrite(#[from] std::io::Error),

    #[error("ffmpeg exited with non-zero status: {0}")]
    NonZeroExit(std::process::ExitStatus),
}

#[derive(Debug, Clone, Default)]
pub struct Metadata<'a> {
    pub album: Option<&'a str>,
    pub album_artist: Option<&'a str>,
    pub artist: Option<Cow<'a, str>>,
    pub title: Option<&'a str>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
}

impl<'a> From<&'a TrackResult> for Metadata<'a> {
    fn from(track: &'a TrackResult) -> Self {
        Self {
            album: Some(&track.album.title),
            album_artist: Some(
                track
                    .artists
                    .iter()
                    .find(|a| a.kind == "MAIN")
                    .unwrap_or(&track.artist)
                    .name
                    .as_str(),
            ),
            artist: Some(
                track
                    .artists
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
                    .into(),
            ),
            title: Some(&track.title),
            track_number: Some(track.track_number),
            disc_number: Some(track.volume_number),
        }
    }
}

pub struct Transcoder<S> {
    child: Child,
    stream: S,
    track_id: TrackId,
}

impl<S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin> Transcoder<S> {
    pub fn new(
        stream: S,
        metadata: Metadata,
        track_id: TrackId,
        output: &str,
    ) -> Result<Self, std::io::Error> {
        let mut args = vec![
            "-i",
            "pipe:0",
            "-vn",
            "-c:a",
            "libopus",
            "-b:a",
            "192k",
            "-vbr",
            "on",
            "-compression_level",
            "10",
            "-nostdin",
            "-y",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

        if let Some(album) = metadata.album {
            args.push("-metadata".to_string());
            args.push(format!("album={album}"));
        }

        if let Some(album_artist) = metadata.album_artist {
            args.push("-metadata".to_string());
            args.push(format!("album_artist={album_artist}"));
        }

        if let Some(artist) = metadata.artist {
            args.push("-metadata".to_string());
            args.push(format!("artist={artist}"));
        }

        if let Some(title) = metadata.title {
            args.push("-metadata".to_string());
            args.push(format!("title={title}"));
        }

        if let Some(track_number) = metadata.track_number {
            args.push("-metadata".to_string());
            args.push(format!("track={track_number}"));
        }

        if let Some(disc_number) = metadata.disc_number {
            args.push("-metadata".to_string());
            args.push(format!("disc={disc_number}"));
        }

        args.push(output.to_string());

        let child = Command::new("ffmpeg")
            .args(&args)
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(Self {
            child,
            stream,
            track_id,
        })
    }

    pub async fn run(mut self, tx: &mpsc::Sender<ProgressUpdate>) -> Result<(), TranscodeError> {
        tracing::debug!("starting transcoder run...");

        tx.send(ProgressUpdate::Downloading {
            track_id: self.track_id,
            bytes_downloaded: 0,
        })
        .await
        .ok();

        let mut stdin = self.child.stdin.take().ok_or(TranscodeError::StdinOpen)?;

        let mut downloaded = 0;

        while let Some(chunk) = self.stream.next().await {
            let chunk = chunk.map_err(|_| TranscodeError::StdinOpen)?;
            stdin.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            tx.send(ProgressUpdate::Downloading {
                track_id: self.track_id,
                bytes_downloaded: downloaded,
            })
            .await
            .ok();
        }

        stdin.flush().await?;

        tx.send(ProgressUpdate::Transcoding {
            track_id: self.track_id,
        })
        .await
        .ok();

        drop(stdin); // idk why shutdown() doesn't work but this does so
        tracing::debug!("finished writing to ffmpeg stdin, waiting for ffmpeg to exit...");
        let status = self.child.wait().await?;
        if !status.success() {
            tracing::error!(%status, "ffmpeg exited with non-zero status");
            return Err(TranscodeError::NonZeroExit(status));
        }

        tx.send(ProgressUpdate::Finished {
            track_id: self.track_id,
        })
        .await
        .ok();

        Ok(())
    }
}
