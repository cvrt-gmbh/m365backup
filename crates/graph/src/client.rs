use std::time::Duration;

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use tracing::{debug, warn};

use crate::auth::AuthProvider;

const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";
const MAX_RETRIES: u32 = 5;

#[derive(Clone)]
pub struct GraphClient {
    auth: AuthProvider,
    http: reqwest::Client,
}

impl GraphClient {
    pub fn new(auth: AuthProvider) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build HTTP client");
        Self { auth, http }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = if path.starts_with("https://") {
            path.to_string()
        } else {
            format!("{GRAPH_BASE}{path}")
        };

        let mut retries = 0;
        loop {
            let token = self.auth.get_token().await?;
            let resp = self
                .http
                .get(&url)
                .bearer_auth(&token)
                .send()
                .await
                .with_context(|| format!("GET {url} failed"))?;

            let status = resp.status();

            if status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
            {
                retries += 1;
                if retries > MAX_RETRIES {
                    anyhow::bail!("max retries exceeded for {url}");
                }
                let retry_after = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(2u64.pow(retries));
                warn!(url = %url, retry_after, retries, "rate limited, backing off");
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }

            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("GET {url} returned {status}: {body}");
            }

            debug!(url = %url, "OK");
            return resp.json().await.context("failed to deserialize response");
        }
    }

    pub async fn get_bytes(&self, url: &str) -> Result<bytes::Bytes> {
        let full_url = if url.starts_with("https://") {
            url.to_string()
        } else {
            format!("{GRAPH_BASE}{url}")
        };

        let mut retries = 0;
        loop {
            let token = self.auth.get_token().await?;
            let resp = self
                .http
                .get(&full_url)
                .bearer_auth(&token)
                .send()
                .await
                .with_context(|| format!("GET {full_url} failed"))?;

            let status = resp.status();

            if status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
            {
                retries += 1;
                if retries > MAX_RETRIES {
                    anyhow::bail!("max retries exceeded for {full_url}");
                }
                let retry_after = resp
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(2u64.pow(retries));
                warn!(url = %full_url, retry_after, retries, "rate limited, backing off");
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }

            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("GET {full_url} returned {status}: {body}");
            }

            return resp.bytes().await.context("failed to read response body");
        }
    }

    /// Paginate through all pages of a Graph API collection.
    pub async fn get_all_pages<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>> {
        let mut items = Vec::new();
        let mut url = if path.starts_with("https://") {
            path.to_string()
        } else {
            format!("{GRAPH_BASE}{path}")
        };

        loop {
            let page: GraphPage<T> = self.get_json(&url).await?;
            items.extend(page.value);
            match page.next_link {
                Some(next) => url = next,
                None => break,
            }
        }

        Ok(items)
    }
}

#[derive(serde::Deserialize)]
pub struct GraphPage<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}
