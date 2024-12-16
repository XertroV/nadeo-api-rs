use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{auth::NadeoClient, client::NadeoApiClient};
use std::error::Error;

/// API calls for the Meet API
pub trait MeetApiClient: NadeoApiClient {
    /// <https://webservices.openplanet.dev/meet/cup-of-the-day/current>
    ///
    /// Get the current Cup of the Day
    async fn get_cup_of_the_day(&self) -> Result<CupOfTheDay, Box<dyn Error>> {
        let j = self.run_meet_get("api/cup-of-the-day/current").await?;
        Ok(serde_json::from_value(j)?)
    }
}

impl MeetApiClient for NadeoClient {}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct CupOfTheDay {
    pub id: i64,
    pub edition: i64,
    pub competition: Competition,
    pub challenge: Challenge,
    pub startDate: i64,
    pub endDate: i64,
    pub deletedOn: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct Competition {
    pub id: i64,
    pub liveId: String,
    pub creator: String,
    pub name: String,
    pub participantType: String,
    pub description: Option<String>,
    pub registrationStart: Option<i64>,
    pub registrationEnd: Option<i64>,
    pub startDate: i64,
    pub endDate: i64,
    pub matchesGenerationDate: i64,
    pub nbPlayers: i64,
    pub spotStructure: String,
    pub leaderboardId: i64,
    pub manialink: Option<String>,
    pub rulesUrl: Option<String>,
    pub streamUrl: Option<String>,
    pub websiteUrl: Option<String>,
    pub logoUrl: Option<String>,
    pub verticalUrl: Option<String>,
    pub allowedZones: Vec<Value>,
    pub deletedOn: Option<i64>,
    pub autoNormalizeSeeds: bool,
    pub region: String,
    pub autoGetParticipantSkillLevel: String,
    pub matchAutoMode: String,
    pub partition: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct Challenge {
    pub id: i64,
    pub uid: String,
    pub name: String,
    pub scoreDirection: String,
    pub startDate: i64,
    pub endDate: i64,
    pub status: String,
    pub resultsVisibility: String,
    pub creator: String,
    pub admins: Vec<String>,
    pub nbServers: i64,
    pub autoScale: bool,
    pub nbMaps: i64,
    pub leaderboardId: i64,
    pub deletedOn: Option<i64>,
    pub leaderboardType: String,
    pub completeTimeout: i64,
}

#[cfg(test)]
mod tests {
    use crate::{
        auth::{NadeoClient, UserAgentDetails},
        meet::MeetApiClient,
        test_helpers::get_test_creds,
    };

    // #[ignore]
    #[tokio::test]
    async fn test_get_cup_of_the_day() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, UserAgentDetails::new_autodetect(&email), 10)
            .await
            .unwrap();
        let cup = client.get_cup_of_the_day().await.unwrap();
        assert!(cup.id > 0);
        println!("{:?}", cup);
    }
}
