use std::{error::Error, sync::Arc};

use base64::prelude::*;
use chrono::TimeZone;
use reqwest::header::AUTHORIZATION;
use serde_json::{json, Value};
use tokio::sync::{RwLock, Semaphore, SemaphorePermit, TryLockError};

// Handle nadeo authentication
/// Credentials for nadeo services -- create via [::dedicated_server](NadeoCredentials::dedicated_server) or [::ubisoft](NadeoCredentials::ubisoft)
#[derive(Debug)]
pub enum NadeoCredentials {
    /// username and pw
    DedicatedServer { u: String, p: String },
    /// email and pw
    Ubisoft { e: String, p: String },
}

impl NadeoCredentials {
    /// Create credentials from a dedicated server login
    pub fn dedicated_server(username: &str, password: &str) -> Self {
        NadeoCredentials::DedicatedServer {
            u: username.to_string(),
            p: password.to_string(),
        }
    }

    /// Create credentials from a ubisoft login
    ///
    /// ! Not tested
    pub fn ubisoft(email: &str, password: &str) -> Self {
        NadeoCredentials::Ubisoft {
            e: email.to_string(),
            p: password.to_string(),
        }
    }
}

#[derive(Debug)]
pub enum LoginError {
    Misc(String),
    Req(reqwest::Error),
}

impl From<reqwest::Error> for LoginError {
    fn from(e: reqwest::Error) -> Self {
        LoginError::Req(e)
    }
}

impl From<&str> for LoginError {
    fn from(s: &str) -> Self {
        LoginError::Misc(s.to_string())
    }
}

impl From<String> for LoginError {
    fn from(s: String) -> Self {
        LoginError::Misc(s)
    }
}

impl std::error::Error for LoginError {}

impl std::fmt::Display for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginError::Misc(s) => write!(f, "LoginError: {}", s),
            LoginError::Req(e) => write!(f, "LoginError: {}", e),
        }
    }
}

impl NadeoCredentials {
    pub fn get_basic_auth_header(&self) -> String {
        let auth_string = match self {
            NadeoCredentials::DedicatedServer { u, p } => format!("{}:{}", u, p),
            NadeoCredentials::Ubisoft { e, p } => format!("{}:{}", e, p),
        };
        format!("Basic {}", BASE64_STANDARD.encode(auth_string))
    }

    pub fn is_ubi(&self) -> bool {
        matches!(self, NadeoCredentials::Ubisoft { .. })
    }

    pub fn is_dedi(&self) -> bool {
        matches!(self, NadeoCredentials::DedicatedServer { .. })
    }

    pub async fn run_login(&self, client: &reqwest::Client) -> Result<NadeoTokens, LoginError> {
        let req_for_audience_token;
        match self {
            NadeoCredentials::Ubisoft { e, p } => {
                let res = client
                    .post(AUTH_UBI_URL)
                    .basic_auth(e, Some(p))
                    .header("Ubi-AppId", "86263886-327a-4328-ac69-527f0d20a237")
                    .header("Content-Type", "application/json")
                    .send()
                    .await?;
                let body: Value = res.json().await?;
                let ticket = body["ticket"].as_str().ok_or("No ticket in response")?;
                let auth2 = format!("ubi_v1 t={}", ticket);
                req_for_audience_token = client
                    .post(AUTH_UBI_URL2)
                    .header("Authorization", auth2)
                    .header("Content-Type", "application/json");
            }
            NadeoCredentials::DedicatedServer { u, p } => {
                req_for_audience_token = client
                    .post(AUTH_DEDI_URL)
                    .header("Content-Type", "application/json")
                    .basic_auth(u, Some(p));
            }
        };
        let req_core_token = req_for_audience_token
            .try_clone()
            .unwrap()
            .json(&json!({"audience": "NadeoServices"}))
            .send();
        let req_live_token = req_for_audience_token
            .try_clone()
            .unwrap()
            .json(&json!({"audience": "NadeoLiveServices"}))
            .send();
        let (core_token, live_token) = tokio::try_join!(req_core_token, req_live_token)?;
        let core_token_resp: Value = core_token.json().await?;
        let live_token_resp: Value = live_token.json().await?;
        let core_token = NadeoToken::new(NadeoAudience::Core, core_token_resp)?;
        let live_token = NadeoToken::new(NadeoAudience::Live, live_token_resp)?;
        Ok(NadeoTokens::new(core_token, live_token))
    }
}

// first url to post to creds
pub static AUTH_UBI_URL: &str = "https://public-ubiservices.ubi.com/v3/profiles/sessions";
// second url to post audience to
pub static AUTH_UBI_URL2: &str =
    "https://prod.trackmania.core.nadeo.online/v2/authentication/token/ubiservices";
// dedicated server url to post audience to
pub static AUTH_DEDI_URL: &str =
    "https://prod.trackmania.core.nadeo.online/v2/authentication/token/basic";

pub static AUTH_REFRESH_URL: &str =
    "https://prod.trackmania.core.nadeo.online/v2/authentication/token/refresh";

// pub struct UbiAuth {
//     token: String,
// }

