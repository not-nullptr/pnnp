use crate::id::TrackId;
use base64::{Engine, prelude::BASE64_STANDARD};
use serde::{Deserialize, Serialize};

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
    pub duration: u32,
}
