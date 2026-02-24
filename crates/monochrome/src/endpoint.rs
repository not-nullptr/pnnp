use std::{sync::Arc, time::Duration};

use crate::{
    Monochrome, MonochromeError, error::MonochromeManifestError, response::MonochromeResponse,
    track::TrackManifest,
};
use chrono::Utc;
use reqwest::{Response, Url};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::RwLock;

const UPTIME_URL: &str = "https://tidal-uptime.jiffy-puffs-1j.workers.dev";

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("failed to fetch url: {0:?}")]
    Fetch(#[from] reqwest::Error),
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    // TODO: would prefer if these weren't Arc<RwLock<T>> but this is the easiest way for now
    preferred_api: Arc<RwLock<Url>>,
    preferred_streaming: Arc<RwLock<Url>>,
    client: reqwest::Client,
}

impl Endpoint {
    pub fn new() -> Self {
        Self {
            preferred_api: Arc::new(RwLock::new("https://triton.squid.wtf".parse().unwrap())),
            preferred_streaming: Arc::new(RwLock::new("https://triton.squid.wtf".parse().unwrap())),
            client: reqwest::Client::new(),
        }
    }

    pub async fn scan(&self) -> Result<(), ScanError> {
        let uptime: UptimeResponse = self.client.get(UPTIME_URL).send().await?.json().await?;

        let api_futs = uptime
            .api
            .into_iter()
            .map(|a| ApiMeasureInfo {
                url: a.url.clone(),
                test_url: {
                    let mut url = a.url.clone();
                    url.set_path("/album");
                    url.set_query(Some("id=109485854"));
                    url
                },
                kind: MeasureKind::Api,
            })
            .map(|api| {
                let client = self.client.clone();
                (api.url.clone(), fut(api, client, async |_| Ok(())))
            });

        let streaming_futs = uptime
            .streaming
            .into_iter()
            .map(|a| ApiMeasureInfo {
                url: a.url.clone(),
                test_url: {
                    let mut url = a.url.clone();
                    url.set_path("/track");
                    url.set_query(Some("id=109485855"));
                    url
                },
                kind: MeasureKind::Streaming,
            })
            .map(|api| {
                let client = self.client.clone();
                (
                    api.url.clone(),
                    fut(api, client, async |res| {
                        let data: MonochromeResponse<TrackManifest> = res.json().await?;
                        let data = data.data;
                        if data.asset_presentation != "PREVIEW" {
                            return Err(MonochromeError::Manifest(
                                MonochromeManifestError::Preview,
                            ));
                        }
                        Ok(())
                    }),
                )
            });

        let (api_measurements, streaming_measurements) = futures::future::join(
            futures::future::join_all(api_futs.map(|(url, fut)| async move { (url, fut.await) })),
            futures::future::join_all(
                streaming_futs.map(|(url, fut)| async move { (url, fut.await) }),
            ),
        )
        .await;

        let mut api_measurements = api_measurements
            .into_iter()
            .filter_map(|(u, r)| match r {
                Ok(m) => Some(m),
                Err(e) => {
                    tracing::warn!(error = %e, url = %u, "failed to measure api endpoint");
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut streaming_measurements = streaming_measurements
            .into_iter()
            .filter_map(|(u, m)| match m {
                Ok(m) => Some(m),
                Err(e) => {
                    tracing::warn!(error = %e, url = %u, "failed to measure streaming endpoint");
                    None
                }
            })
            .collect::<Vec<_>>();

        api_measurements.sort_by_key(|m| m.latency);

        streaming_measurements.sort_by_key(|m| m.latency);

        if let Some(best) = api_measurements.into_iter().next() {
            *self.preferred_api.write().await = best.url;
        }

        if let Some(best) = streaming_measurements.into_iter().next() {
            *self.preferred_streaming.write().await = best.url;
        }

        Ok(())
    }

    pub fn api(&self) -> Monochrome {
        Monochrome::new(self.clone())
    }

    pub fn client(&self) -> reqwest::Client {
        self.client.clone()
    }

    pub async fn preferred_api(&self) -> Url {
        self.preferred_api.read().await.clone()
    }

    pub async fn preferred_streaming(&self) -> Url {
        self.preferred_streaming.read().await.clone()
    }

    pub(crate) async fn fetch<T, Q>(
        &self,
        path: &str,
        kind: FetchKind,
        query: Q,
    ) -> Result<T, MonochromeError>
    where
        T: serde::de::DeserializeOwned,
        Q: serde::ser::Serialize,
    {
        let url = match kind {
            FetchKind::Api => self.preferred_api().await.join(path)?,
            FetchKind::Streaming => self.preferred_streaming().await.join(path)?,
        };

        tracing::debug!(%url, "fetching endpoint");

        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(5))
            .query(&query)
            .send()
            .await?;
        if response.status() != reqwest::StatusCode::OK {
            // if it's 429, we need to find a diff endpoint
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                tracing::warn!("endpoint returned 429, rescanning for new endpoints");
                if let Err(e) = self.scan().await {
                    tracing::error!(error = %e, "failed to rescan endpoints after 429");
                } else {
                    tracing::info!("rescan complete, retrying request");
                    return Box::pin(self.fetch(path, kind, query)).await;
                }
            }

            return Err(MonochromeError::Non200(response.text().await?));
        }

        let data = response.json::<MonochromeResponse<T>>().await?;
        Ok(data.data)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FetchKind {
    Api,
    Streaming,
}

#[derive(Debug, Deserialize)]
struct UptimeResponse {
    api: Vec<Api>,
    streaming: Vec<Api>,
}

#[derive(Debug, Deserialize)]
struct Api {
    url: Url,
}

#[derive(Debug)]
struct ApiMeasureInfo {
    url: Url,
    test_url: Url,
    kind: MeasureKind,
}

#[derive(Debug, Clone)]
struct EndpointMeasurement {
    url: Url,
    latency: chrono::Duration,
}

async fn fut<V>(
    api: ApiMeasureInfo,
    client: reqwest::Client,
    validate: V,
) -> Result<EndpointMeasurement, MonochromeError>
where
    V: AsyncFnOnce(Response) -> Result<(), MonochromeError>,
{
    let start = Utc::now();
    tracing::debug!(url = %api.url, kind = ?api.kind, "measuring endpoint");
    let res = client
        .get(api.test_url)
        .timeout(Duration::from_secs(5))
        .send()
        .await?;

    let latency = Utc::now() - start;
    if res.status() != reqwest::StatusCode::OK {
        return Err(MonochromeError::Non200(res.text().await?));
    }

    validate(res).await?;

    tracing::debug!(url = %api.url, %latency, kind = ?api.kind, "endpoint responded");

    Ok(EndpointMeasurement {
        url: api.url,
        latency,
    })
}

#[derive(Debug, Clone, Copy)]
enum MeasureKind {
    Api,
    Streaming,
}
