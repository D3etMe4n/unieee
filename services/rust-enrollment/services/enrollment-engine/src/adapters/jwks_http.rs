use crate::core::JwksFetcher;
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

#[derive(Clone)]
pub struct ReqwestJwksFetcher {
    client: Client,
    jwks_uri: String,
}

impl ReqwestJwksFetcher {
    pub fn new(jwks_uri: impl Into<String>, timeout: Duration) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| format!("failed to build JWKS HTTP client: {e}"))?;
        Ok(Self {
            client,
            jwks_uri: jwks_uri.into(),
        })
    }
}

#[async_trait]
impl JwksFetcher for ReqwestJwksFetcher {
    async fn fetch_jwks_json(&self) -> Result<String, String> {
        let resp = self
            .client
            .get(&self.jwks_uri)
            .send()
            .await
            .map_err(|e| format!("JWKS request error: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("JWKS endpoint returned HTTP {}", resp.status()));
        }
        resp.text()
            .await
            .map_err(|e| format!("JWKS response read error: {e}"))
    }
}
