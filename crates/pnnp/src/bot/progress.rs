use bytesize::ByteSize;
use chrono::{Datelike, NaiveDate};
use monochrome::{
    album::Album,
    id::{AlbumId, TrackId},
    track::Track,
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
    track_or_album::TrackOrAlbum,
};

pub struct ProgressTask {
    rx: mpsc::UnboundedReceiver<ProgressTaskMessage>,
    http: Arc<serenity::Http>,
    channel: serenity::ChannelId,
    albums: HashMap<AlbumId, AlbumProgress>,
    tracks: HashMap<TrackId, TrackProgress>,
    count: usize,
    submarine: Option<submarine::Client>,
}

pub enum ProgressTaskMessage {
    DiscoverAlbum(AlbumId, Album),
    DiscoverTrack(TrackId, Track),
    Progress(ProgressUpdate),
    TrackDone(TrackId),
}

struct AlbumProgress {
    sort: usize,
    tracks: Vec<TrackId>,
    title: String,
    artist: String,
    release_date: NaiveDate,
}

struct TrackProgress {
    // name: String,
    sort: (u32, u32),
    state: Option<ProgressState>,
    last_known_bytes: u64,
    album_id: Option<AlbumId>,
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
            tracks: HashMap::new(),
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
                    pending_edit = self.handle_message(msg).await?;
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
            let mut tracks = progress
                .tracks
                .iter()
                .filter_map(|id| self.tracks.get(id))
                .collect::<Vec<_>>();

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

        tracing::info!("edited progress message");

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

    async fn handle_message(&mut self, msg: ProgressTaskMessage) -> anyhow::Result<bool> {
        match msg {
            ProgressTaskMessage::DiscoverAlbum(id, album) => {
                self.albums.insert(
                    id,
                    AlbumProgress {
                        tracks: album.tracks.iter().map(|t| t.id).collect(),
                        title: album.title,
                        release_date: album.release_date,
                        artist: album.artist.name,
                        sort: self.count,
                    },
                );

                self.tracks.extend(album.tracks.into_iter().map(|t| {
                    (
                        t.id,
                        TrackProgress {
                            sort: (t.volume_number, t.track_number),
                            state: None,
                            last_known_bytes: 0,
                            album_id: Some(id),
                        },
                    )
                }));

                self.count = self.count.wrapping_add(1);

                Ok(true)
            }

            ProgressTaskMessage::DiscoverTrack(id, track) => {
                self.tracks.insert(
                    id,
                    TrackProgress {
                        sort: (track.volume_number, track.track_number),
                        state: None,
                        last_known_bytes: 0,
                        album_id: None,
                    },
                );

                Ok(true)
            }

            ProgressTaskMessage::Progress(update) => {
                let Some(track) = self.tracks.get_mut(&update.track_id) else {
                    return Ok(false);
                };

                track.last_known_bytes = match update.state {
                    ProgressState::Downloading(bytes) => bytes,
                    _ => track.last_known_bytes,
                };

                track.state = Some(update.state);

                if let Some(album_id) = track.album_id {
                    self.progress_update(album_id).await;
                }

                Ok(true)
            }

            ProgressTaskMessage::TrackDone(id) => {
                let album_id = self.tracks.get(&id).and_then(|t| t.album_id);

                if let Some(album_id) = album_id {
                    self.progress_update(album_id).await;
                } else {
                    self.track_removal(id).await;
                }

                Ok(true)
            }
        }
    }

    async fn progress_update(&mut self, album_id: AlbumId) -> bool {
        let Some(album) = self.albums.get(&album_id) else {
            return false;
        };

        let all_finished = album
            .tracks
            .iter()
            .filter_map(|id| self.tracks.get(id))
            .all(|t| matches!(t.state, Some(ProgressState::Finished)));

        if all_finished {
            tracing::info!(album = %album.title, "album completed, removing from progress");
            let track_ids = album.tracks.clone();
            for track_id in track_ids {
                self.track_removal(track_id).await;
            }

            self.albums.remove(&album_id);

            return true;
        }

        false
    }

    async fn track_removal(&mut self, track_id: TrackId) {
        let are_we_last = self.tracks.len() == 1;
        self.tracks.remove(&track_id);

        if self.tracks.is_empty() && are_we_last {
            tracing::info!("all downloads completed, refreshing library...");
            if let Err(e) = self.refresh_library().await {
                tracing::error!(error = %e, "failed to refresh library");
            }
        }
    }
}

pub fn done_msgs(album: &Album) -> Vec<ProgressTaskMessage> {
    album
        .tracks
        .iter()
        .map(|t| ProgressTaskMessage::TrackDone(t.id))
        .collect()
}
