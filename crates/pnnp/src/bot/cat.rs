use poise::serenity_prelude::{self as serenity, CreateMessage, GetMessages};
use rand::seq::IndexedRandom;
use std::{sync::Arc, time::Duration};

pub async fn cat(http: Arc<serenity::Http>, channel: serenity::ChannelId, us: serenity::UserId) {
    loop {
        if let Err(e) = run_cat(http.as_ref(), channel, us).await {
            tracing::error!(error = ?e, "error in cat loop");
        }

        tokio::time::sleep(Duration::from_mins(15)).await;
    }
}

async fn run_cat(
    http: &serenity::Http,
    channel: serenity::ChannelId,
    us: serenity::UserId,
) -> anyhow::Result<()> {
    const PHRASES: &[&str] = &[
        "mrmeoww ;;.,,",
        "mrmeowrmwoww ;;",
        "mrrprmormowww,,;",
        "mrmeoowrmwowmoww ,,",
        "MRMEOWMROWWWW !!!!!",
        "mrrowwww !!!",
        "^_^",
        "mrmeowmwowwww",
        "nyaamarprrppp",
        "\\*purrs ,,\\*",
        "HISSSSSSS",
        "mrmowmpeowkmrwopww",
        "mrmeowmewrowmerwomrrrpprppp",
    ];

    let phrase = PHRASES.choose(&mut rand::rng()).unwrap();

    let last_ours = channel
        .messages(http, GetMessages::new().limit(1))
        .await?
        .into_iter()
        .next()
        .map(|m| m.author.id == us)
        .unwrap_or(false);

    if last_ours {
        tracing::warn!("last message in cat channel was ours, skipping to avoid spam");
        return Ok(());
    }

    channel
        .send_message(http, CreateMessage::new().content(*phrase))
        .await?;

    Ok(())
}
