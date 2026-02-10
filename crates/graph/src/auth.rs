use anyhow::{Context, Result};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: String,
    pub tenant_id: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    expires_at: std::time::Instant,
}

#[derive(Clone)]
pub struct AuthProvider {
    credentials: ClientCredentials,
    http: reqwest::Client,
    cache: Arc<RwLock<Option<CachedToken>>>,
}

impl AuthProvider {
    pub fn new(credentials: ClientCredentials) -> Self {
        Self {
            credentials,
            http: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_token(&self) -> Result<String> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref()
                && cached.expires_at
                    > std::time::Instant::now() + std::time::Duration::from_secs(60)
            {
                return Ok(cached.access_token.clone());
            }
        }

        // Fetch new token
        let url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.credentials.tenant_id
        );
        let resp = self
            .http
            .post(&url)
            .form(&[
                ("client_id", self.credentials.client_id.as_str()),
                ("client_secret", self.credentials.client_secret.as_str()),
                ("scope", "https://graph.microsoft.com/.default"),
                ("grant_type", "client_credentials"),
            ])
            .send()
            .await
            .context("token request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("token request failed ({status}): {body}");
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .context("failed to parse token response")?;
        let cached = CachedToken {
            access_token: token_resp.access_token.clone(),
            expires_at: std::time::Instant::now()
                + std::time::Duration::from_secs(token_resp.expires_in),
        };

        let mut cache = self.cache.write().await;
        *cache = Some(cached);

        Ok(token_resp.access_token)
    }
}
