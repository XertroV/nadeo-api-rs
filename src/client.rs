use reqwest::header::{HeaderValue, AUTHORIZATION, CONTENT_LENGTH, LOCATION};
pub use reqwest::{Error as RqError, RequestBuilder, Response};
pub use serde_json::Value;
use tokio::sync::{SemaphorePermit, TryLockError};

use crate::{auth::*, urls::*};

pub enum MonthlyCampaignType {
    Royal,
    TOTD,
}

pub async fn run_req<'a>(
    (req, permit): (RequestBuilder, SemaphorePermit<'a>),
) -> Result<Value, RqError> {
    let r = req.send().await?.json().await?;
    drop(permit);
    Ok(r)
}

pub trait NadeoApiClient {
    async fn get_client(&self) -> &reqwest::Client;

    async fn rate_limit(&self) -> SemaphorePermit;

    async fn get_file_size(&self, url: &str) -> Result<u64, RqError> {
        let resp = self.get_client().await.head(url).send().await?;
        Ok(resp
            .headers()
            .get(CONTENT_LENGTH)
            .map_or(0, |v| v.to_str().unwrap_or("0").parse().unwrap_or(1)))
    }

    fn get_auth_token(&self, audience: NadeoAudience) -> Result<String, TryLockError>;

    async fn aget_auth_token(&self, audience: NadeoAudience) -> String;

    fn get_auth_header(&self, audience: NadeoAudience) -> String {
        format!("nadeo_v1 t={}", &self.get_auth_token(audience).unwrap())
    }

    async fn aget_auth_header(&self, audience: NadeoAudience) -> String {
        format!("nadeo_v1 t={}", &self.aget_auth_token(audience).await)
    }

    async fn get_auth_header_value(&self, audience: NadeoAudience) -> HeaderValue {
        let mut hv = HeaderValue::from_str(&self.aget_auth_header(audience).await).unwrap();
        hv.set_sensitive(true);
        hv
    }

    async fn oauth_get<'a>(
        &self,
        path: &str,
        token: &OAuthToken,
        _permit: &SemaphorePermit<'a>,
    ) -> RequestBuilder {
        let rb = self
            .get_client()
            .await
            .get(&format!("{}{}", OAUTH_URL, path))
            .header(AUTHORIZATION, token.get_authz_header());
        rb
    }

    async fn core_get(&self, path: &str) -> (RequestBuilder, SemaphorePermit) {
        (
            self.get_client()
                .await
                .get(&format!("{}{}", CORE_URL, path))
                .header(AUTHORIZATION, self.get_auth_header_value(Core).await),
            self.rate_limit().await,
        )
    }

    async fn run_core_get(&self, path: &str) -> Result<Value, RqError> {
        run_req(self.core_get(path).await).await
    }

    async fn core_post_bytes(&self, path: &str, body: &[u8]) -> (RequestBuilder, SemaphorePermit) {
        (
            self.get_client()
                .await
                .post(&format!("{}{}", CORE_URL, path))
                .header(AUTHORIZATION, self.get_auth_header_value(Core).await)
                .body(body.to_vec()),
            self.rate_limit().await,
        )
    }
    async fn run_core_post_bytes(&self, path: &str, body: &[u8]) -> Result<Value, RqError> {
        run_req(self.core_post_bytes(path, body).await).await
    }

    async fn core_post(&self, path: &str, body: &Value) -> (RequestBuilder, SemaphorePermit) {
        (
            self.get_client()
                .await
                .post(&format!("{}{}", CORE_URL, path))
                .header(AUTHORIZATION, self.get_auth_header_value(Core).await)
                .json(body),
            self.rate_limit().await,
        )
    }
    async fn run_core_post(&self, path: &str, body: &Value) -> Result<Value, RqError> {
        run_req(self.core_post(path, body).await).await
    }

    async fn live_get(&self, path: &str) -> (RequestBuilder, SemaphorePermit) {
        (
            self.get_client()
                .await
                .get(&format!("{}{}", LIVE_URL, path))
                .header(AUTHORIZATION, self.get_auth_header_value(Live).await),
            self.rate_limit().await,
        )
    }
    async fn run_live_get(&self, path: &str) -> Result<Value, RqError> {
        run_req(self.live_get(path).await).await
    }

    async fn live_post_bytes(&self, path: &str, body: &[u8]) -> (RequestBuilder, SemaphorePermit) {
        (
            self.get_client()
                .await
                .post(&format!("{}{}", LIVE_URL, path))
                .header(AUTHORIZATION, self.get_auth_header_value(Live).await)
                .body(body.to_vec()),
            self.rate_limit().await,
        )
    }
    async fn run_live_post_bytes(&self, path: &str, body: &[u8]) -> Result<Value, RqError> {
        run_req(self.live_post_bytes(path, body).await).await
    }

    async fn live_post(&self, path: &str, body: &Value) -> (RequestBuilder, SemaphorePermit) {
        (
            self.get_client()
                .await
                .post(&format!("{}{}", LIVE_URL, path))
                .header(AUTHORIZATION, self.get_auth_header_value(Live).await)
                .json(body),
            self.rate_limit().await,
        )
    }
    async fn run_live_post(&self, path: &str, body: &Value) -> Result<Value, RqError> {
        run_req(self.live_post(path, body).await).await
    }

    async fn meet_get(&self, path: &str) -> (RequestBuilder, SemaphorePermit) {
        (
            self.get_client()
                .await
                .get(&format!("{}{}", MEET_URL, path))
                .header(AUTHORIZATION, self.get_auth_header_value(Live).await),
            self.rate_limit().await,
        )
    }
    async fn run_meet_get(&self, path: &str) -> Result<Value, RqError> {
        run_req(self.meet_get(path).await).await
    }

    async fn meet_post_bytes(&self, path: &str, body: &[u8]) -> (RequestBuilder, SemaphorePermit) {
        (
            self.get_client()
                .await
                .post(&format!("{}{}", MEET_URL, path))
                .header(AUTHORIZATION, self.get_auth_header_value(Live).await)
                .body(body.to_vec()),
            self.rate_limit().await,
        )
    }
    async fn run_meet_post_bytes(&self, path: &str, body: &[u8]) -> Result<Value, RqError> {
        run_req(self.meet_post_bytes(path, body).await).await
    }

    async fn meet_post(&self, path: &str, body: &Value) -> (RequestBuilder, SemaphorePermit) {
        (
            self.get_client()
                .await
                .post(&format!("{}{}", MEET_URL, path))
                .header(AUTHORIZATION, self.get_auth_header_value(Live).await)
                .json(body),
            self.rate_limit().await,
        )
    }
    async fn run_meet_post(&self, path: &str, body: &Value) -> Result<Value, RqError> {
        run_req(self.meet_post(path, body).await).await
    }
}
