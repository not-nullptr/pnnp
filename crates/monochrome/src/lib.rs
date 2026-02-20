pub mod album;
pub mod artist;
mod error;
pub mod id;
mod response;
pub mod track;

use std::sync::Arc;

use crate::{
    album::{Album, AlbumResult},
    artist::Artist,
    error::MonochromeManifestError,
    id::{AlbumId, TrackId},
    response::MonochromeResponse,
    track::{Track, TrackResult},
};
use async_stream::try_stream;
use bytes::Bytes;
pub use error::MonochromeError;
use futures::{Stream, StreamExt, stream::FuturesOrdered};
use reqwest::Url;
use roxmltree::Document;
use serde::{Deserialize, Deserializer};
use tokio::sync::Semaphore;
use uuid::Uuid;

const BASE_URL: &'static str = "https://arran.monochrome.tf";
const RESOURCES_URL: &'static str = "https://resources.tidal.com/images";

#[derive(Debug, Clone)]
pub struct Monochrome {
    client: reqwest::Client,
}

impl Monochrome {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn track(&self, id: impl Into<TrackId>) -> Result<Track, MonochromeError> {
        const PATH: &'static str = "track";
        const URL: &'static str = const_format::formatcp!("{BASE_URL}/{PATH}");
        self.fetch(
            URL,
            [
                ("id", id.into().to_string().as_ref()),
                ("quality", "HI_RES_LOSSLESS"),
            ],
        )
        .await
    }

    pub async fn download_track(
        &self,
        track: &Track,
        chunk_semaphore: Arc<Semaphore>,
    ) -> Result<impl Stream<Item = Result<Bytes, reqwest::Error>>, MonochromeError> {
        let manifest = track.decode_manifest()?;
        #[derive(Debug, Deserialize)]
        struct UrlHolder {
            urls: Vec<String>,
        }

        let url = if manifest.contains("<MPD") {
            return Ok(MaybeMpdStream::Mpd(Box::pin(
                self.download_mpd(manifest, chunk_semaphore).await?,
            )));
        } else if let Ok(urls) = serde_json::from_str::<UrlHolder>(&manifest)
            && let Some(url) = urls.urls.into_iter().next()
        {
            url
        } else {
            return Err(MonochromeError::ManifestDecode);
        };

        let res = self.client.get(url).send().await?;
        if res.status() != reqwest::StatusCode::OK {
            return Err(MonochromeError::Non200(res.text().await?));
        }

        let bytes = res.bytes_stream();

        Ok(MaybeMpdStream::Regular(bytes))
    }

    async fn download_mpd(
        &self,
        manifest: String,
        chunk_semaphore: Arc<Semaphore>,
    ) -> Result<impl Stream<Item = Result<Bytes, reqwest::Error>>, MonochromeManifestError> {
        let doc = Document::parse(&manifest)?;

        let seg = doc
            .descendants()
            .find(|n| n.tag_name().name() == "SegmentTemplate")
            .ok_or_else(|| MonochromeManifestError::MissingSegmentTemplate)?;

        let init_tpl = seg
            .attribute("initialization")
            .ok_or_else(|| MonochromeManifestError::MissingInitializationTemplate)?;

        let media_tpl = seg
            .attribute("media")
            .ok_or_else(|| MonochromeManifestError::MissingMedia)?;

        let start_number: u64 = seg
            .attribute("startNumber")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let mut segment_counts: Vec<u64> = Vec::new();
        if let Some(tl) = seg
            .children()
            .find(|c| c.tag_name().name() == "SegmentTimeline")
        {
            for s in tl.children().filter(|c| c.tag_name().name() == "S") {
                let d: u64 = s
                    .attribute("d")
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| MonochromeManifestError::SMissingD)?;
                let r: i64 = s.attribute("r").and_then(|v| v.parse().ok()).unwrap_or(0);
                for _ in 0..=(r as usize) {
                    segment_counts.push(d);
                }
            }
        } else {
            segment_counts.push(0);
        }

        let init_url = Url::parse(init_tpl)?;
        let media_tpl = media_tpl.to_string();

