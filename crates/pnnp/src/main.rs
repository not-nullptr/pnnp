mod bot;
mod config;
mod ffmpeg;
mod pipeline;
mod track_or_album;

use std::time::Duration;

use monochrome::{Monochrome, endpoint::Endpoint};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or("monochrome=debug,pnnp=debug".into()),
        )
        .init();

    let config = config::load()?;
    // let client = Monochrome::new();

    // bot::start(client, config).await?;

    let endpoint = Endpoint::new();
    endpoint.scan().await?;
    let client = Monochrome::new(endpoint.clone());

    let preferred_api = endpoint.preferred_api().await;
    let preferred_streaming = endpoint.preferred_streaming().await;

    tracing::info!(preferred_api = %preferred_api, preferred_streaming = %preferred_streaming, "scan complete");

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_mins(20)).await;
            if let Err(e) = endpoint.scan().await {
                tracing::error!(error = %e, "failed to rescan endpoints");
            } else {
                let preferred_api = endpoint.preferred_api().await;
                let preferred_streaming = endpoint.preferred_streaming().await;

                tracing::info!(preferred_api = %preferred_api, preferred_streaming = %preferred_streaming, "rescan complete");
            }
        }
    });

    bot::start(client, config).await?;

    Ok(())
}
