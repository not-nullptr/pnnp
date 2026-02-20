use std::sync::Arc;

use crate::config::Config;
use monochrome::Monochrome;

pub struct Data {
    pub config: Arc<Config>,
    pub client: Monochrome,
}