        Ok(try_stream! {
            let init_bytes = self.client.get(init_url).send().await?.bytes().await?;
            yield init_bytes;

            let mut futs = FuturesOrdered::new();

            for (idx, _dur) in segment_counts.iter().enumerate() {
                let client = self.client.clone();
                let sem = chunk_semaphore.clone();
                let number = start_number + idx as u64;
                let url = media_tpl.replace("$Number$", &number.to_string());

                futs.push_back(tokio::spawn(async move {
                    let _permit = sem.acquire_owned().await.unwrap();
                    client.get(url).send().await?.bytes().await
                }));
            }

            while let Some(res) = futs.next().await {
                yield res.unwrap()?;
            }
        })
    }

    pub async fn search_tracks(
        &self,
        query: impl AsRef<str>,
    ) -> Result<Vec<TrackResult>, MonochromeError> {
        let query = query.as_ref();

        #[derive(Debug, Deserialize)]
        struct Res {
            items: Vec<TrackResult>,
        }

        const PATH: &'static str = "search";
        const URL: &'static str = const_format::formatcp!("{BASE_URL}/{PATH}");
        let res: Res = self.fetch(URL, [("s", query)]).await?;
        Ok(res.items)
    }

    pub async fn search_albums(
        &self,
        query: impl AsRef<str>,
    ) -> Result<Vec<AlbumResult>, MonochromeError> {
        let query = query.as_ref();

        #[derive(Debug, Deserialize)]
        struct Res {
            albums: Albums,
        }

        #[derive(Debug, Deserialize)]
        struct Albums {
            items: Vec<AlbumResult>,
        }

        const PATH: &'static str = "search";
        const URL: &'static str = const_format::formatcp!("{BASE_URL}/{PATH}");
        let res: Res = self.fetch(URL, [("al", query)]).await?;
        Ok(res.albums.items)
    }

    pub async fn album(&self, id: impl Into<id::AlbumId>) -> Result<album::Album, MonochromeError> {
        const PATH: &'static str = "album";
        const URL: &'static str = const_format::formatcp!("{BASE_URL}/{PATH}");

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct AlbumTemp {
            pub id: AlbumId,
            pub title: String,
            pub release_date: chrono::NaiveDate,
            pub artist: Artist,
            pub artists: Vec<Artist>,
            pub items: Vec<Item>,
            pub cover: Uuid,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Item {
            #[serde(rename = "type")]
            pub kind: String,
            #[serde(default, deserialize_with = "null_on_error")]
            pub item: Option<TrackResult>,
        }

        let res: AlbumTemp = self
            .fetch(URL, [("id", id.into().to_string().as_str())])
            .await?;

        let tracks = res
            .items
            .into_iter()
            .filter_map(|i| {
                if i.kind == "track"
                    && let Some(track) = i.item
                {
                    Some(track)
                } else {
                    None
                }
            })
            .collect();

        Ok(album::Album {
            id: res.id,
            title: res.title,
            release_date: res.release_date,
            artist: res.artist,
            artists: res.artists,
            cover: res.cover,
            tracks,
        })
    }

    pub async fn album_art(
        &self,
        album: &Album,
    ) -> Result<impl Stream<Item = Result<Bytes, reqwest::Error>>, MonochromeError> {
        self.art(album.cover).await
    }

    async fn art(
        &self,
        uuid: Uuid,
    ) -> Result<impl Stream<Item = Result<Bytes, reqwest::Error>>, MonochromeError> {
        let id = uuid.to_string().replace("-", "/");
        let url = format!("{RESOURCES_URL}/{id}/1280x1280.jpg");
        let res = self.client.get(url).send().await?;
        if res.status() != reqwest::StatusCode::OK {
            return Err(MonochromeError::Non200(res.text().await?));
        }

        Ok(res.bytes_stream())
    }

    async fn fetch<T, Q>(&self, url: &str, query: Q) -> Result<T, MonochromeError>
    where
        T: serde::de::DeserializeOwned,
        Q: serde::ser::Serialize,
    {
        let response = self.client.get(url).query(&query).send().await?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(MonochromeError::Non200(response.text().await?));
        }

        let data = response.json::<MonochromeResponse<T>>().await?;
        Ok(data.data)
    }
}

pub enum MaybeMpdStream<
    M: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
    I: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
> {
    Mpd(M),
    Regular(I),
}

impl<
    M: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
    I: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
> Stream for MaybeMpdStream<M, I>
{
    type Item = Result<Bytes, reqwest::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match &mut *self {
            MaybeMpdStream::Mpd(s) => std::pin::Pin::new(s).poll_next(cx),
            MaybeMpdStream::Regular(s) => std::pin::Pin::new(s).poll_next(cx),
        }
    }
}

fn null_on_error<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    match T::deserialize(deserializer) {
        Ok(v) => Ok(Some(v)),
        Err(_) => Ok(None), // swallow error -> field becomes None
    }
}
