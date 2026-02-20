mod album;
mod data;
mod interaction;

use std::sync::Arc;

use crate::config::Config;
use data::Data;
use monochrome::Monochrome;
use poise::serenity_prelude as serenity;

type Error = anyhow::Error;
type Context<'a> = poise::Context<'a, Data, Error>;

pub async fn start(client: Monochrome, config: Config) -> anyhow::Result<()> {
    tracing::info!("starting bot");

    let intents = serenity::GatewayIntents::non_privileged();

    let config = Arc::new(config);
    let c = config.clone();

    let framework = poise::Framework::builder()
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
        .setup(|ctx, ready, framework| {
            tracing::info!(user = %ready.user.name, "bot is ready");
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data { config, client })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(&c.bot.token, intents)
        .framework(framework)
        .await?;

    client.start().await?;

    Ok(())
}
