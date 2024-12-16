use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    auth::NadeoClient,
    client::{run_req, NadeoApiClient},
};
use std::error::Error;

/// API calls for the Meet API
pub trait MeetApiClient: NadeoApiClient {
    /// <https://webservices.openplanet.dev/meet/cup-of-the-day/current>
    ///
    /// Get the current Cup of the Day. Sometimes the status will be 204 No Content. (Indicated by Ok(None))
    async fn get_cup_of_the_day(&self) -> Result<Option<CupOfTheDay>, Box<dyn Error>> {
        let (rb, permit) = self.meet_get("api/cup-of-the-day/current").await;
        let resp = rb.send().await?;
        if resp.status().as_u16() == 204 {
            return Ok(None);
        }
        let j = resp.json().await?;
        drop(permit);
        Ok(Some(serde_json::from_value(j)?))
    }
}

impl MeetApiClient for NadeoClient {}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    use serde_json::Value;

    use crate::{
        auth::{NadeoClient, UserAgentDetails},
        client::NadeoApiClient,
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
        // let r = client.meet_get("api/cup-of-the-day/current").await;
        // let resp = r.0.send().await.unwrap();
        // println!("{:?}", resp);
        // let b = resp.bytes().await.unwrap();
        // println!("bytes {:?}", b);
        // let j: Value = resp.json().await.unwrap();
        // println!("json {:?}", j);
        let cup = client.get_cup_of_the_day().await.unwrap();
        println!("{:?}", cup);
        match cup {
            Some(c) => {
                assert!(c.id > 0);
                println!("Cup of the Day: {}", c.competition.name);
            }
            None => {
                println!("No Cup of the Day (confirm it's working then ignore this test)");
            }
        }
    }
}
