// API calls for the Live API

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::error::Error;

use crate::{auth::NadeoClient, client::*};

/// Returns query params for length and offset
fn query_lo(length: u32, offset: u32) -> Vec<(&'static str, String)> {
    vec![
        ("length", length.to_string()),
        ("offset", offset.to_string()),
    ]
}

/// API calls for the Live API
pub trait LiveApiClient: NadeoApiClient {
    /// Get TOTDs / Royal maps
    async fn get_monthly_campaign(
        &self,
        ty: MonthlyCampaignType,
        length: u32,
        offset: u32,
    ) -> Result<MonthlyCampaign_List, Box<dyn Error>> {
        let mut query: Vec<(&str, String)> = query_lo(length, offset);
        if matches!(ty, MonthlyCampaignType::Royal) {
            query.push(("royal", "true".to_string()));
        }
        let (rb, permit) = self.live_get("api/token/campaign/month").await;
        let j: Value = rb.query(&query).send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j)?)
    }

    ///
    /// <https://webservices.openplanet.dev/live/leaderboards/top>
    ///
    /// calls `api/token/leaderboard/group/{groupUid}/map/{mapUid}/top`
    async fn get_map_group_leaderboard(
        &self,
        group_uid: &str,
        map_uid: &str,
        only_world: bool,
        length: u32,
        offset: u32,
    ) -> Result<MapGroupLeaderboard, Box<dyn Error>> {
        let mut query = query_lo(length, offset);
        query.push(("onlyWorld", only_world.to_string()));
        let (rb, permit) = self
            .live_get(&format!(
                "api/token/leaderboard/group/{group_uid}/map/{map_uid}/top"
            ))
            .await;
        let j: Value = rb.query(&query).send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j)?)
    }

    /// Personal_Best LB
    ///
    /// <https://webservices.openplanet.dev/live/leaderboards/top>
    ///
    /// calls `api/token/leaderboard/group/Personal_Best/map/{mapUid}/top`
    async fn get_map_leaderboard(
        &self,
        map_uid: &str,
        only_world: bool,
        length: u32,
        offset: u32,
    ) -> Result<MapGroupLeaderboard, Box<dyn Error>> {
        self.get_map_group_leaderboard("Personal_Best", map_uid, only_world, length, offset)
            .await
    }

    /// <https://webservices.openplanet.dev/live/leaderboards/position>
    ///
    /// Note: different groups are supported by the API but not this method.
    /// Note for future: "When using a different groupUid, make sure you're only referencing currently open leaderboards. Maps with closed leaderboards will not be included in the response.""
    ///
    /// calls `api/token/leaderboard/group/map?scores[{mapUid}]={score}`
    async fn get_records_by_time(
        &self,
        uid_scores: &[(&str, i32)],
    ) -> Result<Vec<RecordsByTime>, Box<dyn Error>> {
        let mut query = vec![];
        for (uid, score) in uid_scores.iter() {
            query.push((format!("scores[{}]", uid), score.to_string()));
        }

        let mut body_maps = vec![];
        for (uid, _) in uid_scores.iter() {
            body_maps.push(json!({
                "mapUid": uid,
                "groupUid": "Personal_Best",
            }));
        }
        let body = json!({ "maps": body_maps });

        let (rb, permit) = self
            .live_post("api/token/leaderboard/group/map", &body)
            .await;
        let j: Value = rb.query(&query).send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j)?)
    }

    /// <https://webservices.openplanet.dev/live/leaderboards/surround>
    ///
    /// calls `api/token/leaderboard/group/{groupUid}/map/{mapUid}/surround/{lower}/{upper}?score={score}`
    async fn get_group_surround(
        &self,
        group_uid: &str,
        map_uid: &str,
        lower: i32,
        upper: i32,
        score: u32,
    ) -> Result<RecordsSurround, Box<dyn Error>> {
        let (rb, permit) = self
            .live_get(&format!(
                "api/token/leaderboard/group/{group_uid}/map/{map_uid}/surround/{lower}/{upper}"
            ))
            .await;
        let j: Value = rb
            .query(&[("score", score.to_string())])
            .send()
            .await?
            .json()
            .await?;
        drop(permit);
        Ok(serde_json::from_value(j)?)
    }

    /// Surround on the Personal_Best group
    ///
    /// <https://webservices.openplanet.dev/live/leaderboards/surround>
    ///
    /// calls `api/token/leaderboard/group/Personal_Best/map/{mapUid}/surround/{lower}/{upper}?score={score}`
    async fn get_pb_surround(
        &self,
        map_uid: &str,
        lower: i32,
        upper: i32,
        score: u32,
    ) -> Result<RecordsSurround, Box<dyn Error>> {
        self.get_group_surround("Personal_Best", map_uid, lower, upper, score)
            .await
    }

    /// <https://webservices.openplanet.dev/live/maps/info>
    ///
    /// calls `api/token/map/{mapUid}`
    async fn get_map_info(&self, map_uid: &str) -> Result<MapInfo, Box<dyn Error>> {
        let (rb, permit) = self.live_get(&format!("api/token/map/{map_uid}")).await;
        let j: Value = rb.send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j)?)
    }

    /// <https://webservices.openplanet.dev/live/maps/info-multiple>
    ///
    /// calls `api/token/map/get-multiple?mapUidList={mapUidList}`
    async fn get_map_info_multiple(&self, map_uids: &[&str]) -> Result<MapInfos, Box<dyn Error>> {
        if map_uids.len() > 100 {
            return Err("map_uids length must be <= 100".into());
        }
        let j = self
            .run_live_get(
                &("api/token/map/get-multiple?mapUidList=".to_string() + &map_uids.join(",")),
            )
            .await?;
        Ok(serde_json::from_value(j)?)
    }
}

