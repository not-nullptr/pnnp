mod error;
mod id;
mod response;
mod track;

use crate::{
    error::MonochromeManifestError,
    id::TrackId,
    response::MonochromeResponse,
    track::{Track, TrackResult},
};
use dash_mpd::fetch::DashDownloader;
pub use error::MonochromeError;
use reqwest::Url;
use roxmltree::Document;
use serde::Deserialize;
use tokio::fs::File;
use uuid::Uuid;

const BASE_URL: &'static str = "https://eu-central.monochrome.tf";

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
        self.fetch(URL, [("id", id.into())]).await
    }

    pub async fn download_track(&self, track: &Track) -> Result<Vec<u8>, MonochromeError> {
        let manifest = track.decode_manifest()?;
        #[derive(Debug, Deserialize)]
        struct UrlHolder {
            urls: Vec<String>,
        }

        let url = if manifest.contains("<MPD") {
            return Ok(self.download_mpd(manifest).await?);
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

        let bytes = res.bytes().await?;

        Ok(bytes.into())
    }

    async fn download_mpd(&self, manifest: String) -> Result<Vec<u8>, MonochromeManifestError> {
        // let doc = Document::parse(&manifest)?;

        // let seg = doc
        //     .descendants()
        //     .find(|n| n.tag_name().name() == "SegmentTemplate")
        //     .ok_or_else(|| MonochromeManifestError::MissingSegmentTemplate)?;

        // let init_tpl = seg
        //     .attribute("initialization")
        //     .ok_or_else(|| MonochromeManifestError::MissingInitializationTemplate)?;

        // let media_tpl = seg
        //     .attribute("media")
        //     .ok_or_else(|| MonochromeManifestError::MissingMedia)?;

        // let start_number: u64 = seg
        //     .attribute("startNumber")
        //     .and_then(|s| s.parse().ok())
        //     .unwrap_or(1);

        // let mut segment_counts: Vec<u64> = Vec::new();
        // if let Some(tl) = seg
        //     .children()
        //     .find(|c| c.tag_name().name() == "SegmentTimeline")
        // {
        //     for s in tl.children().filter(|c| c.tag_name().name() == "S") {
        //         let d: u64 = s
        //             .attribute("d")
        //             .and_then(|v| v.parse().ok())
        //             .ok_or_else(|| MonochromeManifestError::SMissingD)?;
        //         let r: i64 = s.attribute("r").and_then(|v| v.parse().ok()).unwrap_or(0);
        //         for _ in 0..=(r as usize) {
        //             segment_counts.push(d);
        //         }
        //     }
        // } else {
        //     segment_counts.push(0);
        // }

        // let replace_number = |tpl: &str, n: u64| tpl.replace("$Number$", &n.to_string());

        // let mut out: Vec<u8> = Vec::new();

        // let init_url = Url::parse(init_tpl)?;

        // let init_bytes = self.client.get(init_url).send().await?.bytes().await?;
        // out.extend_from_slice(&init_bytes);

        // // Fetch each media segment by increasing $Number$
        // for (idx, _dur) in segment_counts.iter().enumerate() {
        //     let number = start_number + idx as u64;
        //     let url = replace_number(media_tpl, number);
        //     let bytes = self.client.get(url).send().await?.bytes().await?;
        //     out.extend_from_slice(&bytes);
        // }

        // Ok(out)

        // this is so dumb...
        let id = Uuid::new_v4();
        let name = format!("monochrome-{}.mpd", id);
        let path = std::env::temp_dir().join(name);

        let mut file = File::options()
            .write(true)
            .truncate(true)
            .create_new(true)
            .open(&path)?;

        file.write_all(manifest.as_bytes()).await?;
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
