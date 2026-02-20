mod bot;
mod config;
mod ffmpeg;
mod pipeline;

use monochrome::Monochrome;

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

    bot::start(client, config).await?;

    Ok(())
}