impl LiveApiClient for NadeoClient {}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapInfos {
    pub mapList: Vec<MapInfo>,
    pub itemCount: i32,
}

/// get_map_info response types
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapInfo {
    pub uid: String,
    pub mapId: String,
    pub name: String,
    pub author: String,
    pub submitter: String,
    pub authorTime: i32,
    pub goldTime: i32,
    pub silverTime: i32,
    pub bronzeTime: i32,
    pub nbLaps: i32,
    pub valid: bool,
    pub downloadUrl: String,
    pub thumbnailUrl: String,
    pub uploadTimestamp: i64,
    pub updateTimestamp: i64,
    pub fileSize: Option<i32>,
    pub public: bool,
    pub favorite: bool,
    pub playable: bool,
    pub mapStyle: String,
    pub mapType: String,
    pub collectionName: String,
}

/// get_group_surround response types
#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsSurround {
    pub groupUid: String,
    pub mapUid: String,
    pub tops: Vec<RecordsSurround_Top>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsSurround_Top {
    pub zoneId: String,
    pub zoneName: String,
    pub top: Vec<RecordsSurround_TopEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsSurround_TopEntry {
    pub accountId: String,
    pub zoneId: String,
    pub zoneName: String,
    pub position: i32,
    pub score: u32,
    pub timestamp: Option<i64>,
}

/// get_records_by_time response types
#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsByTime {
    pub groupUid: String,
    pub mapUid: String,
    pub score: i32,
    pub zones: Vec<RecordsByTime_Zone>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsByTime_Zone {
    pub zoneId: String,
    pub zoneName: String,
    pub ranking: RecordsByTime_ZoneRanking,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsByTime_ZoneRanking {
    pub position: i32,
    pub length: i32,
}

/// Map Group Leaderboard response types
#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapGroupLeaderboard {
    pub groupUid: String,
    pub mapUid: String,
    /// If the map doesn't exist, this has `top` as an empty array for zone World
    pub tops: Vec<MapGroupLeaderboard_Top>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapGroupLeaderboard_Top {
    pub zoneId: String,
    pub zoneName: String,
    pub top: Vec<MapGroupLeaderboard_TopEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapGroupLeaderboard_TopEntry {
    pub accountId: String,
    pub zoneId: String,
    pub zoneName: String,
    pub position: i32,
    pub score: u32,
}

/// TOTD/Royal response types
#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MonthlyCampaign_List {
    pub monthList: Vec<MonthlyCampaign_Month>,
    pub itemCount: i32,
    pub nextRequestTimestamp: i64,
    pub relativeNextRequest: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MonthlyCampaign_Month {
    pub year: i32,
    pub month: i32,
    pub lastDay: i32,
    pub days: Vec<MonthlyCampaign_Day>,
    pub media: MonthlyCampaign_Media,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MonthlyCampaign_Day {
    pub campaignId: i32,
    pub mapUid: String,
    pub day: i32,
    pub monthDay: i32,
    pub seasonUid: String,
    pub leaderboardGroup: Option<String>,
    pub startTimestamp: i64,
    pub endTimestamp: i64,
    pub relativeStart: i64,
    pub relativeEnd: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MonthlyCampaign_Media {
    pub buttonBackgroundUrl: String,
    pub buttonForegroundUrl: String,
    pub decalUrl: String,
    pub popUpBackgroundUrl: String,
    pub popUpImageUrl: String,
    pub liveButtonBackgroundUrl: String,
    pub liveButtonForegroundUrl: String,
}

#[cfg(test)]
mod tests {
    use std::u32;

    use crate::test_helpers::*;
    use auth::{NadeoClient, UserAgentDetails};

    use super::*;
    use crate::*;

    pub static MAP_UIDS: [&str; 5] = [
        "YewzuEnjmnh_ShMW1cX0puuZHcf",
        "HisPAAWhTMTjQPxhMJtMak7Daud",
        "PrometheusByXertroVFtArcher",
        "DeepDip2__The_Gentle_Breeze",
        "DeepDip2__The_Storm_Is_Here",
    ];

    #[ignore]
    #[tokio::test]
    async fn test_monthly_campaign() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, UserAgentDetails::new_autodetect(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_monthly_campaign(MonthlyCampaignType::Royal, 10, 0)
            .await
            .unwrap();
        println!("Monthly Campaign: {:?}", res);
    }

    // test get_map_info_multiple -- works, ignore now
    #[ignore]
    #[tokio::test]
    async fn test_get_map_info_multiple() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, UserAgentDetails::new_autodetect(&email), 10)
            .await
            .unwrap();
        let uids = Vec::from(&MAP_UIDS[..]);
        let res = client.get_map_info_multiple(&uids).await.unwrap();
        println!("Map Info Multiple: {:?}", res);
        for (mi, uid) in res.mapList.iter().zip(MAP_UIDS.iter()) {
            let mi2 = client.get_map_info(uid).await.unwrap();
            assert_eq!(mi, &mi2);
            println!("Matches: {:?} -> {:?}", mi.uid, mi2);
        }
    }

    // test get_map_info for PrometheusByXertroVFtArcher
    #[ignore]
    #[tokio::test]
    async fn test_get_map_info() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, UserAgentDetails::new_autodetect(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_map_info("PrometheusByXertroVFtArcher")
            .await
            .unwrap();
        println!("Map Info: {:?}", res);
    }

    // test surround
    #[ignore]
    #[tokio::test]
    async fn test_surround() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, UserAgentDetails::new_autodetect(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_pb_surround("PrometheusByXertroVFtArcher", 1, 1, u32::MAX)
            .await
            .unwrap();
        println!("Surround: {:?}", res);
    }

    // test get_records_by_time
    #[ignore]
    #[tokio::test]
    async fn test_records_by_time() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, UserAgentDetails::new_autodetect(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_records_by_time(&[
                ("PrometheusByXertroVFtArcher", 47391),
                ("DeepDip2__The_Storm_Is_Here", 60000 * 50),
            ])
            .await
            .unwrap();
        println!("Records by Time: {:?}", res);
    }
}