mod bot;
mod config;
mod ffmpeg;
mod pipeline;

use monochrome::Monochrome;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(tokio_unstable)]
    console_subscriber::init();

    #[cfg(not(tokio_unstable))]
    tracing_subscriber::fmt()
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
