mod album;
mod data;
mod interaction;
mod progress;

use std::sync::Arc;

use crate::{bot::progress::ProgressTask, config::Config};
use data::Data;
use monochrome::Monochrome;
use poise::serenity_prelude::{self as serenity, GetMessages};
use tokio::sync::{Semaphore, mpsc};

type Error = anyhow::Error;
type Context<'a> = poise::Context<'a, Data, Error>;

pub async fn start(client: Monochrome, config: Config) -> anyhow::Result<()> {
    tracing::info!("starting bot");

    let intents = serenity::GatewayIntents::non_privileged();

    let config = Arc::new(config);

    let framework = {
        let config = config.clone();

        poise::Framework::builder()
            .options(poise::FrameworkOptions {
                commands: vec![album::album()],
                event_handler: |ctx, event, _, data| {
                    Box::pin(async move {
                        match event {
                            serenity::FullEvent::InteractionCreate { interaction } => {
                                interaction::handle_interaction(ctx, interaction, data).await?;
                            }

                            _ => {}
                        }

                        Ok(())
                    })
                },
                ..Default::default()
            })
            .setup(move |ctx, ready, framework| {
                tracing::info!(user = %ready.user.name, "bot is ready");
                // delete the last message in the progress channel, just to clean up any old messages from previous runs
                let channel = serenity::ChannelId::new(config.bot.progress_channel);

                Box::pin(async move {
                    if let Some(msg) = channel
                        .messages(&ctx.http, GetMessages::default().limit(1))
                        .await?
                        .first()
                    {
                        channel.delete_message(&ctx.http, msg).await?;
                    }

                    // send a message to the progress channel just to indicate that the bot is online and working
                    let (tx, rx) = mpsc::unbounded_channel();
                    let mut task = ProgressTask::new(rx, ctx.http.clone(), channel);
                    tokio::spawn(async move {
                        if let Err(e) = task.run().await {
                            tracing::error!(error = %e, "progress task failed");
                        }
                    });

                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    Ok(Data {
                        track_semaphore: Arc::new(Semaphore::new(
                            config.downloads.track_concurrency,
                        )),
                        chunk_semaphore: Arc::new(Semaphore::new(
                            config.downloads.chunk_concurrency,
                        )),
                        config,
                        client,
                        progress_tx: tx,
                    })
                })
            })
            .build()
    };

    let mut client = serenity::ClientBuilder::new(&config.bot.token, intents)
        .framework(framework)
        .await?;

    client.start().await?;

    Ok(())
}
