//! Create a [NadeoClient](auth::NadeoClient) to interact with the Nadeo API.
//!
//! Use [NadeoClient::create](auth::NadeoClient::create) to create a new client.
//!
//! You will also need to create [NadeoCredentials](auth::NadeoCredentials) and [UserAgentDetails](auth::UserAgentDetails) (see [auth::UserAgentDetails::new_autodetect])
//!
//! API methods are defined on traits: [live::LiveApiClient], [meet::MeetApiClient], [core::CoreApiClient].

pub mod auth;
pub mod client;
pub mod core;
pub mod live;
pub mod meet;
// #[cfg(test)]
pub mod test_helpers;
pub mod urls;
