use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MonochromeResponse<T> {
    pub data: T,
}
