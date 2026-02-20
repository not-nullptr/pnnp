use crate::{bot::progress::ProgressTaskMessage, config::Config};
use monochrome::Monochrome;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};

pub struct Data {
    pub config: Arc<Config>,
    pub client: Monochrome,
    pub track_semaphore: Arc<Semaphore>,
    pub chunk_semaphore: Arc<Semaphore>,
    pub progress_tx: mpsc::UnboundedSender<ProgressTaskMessage>,
}
