use crate::id::ArtistId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: ArtistId,
    pub name: String,
}
