use std::{
    error::Error,
    sync::{Arc, OnceLock},
};

use base64::prelude::*;
use chrono::{DateTime, TimeZone, Utc};
use log::{info, warn};
use reqwest::{
    header::{AUTHORIZATION, CONTENT_TYPE},
    redirect::Policy,
};
use ringbuf::{traits::*, HeapRb};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{
    select,
    sync::{RwLock, Semaphore, SemaphorePermit, TryLockError},
};

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
                    .header(CONTENT_TYPE, "application/json")
                    .send()
                    .await?;
                let body: Value = res.json().await?;
                let ticket = body["ticket"].as_str().ok_or("No ticket in response")?;
                let auth2 = format!("ubi_v1 t={}", ticket);
                req_for_audience_token = client
                    .post(AUTH_UBI_URL2)
                    .header(AUTHORIZATION, auth2)
                    .header(CONTENT_TYPE, "application/json");
            }
            NadeoCredentials::DedicatedServer { u, p } => {
                req_for_audience_token = client
                    .post(AUTH_DEDI_URL)
                    .header(CONTENT_TYPE, "application/json")
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

#[derive(Debug)]
pub struct OAuthCredentials {
    pub client_id: String,
    pub client_secret: String,
}

impl OAuthCredentials {
    pub fn new(client_id: &str, client_secret: &str) -> Self {
        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
        }
    }

    pub async fn run_login(&self, client: &reqwest::Client) -> Result<OAuthToken, reqwest::Error> {
        let res = client
            .post(AUTH_OAUTH_URL)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
            ])
            .send()
            .await?;
        let j: OAuthToken = res.json().await?;
        let _ = j
            .expires_at
            .set(Utc::now() + chrono::Duration::seconds(j.expires_in))
            .expect("Failed to set expires_at");
        Ok(j)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct OAuthToken {
    pub token_type: String,
    pub expires_in: i64,
    pub access_token: String,
    #[serde(skip)]
    pub expires_at: OnceLock<DateTime<Utc>>,
}

impl OAuthToken {
    /// Returns `<bearer> <access_token>`
    pub fn get_authz_header(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }

    pub fn should_refresh(&self) -> bool {
        *self.expires_at.get().unwrap() - chrono::TimeDelta::seconds(60) < Utc::now()
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
// oauth machine-to-machine url to post id,sec to
pub static AUTH_OAUTH_URL: &str = "https://api.trackmania.com/api/access_token";

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

#[macro_export]
macro_rules! user_agent_auto {
    ($email:expr) => {
        UserAgentDetails::new(env!("CARGO_CRATE_NAME"), $email, env!("CARGO_PKG_VERSION"))
    };
}
#[macro_export]
macro_rules! user_agent_auto_ver {
    ($appname:expr, $email:expr) => {
        UserAgentDetails::new($appname, $email, env!("CARGO_PKG_VERSION"))
    };
}

impl UserAgentDetails {
    // /// Detects app_name and version from cargo manifest
    // pub fn new_autodetect(contact_email: &str) -> Self {
    //     let app_name = crate_name!().to_string();
    //     let version = env!("CARGO_PKG_VERSION").to_string();
    //     Self {
    //         app_name,
    //         contact_email: contact_email.to_string(),
    //         version,
    //     }
    // }

    // pub fn new_autover(app_name: &str, contact_email: &str) -> Self {
    //     let version = env!("CARGO_PKG_VERSION").to_string();
    //     Self {
    //         app_name: app_name.to_string(),
    //         contact_email: contact_email.to_string(),
    //         version,
    //     }
    // }

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

#[derive()]
pub struct NadeoClient {
    credentials: NadeoCredentials,
    core_token: RwLock<NadeoToken>,
    live_token: RwLock<NadeoToken>,
    pub user_agent: UserAgentDetails,
    pub max_concurrent_requests: usize,
    req_semaphore: Arc<Semaphore>,
    client: reqwest::Client,
    req_times: RwLock<HeapRb<chrono::DateTime<chrono::Utc>>>,
    nb_reqs: RwLock<usize>,
    last_avg_req_per_sec: RwLock<f64>,
    oauth_credentials: OnceLock<OAuthCredentials>,
    oauth_token: RwLock<Option<OAuthToken>>,
}

impl NadeoApiClient for NadeoClient {
    async fn get_client(&self) -> &reqwest::Client {
        // todo: check if token is expired / refreshable
        self.ensure_tokens_valid().await;
        &self.client
    }

    /// prefer aget_auth_token
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
        let permit = self
            .req_semaphore
            .acquire()
            .await
            .expect("Failed to acquire semaphore");
        self.keep_long_running_rate_limit().await;
        permit
    }
}

// proper defaults: 1.0, 1500
pub const MAX_REQ_PER_SEC: f64 = 3.0;
pub const HIT_MAX_REQ_PER_SEC_WAIT: u64 = 500;

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
            .redirect(Policy::limited(5))
            .build()?;

        // run auth
        let tokens = credentials.run_login(&client).await?;

        let req_times = RwLock::new(HeapRb::new(500));

        Ok(Self {
            credentials,
            core_token: RwLock::new(tokens.core),
            live_token: RwLock::new(tokens.live),
            user_agent,
            max_concurrent_requests,
            req_semaphore,
            client,
            req_times,
            nb_reqs: RwLock::new(0),
            last_avg_req_per_sec: RwLock::new(0.0),
            oauth_credentials: OnceLock::new(),
            oauth_token: RwLock::new(None),
        })
    }

