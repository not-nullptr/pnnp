use figment::{
    Figment,
    providers::{Format, Toml},
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub output: OutputConfig,
    pub bot: BotConfig,
    pub downloads: DownloadConfig,
}

#[derive(Debug, Deserialize)]
pub struct OutputConfig {
    pub dir: String,
}

#[derive(Debug, Deserialize)]
pub struct BotConfig {
    pub token: String,
    pub progress_channel: u64,
}

#[derive(Debug, Deserialize)]
pub struct DownloadConfig {
    pub chunk_concurrency: usize,
    pub track_concurrency: usize,
}

pub fn load() -> anyhow::Result<Config> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("failed to get config directory"))?
        .join("pnnp")
        .join("config.toml");

    Ok(Figment::new()
        .merge(Toml::file("config.toml"))
        .merge(Toml::file(config_dir))
        .extract()?)
}
