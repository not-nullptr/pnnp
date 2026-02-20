use std::fs;

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

    let client = Monochrome::new();
    let tracks = client.search_tracks("ARE WE STILL FRIENDS?").await?;
    let track_result = tracks.first().expect("no tracks found");
    let track = client.track(track_result.id).await?;

    let data = client.download_track(&track).await?;
    fs::write("output.mp4", data)?;

    Ok(())
}