    pub fn with_oauth(self, oauth: OAuthCredentials) -> Option<Self> {
        self.oauth_credentials.set(oauth).ok()?;
        Some(self)
    }

    pub async fn ensure_tokens_valid(&self) {
        let mut core_token = self.core_token.write().await;
        if core_token.is_access_expired() || core_token.can_refresh() {
            info!("Core token expired, refreshing");
            core_token.run_refresh(&self.client).await;
        }

        let mut live_token = self.live_token.write().await;
        if live_token.is_access_expired() || live_token.can_refresh() {
            info!("Live token expired, refreshing");
            live_token.run_refresh(&self.client).await;
        }
    }

    async fn keep_long_running_rate_limit(&self) {
        *self.nb_reqs.write().await += 1;
        let mut req_times = self.req_times.write().await;
        let now = now_dt();
        // if we have made less than 500 requests, just push the current time
        if req_times.occupied_len() < 50 {
            // update avg req per sec
            *self.last_avg_req_per_sec.write().await = match req_times.try_peek() {
                Some(oldest) => {
                    req_times.occupied_len() as f64
                        / (now - *oldest).num_milliseconds().max(1) as f64
                        * 1000.0
                }
                None => 0.0,
            };
            req_times.try_push(now).unwrap();
            return;
        }
        let oldest = req_times.try_peek().unwrap();
        let diff = now - *oldest;
        let req_per_sec =
            req_times.capacity().get() as f64 / diff.num_milliseconds().max(1) as f64 * 1000.0;
        *self.last_avg_req_per_sec.write().await = req_per_sec;
        if req_per_sec > MAX_REQ_PER_SEC {
            // we have an exclusive lock on req_times, so we can sleep and we won't miss any requests
            tokio::time::sleep(std::time::Duration::from_millis(HIT_MAX_REQ_PER_SEC_WAIT)).await;
        }
        req_times.push_overwrite(now_dt());
    }

    pub async fn calc_avg_req_per_sec(&self) -> f64 {
        let req_times = self.req_times.read().await;
        if req_times.is_empty() {
            return 0.0;
        }
        let now = now_dt();
        let diff = now - *req_times.try_peek().unwrap();
        req_times.occupied_len() as f64 / diff.num_milliseconds() as f64 * 1000.0
    }

    pub async fn get_cached_avg_req_per_sec(&self) -> f64 {
        *self.last_avg_req_per_sec.read().await
    }

    pub async fn get_nb_reqs(&self) -> usize {
        *self.nb_reqs.read().await
    }
}