/// User agent details for nadeo services; format: `app_name/version (contact_email)`
#[derive(Debug)]
pub struct UserAgentDetails {
    pub app_name: String,
    pub contact_email: String,
    pub version: String,
}

impl UserAgentDetails {
    /// Detects app_name and version from cargo manifest
    pub fn new_autodetect(contact_email: &str) -> Self {
        let app_name = env!("CARGO_PKG_NAME").to_string();
        let version = env!("CARGO_PKG_VERSION").to_string();
        Self {
            app_name,
            contact_email: contact_email.to_string(),
            version,
        }
    }

    pub fn new_autover(app_name: &str, contact_email: &str) -> Self {
        let version = env!("CARGO_PKG_VERSION").to_string();
        Self {
            app_name: app_name.to_string(),
            contact_email: contact_email.to_string(),
            version,
        }
    }

    pub fn new(app_name: &str, contact_email: &str, version: &str) -> Self {
        Self {
            app_name: app_name.to_string(),
            contact_email: contact_email.to_string(),
            version: version.to_string(),
        }
    }

    pub fn get_user_agent_string(&self) -> String {
        format!(
            "{}/{} ({})",
            self.app_name, self.version, self.contact_email
        )
    }
}

#[derive(Debug)]
pub struct NadeoClient {
    credentials: NadeoCredentials,
    core_token: RwLock<NadeoToken>,
    live_token: RwLock<NadeoToken>,
    pub user_agent: UserAgentDetails,
    pub max_concurrent_requests: usize,
    req_semaphore: Arc<Semaphore>,
    client: reqwest::Client,
}

impl NadeoApiClient for NadeoClient {
    async fn get_client(&self) -> &reqwest::Client {
        // todo: check if token is expired / refreshable
        self.ensure_tokens_valid().await;
        &self.client
    }

    fn get_auth_token(&self, audience: NadeoAudience) -> Result<String, TryLockError> {
        Ok(match audience {
            Core => self.core_token.try_read()?.access_token.clone(),
            Live => self.live_token.try_read()?.access_token.clone(),
        })
    }

    async fn aget_auth_token(&self, audience: NadeoAudience) -> String {
        match audience {
            Core => self.core_token.read().await.access_token.clone(),
            Live => self.live_token.read().await.access_token.clone(),
        }
    }

    async fn rate_limit(&self) -> SemaphorePermit {
        self.req_semaphore
            .acquire()
            .await
            .expect("Failed to acquire semaphore")
    }
}

impl NadeoClient {
    pub fn get_authz_header_for_auth(&self) -> String {
        self.credentials.get_basic_auth_header()
    }

    pub async fn create(
        credentials: NadeoCredentials,
        user_agent: UserAgentDetails,
        max_concurrent_requests: usize,
    ) -> Result<Self, Box<dyn Error>> {
        let req_semaphore = Arc::new(Semaphore::new(max_concurrent_requests));
        let client = reqwest::Client::builder()
            .user_agent(user_agent.get_user_agent_string())
            .build()?;

        // run auth
        let tokens = credentials.run_login(&client).await?;

        Ok(Self {
            credentials,
            core_token: RwLock::new(tokens.core),
            live_token: RwLock::new(tokens.live),
            user_agent,
            max_concurrent_requests,
            req_semaphore,
            client,
        })
    }

    pub async fn ensure_tokens_valid(&self) {
        let mut core_token = self.core_token.write().await;
        if core_token.is_access_expired() {
            println!("Core token expired, refreshing");
            core_token.run_refresh(&self.client).await;
        }

        let mut live_token = self.live_token.write().await;
        if live_token.is_access_expired() {
            println!("Live token expired, refreshing");
            live_token.run_refresh(&self.client).await;
        }
    }
}

#[derive(Debug)]
pub enum NadeoAudience {
    Core,
    Live,
    // Meet, // uses Live
}
pub use NadeoAudience::*;

use crate::client::NadeoApiClient;

impl NadeoAudience {
    pub fn get_audience_string(&self) -> &str {
        match self {
            Core => "NadeoServices",
            Live => "NadeoLiveServices",
        }
    }

    pub fn from_audience_string(aud: &str) -> Option<NadeoAudience> {
        match aud {
            "NadeoServices" => Some(Core),
            "NadeoLiveServices" => Some(Live),
            _ => None,
        }
    }
}

// Returns the token body (middle chunk, aka "payload") as a JSON object
pub fn get_token_body(token: &String) -> Result<Value, String> {
    token
        .split(".")
        .nth(1)
        .map_or(Err("Token does not have 3 parts".to_string()), |part| {
            let decoded = BASE64_URL_SAFE_NO_PAD
                .decode(part.as_bytes())
                .map_err(|e| e.to_string())?;
            serde_json::from_slice(&decoded).map_err(|e| e.to_string())
        })
}

// Returns e.g., "NadeoLiveServices"
pub fn get_token_body_audience(body: &Value) -> Result<&str, String> {
    body["aud"]
        .as_str()
        .ok_or("token.aud is not a string".to_string())
}

