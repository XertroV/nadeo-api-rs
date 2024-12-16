use std::error::Error;

use serde::{Deserialize, Serialize};

use crate::{auth::NadeoClient, client::NadeoApiClient};

/// API calls for the Core API
pub trait CoreApiClient: NadeoApiClient {
    /// Get a list of zones
    ///
    /// <https://webservices.openplanet.dev/core/meta/zones>
    async fn get_zones(&self) -> Result<Vec<Zone>, Box<dyn Error>> {
        let j = self.run_core_get("zones").await?;
        Ok(serde_json::from_value(j)?)
    }
}

impl CoreApiClient for NadeoClient {}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct Zone {
    pub icon: String,
    pub name: String,
    pub parentId: Option<String>,
    pub timestamp: String,
    pub zoneId: String,
}

#[cfg(test)]
mod tests {
    use crate::{
        auth::{NadeoClient, UserAgentDetails},
        core::CoreApiClient,
        test_helpers::get_test_creds,
    };

    #[ignore]
    #[tokio::test]
    async fn test_get_zones() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, UserAgentDetails::new_autodetect(&email), 10)
            .await
            .unwrap();
        let zones = client.get_zones().await.unwrap();
        assert!(zones.len() > 0);
    }
}
