mod bot;
mod config;
mod ffmpeg;
mod pipeline;

use monochrome::Monochrome;
use tokio::sync::{OnceCell, Semaphore};

static GLOBAL_SEMAPHORE: OnceCell<Semaphore> = OnceCell::const_new();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        // hardcode monochrome=debug,pnnp=debug
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or("monochrome=debug,pnnp=debug".into()),
        )
        .init();

    let config = config::load()?;
    let client = Monochrome::new();

    GLOBAL_SEMAPHORE
        .set(Semaphore::new(config.downloads.global_semaphore))
        .map_err(|_| anyhow::anyhow!("failed to set global semaphore"))?;

    bot::start(client, config).await?;

    Ok(())
}