pub fn get_token_body_expiry(body: &Value) -> Result<i64, String> {
    body["exp"]
        .as_i64()
        .ok_or("token.exp is not a i64".to_string())
        .map(|exp| exp as i64)
}

pub fn get_token_body_refresh_after(body: &Value) -> Result<i64, String> {
    body["rat"]
        .as_i64()
        .ok_or("token.rat is not a i64".to_string())
        .map(|exp| exp as i64)
}

pub fn i64_to_datetime(i: i64) -> Result<chrono::DateTime<chrono::Utc>, String> {
    match chrono::Utc.timestamp_opt(i, 0) {
        chrono::offset::LocalResult::Single(dt) => Ok(dt),
        chrono::offset::LocalResult::Ambiguous(dt, _) => Ok(dt),
        _ => Err("Failed to convert i64 to DateTime".to_string()),
    }
}

pub fn now_dt() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

#[derive(Debug)]
pub struct NadeoTokens {
    core: NadeoToken,
    live: NadeoToken,
}

impl NadeoTokens {
    pub fn new(core: NadeoToken, live: NadeoToken) -> Self {
        Self { core, live }
    }
}

#[derive(Debug)]
pub struct NadeoToken {
    audience: NadeoAudience,
    access_token: String,
    refresh_token: String,
}

impl NadeoToken {
    pub fn new(audience: NadeoAudience, body: Value) -> Result<Self, String> {
        let access_token = body["accessToken"]
            .as_str()
            .ok_or(format!("No accessToken in response; body: {}", body))?
            .to_string();
        let refresh_token = body["refreshToken"]
            .as_str()
            .ok_or("No refreshToken in response")?
            .to_string();
        Ok(Self {
            audience,
            access_token,
            refresh_token,
        })
    }

    /// Returns `nadeo_v1 t=refresh_token` (used to call AUTH_REFRESH_URL)
    pub fn get_refresh_authz_header(&self) -> String {
        format!("nadeo_v1 t={}", self.refresh_token)
    }

    /// Prefer get_*_authz_header instead. Returns `nadeo_v1 t=access_token`
    pub fn get_access_authz_header(&self) -> String {
        format!("nadeo_v1 t={}", self.access_token)
    }

    pub fn get_access_token_body(&self) -> Result<Value, String> {
        get_token_body(&self.access_token)
    }

    pub fn get_refresh_token_body(&self) -> Result<Value, String> {
        get_token_body(&self.refresh_token)
    }

    pub fn get_access_expiry_i64(&self) -> Result<i64, String> {
        get_token_body_expiry(&self.get_access_token_body()?)
    }

    pub fn get_refresh_expiry_i64(&self) -> Result<i64, String> {
        get_token_body_expiry(&self.get_refresh_token_body()?)
    }

    pub fn get_access_expiry(&self) -> Result<chrono::DateTime<chrono::Utc>, String> {
        i64_to_datetime(self.get_access_expiry_i64()?)
    }

    pub fn get_refresh_expiry(&self) -> Result<chrono::DateTime<chrono::Utc>, String> {
        i64_to_datetime(self.get_refresh_expiry_i64()?)
    }

    pub fn is_access_expired(&self) -> bool {
        self.get_access_expiry().map_or(true, |exp| exp < now_dt())
    }

    pub fn is_refresh_expired(&self) -> bool {
        self.get_refresh_expiry().map_or(true, |exp| exp < now_dt())
    }

    /// Returns authz header if audience is core
    pub fn get_core_authz_header(&self) -> Option<String> {
        match self.audience {
            NadeoAudience::Core => Some(self.get_access_authz_header()),
            _ => None,
        }
    }

    /// Returns authz header if audience is live
    pub fn get_live_authz_header(&self) -> Option<String> {
        match self.audience {
            NadeoAudience::Live => Some(self.get_access_authz_header()),
            _ => None,
        }
    }

    // /// Returns authz header if audience is meet
    // pub fn get_meet_authz_header(&self) -> Option<String> {
    //     match self.audience {
    //         NadeoAudience::Meet => Some(self.get_access_authz_header()),
    //         _ => None,
    //     }
    // }

    async fn run_refresh(&mut self, client: &reqwest::Client) {
        let req = client
            .post(AUTH_REFRESH_URL)
            .header(AUTHORIZATION, self.get_refresh_authz_header())
            .send()
            .await
            .unwrap();
        let body: Value = req.json().await.unwrap();
        self.access_token = body["accessToken"].as_str().unwrap().to_string();
        if body["refreshToken"].is_null() {
            // refresh token not returned, keep old one
            println!(
                "Refresh token not returned for audience {:?}",
                self.audience
            );
        } else {
            // todo, test
            self.refresh_token = body["refreshToken"].as_str().unwrap().to_string();
            println!("Updated refresh token for audience {:?}", self.audience);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::test_helpers::*;

    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_login() -> Result<(), Box<dyn Error>> {
        // works for dedicated, disable for now to avoid spam
        return Ok(());

        let cred = get_test_creds();
        let client = reqwest::Client::new();
        let res = cred.run_login(&client).await?;
        println!("{:?}", res);
        Ok(())
    }
}
