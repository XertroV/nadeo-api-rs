//! Create a [NadeoClient](auth::NadeoClient) to interact with the Nadeo API.
//!
//! Use [NadeoClient::create](auth::NadeoClient::create) to create a new client.
//!
//! You will also need to create [NadeoCredentials](auth::NadeoCredentials) and [UserAgentDetails](auth::UserAgentDetails) (see [user_agent_auto] and [user_agent_auto_ver])
//!
//! API methods are defined on traits: [live::LiveApiClient], [meet::MeetApiClient], [core::CoreApiClient].

pub mod auth;
#[allow(async_fn_in_trait)]
pub mod client;
#[allow(async_fn_in_trait)]
pub mod core;
#[allow(async_fn_in_trait)]
pub mod live;
#[allow(async_fn_in_trait)]
pub mod meet;
#[allow(async_fn_in_trait)]
pub mod oauth;
pub mod prelude;
#[cfg(test)]
pub mod test_helpers;
pub mod urls;
