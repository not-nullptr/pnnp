use crate::{artist::Artist, id::AlbumId, track::TrackResult};
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: AlbumId,
    pub title: String,
    pub release_date: chrono::NaiveDate,
    pub artist: Artist,
    pub artists: Vec<Artist>,
    pub tracks: Vec<TrackResult>,
    pub cover: Uuid,
}
