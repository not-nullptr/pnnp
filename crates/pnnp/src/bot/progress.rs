use bytesize::ByteSize;
use chrono::{Datelike, NaiveDate};
use monochrome::{
    album::Album,
    id::{AlbumId, TrackId},
};
use poise::serenity_prelude::{self as serenity, CreateMessage, EditMessage, Message};
use std::{collections::HashMap, sync::Arc};
use std::{
    fmt::Write,
    time::{Duration, Instant},
};
use submarine::auth::AuthBuilder;
use tokio::sync::mpsc;

use crate::{
    config::Config,
    pipeline::{ProgressState, ProgressUpdate},
};

pub struct ProgressTask {
    rx: mpsc::UnboundedReceiver<ProgressTaskMessage>,
    http: Arc<serenity::Http>,
    channel: serenity::ChannelId,
    albums: HashMap<AlbumId, AlbumProgress>,
    count: usize,
    submarine: Option<submarine::Client>,
}

pub enum ProgressTaskMessage {
    DiscoverAlbum(AlbumId, Album),
    Progress(ProgressUpdate),
    Done(AlbumId),
}

struct AlbumProgress {
    sort: usize,
    tracks: HashMap<TrackId, TrackProgress>,
    title: String,
    artist: String,
    release_date: NaiveDate,
}

struct TrackProgress {
    // name: String,
    sort: (u32, u32),
    state: Option<ProgressState>,
    last_known_bytes: u64,
}

impl ProgressTask {
    pub fn new(
        rx: mpsc::UnboundedReceiver<ProgressTaskMessage>,
        http: Arc<serenity::Http>,
        channel: serenity::ChannelId,
        config: &Config,
    ) -> Self {
        Self {
            rx,
            http,
            channel,
            albums: HashMap::new(),
            count: 0,
            submarine: config.navidrome.as_ref().map(|nav| {
                submarine::Client::new(
                    &nav.url,
                    AuthBuilder::new(&nav.username, "1.16.1")
                        .client_name("pnnp auto-refresh")
                        .hashed(&nav.password),
                )
            }),
        }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mut discord_message = self
            .channel
            .send_message(&self.http, CreateMessage::new().content(self.msg_trimmed()))
            .await?;

        let mut last_edit = Instant::now() - Duration::from_secs(1);
        let mut pending_edit = false;

        loop {
            let timeout = if pending_edit {
                let elapsed = last_edit.elapsed();
                if elapsed >= Duration::from_secs(1) {
                    Duration::ZERO
                } else {
                    Duration::from_secs(1) - elapsed
                }
            } else {
                Duration::MAX
            };

            tokio::select! {
                msg = self.rx.recv() => {
                    let Some(msg) = msg else { break };
                    match msg {
                        ProgressTaskMessage::DiscoverAlbum(id, album) => {
                            self.albums.insert(
                                id,
                                AlbumProgress {
                                    tracks: album
                                        .tracks
                                        .into_iter()
                                        .map(|t| {
                                            (
                                                t.id,
                                                TrackProgress {
                                                    // name: t.title,
                                                    sort: (t.volume_number, t.track_number),
                                                    state: None,
                                                    last_known_bytes: 0,
                                                },
                                            )
                                        })
                                        .collect(),
                                    title: album.title,
                                    release_date: album.release_date,
                                    artist: album.artist.name,
                                    sort: self.count,
                                },
                            );
                            self.count = self.count.wrapping_add(1);
                        }
                        ProgressTaskMessage::Progress(update) => {
                            let Some(album) = self.albums.get_mut(&update.album_id) else {
                                continue;
                            };
                            let Some(track) = album.tracks.get_mut(&update.track_id) else {
                                continue;
                            };

                            track.last_known_bytes = match update.state {
                                ProgressState::Downloading(bytes) => bytes,
                                _ => track.last_known_bytes,
                            };

                            track.state = Some(update.state);
                            pending_edit = true;
                        }

                        ProgressTaskMessage::Done(id) => {
                            self.albums.remove(&id);
                            pending_edit = true;

                            if self.albums.is_empty() {
                                if let Err(e) = self.refresh_library().await {
                                    tracing::error!(error = %e, "failed to refresh library");
                                }
                            }
                        }
                    }
                },

                _ = tokio::time::sleep(timeout), if pending_edit => {
                    self.edit_message(&mut discord_message).await?;
                    last_edit = Instant::now();
                    pending_edit = false;
                }
            }
        }

        Ok(())
    }

    fn msg(&self) -> String {
        let mut msg = String::new();
        msg.push_str("---- downloads ----\n");

        if self.albums.is_empty() {
            msg.push_str("\n(no active downloads... check back later!)");
            return msg;
        }

        let mut albums = self.albums.iter().map(|(_, a)| a).collect::<Vec<_>>();
        albums.sort_by_key(|a| a.sort);

        for progress in albums {
            let mut tracks = progress.tracks.iter().map(|(_, t)| t).collect::<Vec<_>>();
            tracks.sort_by_key(|t| t.sort);
            let num_completed = tracks
                .iter()
                .filter(|t| matches!(t.state, Some(ProgressState::Finished)))
                .count();

            let total = tracks.len();

            let percent = if total == 0 {
                0
            } else {
                (num_completed as f64 / total as f64 * 100.0).floor() as u64
            };

            // let is_multidisc = tracks.iter().any(|t| t.sort.0 != 0 && t.sort.0 != 1);

            writeln!(
                msg,
                "\n{} - {} [{}]",
                progress.artist,
                progress.title,
                progress.release_date.year()
            )
            .ok();

            // accumulate all the bytes downloading for the album
            let byte = ByteSize(
                tracks
                    .iter()
                    .map(|t| match t.state {
                        Some(ProgressState::Downloading(bytes)) => bytes,
                        _ => t.last_known_bytes,
                    })
                    .sum::<u64>(),
            );

            writeln!(
                msg,
                "progress: {percent}% ({num_completed} / {total}) ({byte})"
            )
            .ok();

            // for track in tracks {
            //     let state: Cow<'_, str> = match track.state {
            //         None => "waiting".into(),
            //         Some(ProgressState::Downloading(bytes)) => {
            //             format!("downloading... ({})", ByteSize(bytes)).into()
            //         }
            //         Some(ProgressState::Transcoding) => "transcoding...".into(),
            //         Some(ProgressState::Finished) => "finished".into(),
            //     };

            //     if is_multidisc {
            //         writeln!(
            //             msg,
            //             "  {}.{:02}. {} - {}",
            //             track.sort.0, track.sort.1, track.name, state
            //         )
            //         .ok();
            //     } else {
            //         writeln!(msg, "  {:02}. {} - {}", track.sort.1, track.name, state).ok();
            //     }

            writeln!(msg).ok();
        }

        msg
    }

    fn msg_trimmed(&self) -> String {
        let mut msg = self.msg();

        // account for triple ticks
        if msg.len() > 1994 {
            msg = msg.chars().take(1994).collect();
        }

        msg.insert_str(0, "```");
        msg.push_str("```");
        msg
    }

    async fn edit_message(&self, msg: &mut Message) -> anyhow::Result<()> {
        let content = self.msg_trimmed();

        msg.edit(&self.http, EditMessage::new().content(content))
            .await?;

        Ok(())
    }

    async fn refresh_library(&self) -> anyhow::Result<()> {
        let Some(ref client) = self.submarine else {
            tracing::warn!("navidrome client not configured, skipping library refresh");
            return Ok(());
        };

        client.start_scan().await?;

        Ok(())
    }
}
