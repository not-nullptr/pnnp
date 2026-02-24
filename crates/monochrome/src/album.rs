use crate::{
    artist::Artist,
    id::{AlbumId, ArtistId},
    track::Track,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumResult {
    pub id: AlbumId,
    pub title: String,
    pub release_date: chrono::NaiveDate,
    pub artists: Vec<Artist>,
    pub cover: Uuid,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: AlbumId,
    pub title: String,
    pub release_date: chrono::NaiveDate,
    pub artist: Artist,
    pub artists: Vec<Artist>,
    pub tracks: Vec<Track>,
    pub cover: Uuid,
    #[serde(rename = "type")]
    pub kind: String,
}

impl Into<Album> for AlbumResult {
    fn into(self) -> Album {
        Album {
            id: self.id,
            title: self.title,
            release_date: self.release_date,
            artist: self.artists.first().cloned().unwrap_or_else(|| Artist {
                id: ArtistId::from(0),
                name: "Unknown Artist".to_string(),
                kind: "MAIN".to_string(),
            }),
            artists: self.artists,
            tracks: Vec::new(),
            cover: self.cover,
            kind: self.kind,
        }
    }
}
