use crate::id::{AlbumId, ArtistId, TrackId};
use base64::{Engine, prelude::BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub track_id: TrackId,
    pub manifest_mime_type: String,
    pub manifest: String,
}

impl Track {
    pub fn decode_manifest(&self) -> Result<String, base64::DecodeError> {
        let decoded = BASE64_STANDARD.decode(&self.manifest)?;
        Ok(String::from_utf8_lossy(&decoded).into())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackResult {
    pub id: TrackId,
    pub title: String,
    pub artist: TrackArtist,
    pub artists: Vec<TrackArtist>,
    pub album: TrackAlbum,
    pub duration: u32,
    pub track_number: u32,
    pub volume_number: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackAlbum {
    pub id: AlbumId,
    pub title: String,
    pub cover: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackArtist {
    pub id: ArtistId,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}