impl OAuthApiClient for NadeoClient {
    async fn get_oauth_token(&self) -> Result<OAuthToken, String> {
        let oauth = self
            .oauth_credentials
            .get()
            .ok_or("No oauth credentials".to_string())?;
        let cached_token: Option<OAuthToken> = self.oauth_token.read().await.as_ref().cloned();
        match cached_token {
            Some(token) => {
                if !token.should_refresh() {
                    return Ok(token);
                }
                let new_token = oauth
                    .run_login(&self.client)
                    .await
                    .map_err(|e| e.to_string())?;
                let _ = self.oauth_token.write().await.replace(new_token.clone());
                Ok(new_token)
            }
            None => {
                let token = oauth
                    .run_login(&self.client)
                    .await
                    .map_err(|e| e.to_string())?;
                self.oauth_token.write().await.replace(token.clone());
                Ok(token)
            }
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

use crate::{client::NadeoApiClient, oauth::OAuthApiClient};

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

    pub fn get_access_refresh_after(&self) -> Result<chrono::DateTime<chrono::Utc>, String> {
        i64_to_datetime(get_token_body_refresh_after(
            &self.get_access_token_body()?,
        )?)
    }

    pub fn get_refresh_refresh_after(&self) -> Result<chrono::DateTime<chrono::Utc>, String> {
        i64_to_datetime(get_token_body_refresh_after(
            &self.get_refresh_token_body()?,
        )?)
    }

    pub fn can_refresh(&self) -> bool {
        self.get_access_refresh_after()
            .map_or(false, |after| after < now_dt())
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

        #[cfg(test)]
        {
            println!("----------------------------");
            println!("{:?} Token before refresh:", self.audience);
            print_token_stuff(&self);
            println!("----------------------------");
        }

        // #[cfg(test)]
        // dbg!(&req);
        let body: Value = req.json().await.unwrap();
        // #[cfg(test)]
        // dbg!(&body);

        self.access_token = body["accessToken"].as_str().unwrap().to_string();
        if body["refreshToken"].is_null() {
            // refresh token not returned, keep old one
            warn!(
                "Refresh token not returned for audience {:?}",
                self.audience
            );
        } else {
            // todo, test
            self.refresh_token = body["refreshToken"].as_str().unwrap().to_string();
            warn!("Updated refresh token for audience {:?}", self.audience);
        }

        #[cfg(test)]
        {
            println!("----------------------------");
            println!("{:?} Token after refresh:", self.audience);
            print_token_stuff(&self);
            println!("----------------------------");
        }
    }
}

#[cfg(test)]
fn print_token_stuff(token: &NadeoToken) {
    println!("Audience: {:?}", token.audience);
    println!("Access token: {}", token.access_token);
    println!("Refresh token: {}", token.refresh_token);
    println!("Access expiry: {:?}", token.get_access_expiry());
    println!("Refresh expiry: {:?}", token.get_refresh_expiry());
    println!(
        "Access refresh after: {:?}",
        token.get_access_refresh_after()
    );
    println!(
        "Refresh refresh after: {:?}",
        token.get_refresh_refresh_after()
    );
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::test_helpers::*;

    use super::*;

    #[ignore]
    #[tokio::test]
    async fn test_login() -> Result<(), Box<dyn Error>> {
        let cred = get_test_creds();
        let client = reqwest::Client::new();
        let res = cred.run_login(&client).await?;
        println!("{:?}", res);
        Ok(())
    }

    #[ignore]
    #[tokio::test]
    async fn test_ubi_login() -> Result<(), Box<dyn Error>> {
        let cred = get_test_ubi_creds();
        let client = reqwest::Client::new();
        let res = cred.run_login(&client).await?;
        println!("{:?}", res);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_file_size() -> Result<(), Box<dyn Error>> {
        let cred = get_test_creds();
        let nc = NadeoClient::create(cred, user_agent_auto!("email"), 2).await?;
        let size = nc
            .get_file_size(
                "https://core.trackmania.nadeo.live/maps/0c90c62a-f3ea-491c-ab86-1245ff575667/file",
            )
            .await?;
        println!("Size: {}", size);
        assert!(size > 0);
        Ok(())
    }

    #[ignore]
    #[tokio::test]
    async fn test_print_token_times() -> Result<(), Box<dyn Error>> {
        let cred = get_test_creds();
        let client = reqwest::Client::new();
        let tokens = cred.run_login(&client).await?;
        println!("----------------------------");
        print_token_stuff(&tokens.core);
        println!("----------------------------");
        print_token_stuff(&tokens.live);
        println!("----------------------------");
        Ok(())
    }

    fn print_token_stuff(token: &NadeoToken) {
        println!("Audience: {:?}", token.audience);
        println!("Access token: {}", token.access_token);
        println!("Refresh token: {}", token.refresh_token);
        println!("Access expiry: {:?}", token.get_access_expiry());
        println!("Refresh expiry: {:?}", token.get_refresh_expiry());
        println!(
            "Access refresh after: {:?}",
            token.get_access_refresh_after()
        );
        println!(
            "Refresh refresh after: {:?}",
            token.get_refresh_refresh_after()
        );
    }
}
