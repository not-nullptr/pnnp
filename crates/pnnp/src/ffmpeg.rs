use futures::{Stream, StreamExt};
use monochrome::{
    artist::Artist,
    id::{AlbumId, TrackId},
    track::TrackResult,
};
use std::process::Stdio;
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    process::{Child, Command},
    sync::mpsc,
};

use crate::pipeline::{ProgressState, ProgressUpdate};

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
    pub artists: Vec<&'a str>,
    pub title: Option<&'a str>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<u32>,
}

impl<'a> From<(&'a TrackResult, &'a Artist, u32)> for Metadata<'a> {
    fn from((track, artist, year): (&'a TrackResult, &'a Artist, u32)) -> Self {
        Self {
            album: Some(&track.album.title),
            album_artist: Some(&artist.name),
            artists: track
                .artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            title: Some(&track.title),
            track_number: Some(track.track_number),
            disc_number: Some(track.volume_number),
            year: Some(year),
        }
    }
}

pub struct Transcoder<S> {
    child: Child,
    artists: Vec<String>,
    stream: S,
    track_id: TrackId,
    album_id: AlbumId,
    output: String,
}

impl<S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin> Transcoder<S> {
    pub fn new(
        stream: S,
        metadata: Metadata,
        track_id: TrackId,
        album_id: AlbumId,
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

        if metadata.artists.len() == 1 {
            args.push("-metadata".to_string());
            args.push(format!("artist={}", metadata.artists[0]));
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

        if let Some(year) = metadata.year {
            args.push("-metadata".to_string());
            args.push(format!("year={year}"));
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
            album_id,
            artists: metadata.artists.iter().map(|s| s.to_string()).collect(),
            output: output.to_string(),
        })
    }

    pub async fn run(
        mut self,
        tx: &mpsc::UnboundedSender<ProgressUpdate>,
    ) -> Result<(), TranscodeError> {
        if let Some(stderr) = self.child.stderr.take() {
            let mut reader = tokio::io::BufReader::new(stderr).lines();

            tokio::spawn(async move {
                while let Ok(Some(line)) = reader.next_line().await {
                    tracing::debug!("ffmpeg: {}", line);
                }
            });
        }

        tx.send(ProgressUpdate {
            album_id: self.album_id,
            track_id: self.track_id,
            state: ProgressState::Downloading(0),
        })
        .ok();

        let mut stdin = self.child.stdin.take().ok_or(TranscodeError::StdinOpen)?;

        let mut downloaded = 0;

        while let Some(chunk) = self.stream.next().await {
            let chunk = chunk.map_err(|_| TranscodeError::StdinOpen)?;
            stdin.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            tx.send(ProgressUpdate {
                album_id: self.album_id,
                track_id: self.track_id,
                state: ProgressState::Downloading(downloaded),
            })
            .ok();
        }

        tracing::debug!(
            "finished downloading track {}, writing to ffmpeg stdin...",
            self.track_id
        );

        stdin.flush().await?;

        tx.send(ProgressUpdate {
            album_id: self.album_id,
            track_id: self.track_id,
            state: ProgressState::Transcoding,
        })
        .ok();

        stdin.shutdown().await?;
        drop(stdin); // idk why shutdown() doesn't work but this does so

        tracing::debug!("finished writing to ffmpeg stdin, waiting for ffmpeg to exit...");
        let status = self.child.wait().await?;
        if !status.success() {
            tracing::error!(%status, "ffmpeg exited with non-zero status");
            return Err(TranscodeError::NonZeroExit(status));
        }

        // we also need to run opustags for multi artist
        if self.artists.len() > 1 {
            let mut args = vec!["-i"].into_iter().map(String::from).collect::<Vec<_>>();

            for artist in self.artists {
                args.push("-a".to_string());
                args.push(format!("ARTISTS={artist}"));
            }

            args.push(self.output.clone());

            let status = Command::new("opustags").args(&args).status().await?;
            if !status.success() {
                tracing::error!(%status, "opustags exited with non-zero status");
                return Err(TranscodeError::NonZeroExit(status));
            }
        }

        tx.send(ProgressUpdate {
            album_id: self.album_id,
            track_id: self.track_id,
            state: ProgressState::Finished,
        })
        .ok();

        Ok(())
    }
}
