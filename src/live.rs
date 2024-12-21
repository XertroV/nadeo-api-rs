// API calls for the Live API

use ahash::{HashMap, HashMapExt, HashSet};
use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{error::Error, num::NonZero, sync::OnceLock, vec};
use tokio::sync::{oneshot, RwLock};

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
    ///
    /// Warning: duplicate uids with different scores is not supported by the API.
    ///
    /// calls `api/token/leaderboard/group/map?scores[{mapUid}]={score}`
    async fn get_lb_positions_by_time(
        &self,
        uid_scores: &[(&str, NonZero<u32>)],
    ) -> Result<Vec<RecordsByTime>, Box<dyn Error>> {
        self.get_lb_positions_by_time_group(uid_scores, "Personal_Best")
            .await
    }

    /// <https://webservices.openplanet.dev/live/leaderboards/position>
    ///
    /// When using a different groupUid, make sure you're only referencing currently open leaderboards. Maps with closed leaderboards will not be included in the response.
    ///
    /// Warning: duplicate uids with different scores is not supported by the API.
    ///
    /// calls `api/token/leaderboard/group/map?scores[{mapUid}]={score}`
    async fn get_lb_positions_by_time_group(
        &self,
        uid_scores: &[(&str, NonZero<u32>)],
        group_uid: &str,
    ) -> Result<Vec<RecordsByTime>, Box<dyn Error>> {
        let mut query = vec![];
        for (uid, score) in uid_scores.iter() {
            query.push((format!("scores[{}]", uid), score.get().to_string()));
        }

        let mut body_maps = vec![];
        for (uid, _) in uid_scores.iter() {
            body_maps.push(json!({
                "mapUid": uid,
                "groupUid": group_uid,
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

    /// <https://webservices.openplanet.dev/live/leaderboards/position>
    ///
    /// Uses `get_lb_positions_by_time` with an async batching system
    /// to get the position on the leaderboard for a given score.
    async fn get_lb_position_by_time_batched(
        &'static self,
        map_uid: &str,
        score: NonZero<u32>,
    ) -> Result<ScoreToPos, oneshot::error::RecvError> {
        let ret_chan = self
            .push_rec_position_req(map_uid, score.get() as i32)
            .await;
        Ok(ret_chan.await?)
    }

    /// Internal method supporting `get_lb_position_by_time_batched`
    async fn push_rec_position_req(
        &'static self,
        map_uid: &str,
        score: i32,
    ) -> oneshot::Receiver<ScoreToPos>;

    /// <https://webservices.openplanet.dev/live/leaderboards/surround>
    ///
    /// calls `api/token/leaderboard/group/{groupUid}/map/{mapUid}/surround/{lower}/{upper}?score={score}&onlyWorld={onlyWorld}`
    async fn get_group_surround(
        &self,
        group_uid: &str,
        map_uid: &str,
        lower: i32,
        upper: i32,
        score: u32,
        only_world: bool,
    ) -> Result<RecordsSurround, Box<dyn Error>> {
        let (rb, permit) = self
            .live_get(&format!(
                "api/token/leaderboard/group/{group_uid}/map/{map_uid}/surround/{lower}/{upper}"
            ))
            .await;
        let j: Value = rb
            .query(&[
                ("score", score.to_string()),
                ("onlyWorld", only_world.to_string()),
            ])
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
        only_world: bool,
    ) -> Result<RecordsSurround, Box<dyn Error>> {
        self.get_group_surround("Personal_Best", map_uid, lower, upper, score, only_world)
            .await
    }

    /// <https://webservices.openplanet.dev/live/maps/info>
    ///
    /// calls `api/token/map/{mapUid}`
    ///
    /// Returns `None` if the map isn't uploaded
    async fn get_map_info(&self, map_uid: &str) -> Result<Option<MapInfo>, Box<dyn Error>> {
        let (rb, permit) = self.live_get(&format!("api/token/map/{map_uid}")).await;
        let resp = rb.send().await?;
        if resp.status().as_u16() == 404 {
            drop(permit);
            return Ok(None);
        }
        let j: Value = resp.json().await?;
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

impl LiveApiClient for NadeoClient {
    async fn push_rec_position_req(
        &'static self,
        map_uid: &str,
        score: i32,
    ) -> oneshot::Receiver<ScoreToPos> {
        let (tx, rx) = oneshot::channel();
        self.batcher_lb_pos_by_time.push(map_uid, score, tx).await;
        self.check_start_batcher_lb_pos_by_time_loop().await;
        rx
    }
}

// MARK: BatcherLbPosByTime

pub struct BatcherLbPosByTime {
    queued: RwLock<BatcherLbPosByTimeQueue>,
    loop_started: OnceLock<bool>,
}

impl BatcherLbPosByTime {
    pub fn new() -> Self {
        Self {
            queued: RwLock::new(BatcherLbPosByTimeQueue::new()),
            loop_started: OnceLock::new(),
        }
    }

    pub async fn push(&self, map_uid: &str, score: i32, ret_chan: oneshot::Sender<ScoreToPos>) {
        let mut q = self.queued.write().await;
        q.push(map_uid, score, ret_chan);
    }

    pub fn has_loop_started(&self) -> bool {
        self.loop_started.get().is_some()
    }

    pub fn set_loop_started(&self) -> Result<(), bool> {
        self.loop_started.set(true)
    }

    pub async fn is_empty(&self) -> bool {
        self.queued.read().await.nb_queued == 0
    }

    pub async fn nb_queued(&self) -> usize {
        self.queued.read().await.nb_queued
    }

    pub async fn nb_in_progress(&self) -> usize {
        self.queued.read().await.nb_in_progress
    }

    pub async fn run_batch<T: LiveApiClient>(
        &self,
        api: &T,
    ) -> Result<Vec<String>, Box<dyn Error>> {
        // L1 start: queued -> in progress
        let mut q = self.queued.write().await;
        let batch = q.take_up_to(50);
        let uids = batch
            .iter()
            .map(|(uid, _, _)| uid.clone())
            .collect::<HashSet<String>>();

        let batch_size = batch.len();
        q.nb_in_progress += batch_size;
        drop(q);
        // L1 end

        // Do the batch
        let uid_scores: Vec<(&str, _)> = batch
            .iter()
            .map(|(uid, score, _)| (uid.as_str(), NonZero::new(*score as u32).unwrap()))
            .collect();
        let resp = api.get_lb_positions_by_time(&uid_scores).await?;
        if resp.len() != batch_size {
            warn!(
                "[BatcherLbPosByTime] resp.len() != batch_size: {} != {}",
                resp.len(),
                batch_size
            );
        }
        // let b_lookup = batch.into_iter().map(|(uid, time, sender)| (uid, (time, sender))).collect::<HashMap<String, (i32, oneshot::Sender<Option<i32>>)>>();
        let r_lookup = resp
            .into_iter()
            .filter_map(|r| {
                let pos = r.get_global_pos()?;
                Some((r.mapUid, pos))
            })
            .collect::<HashMap<String, i32>>();

        let mut uids = vec![];
        for (uid, score, sender) in batch {
            let pos = r_lookup.get(&uid).copied();
            let _ = sender.send(ScoreToPos { score, pos });
            uids.push(uid);
        }

        // L2 start: in progress -> done
        let mut q = self.queued.write().await;
        q.nb_in_progress -= batch_size;
        drop(q);
        // L2 end
        Ok(uids)
    }
}

#[derive(Debug, Clone)]
pub struct ScoreToPos {
    pub score: i32,
    pub pos: Option<i32>,
}
impl ScoreToPos {
    pub fn get_s_p(&self) -> (i32, Option<i32>) {
        (self.score, self.pos)
    }
}

pub struct BatcherLbPosByTimeQueue {
    pub queue: HashMap<String, Vec<(i32, oneshot::Sender<ScoreToPos>)>>,
    pub nb_queued: usize,
    pub nb_in_progress: usize,
}

impl BatcherLbPosByTimeQueue {
    pub fn new() -> Self {
        Self {
            queue: HashMap::new(),
            nb_queued: 0,
            nb_in_progress: 0,
        }
    }

    pub fn push(&mut self, map_uid: &str, score: i32, ret_chan: oneshot::Sender<ScoreToPos>) {
        self.nb_queued += 1;
        self.queue
            .entry(map_uid.to_string())
            .or_insert_with(Vec::new)
            .push((score, ret_chan));
    }

    pub fn take_up_to(&mut self, limit: usize) -> Vec<(String, i32, oneshot::Sender<ScoreToPos>)> {
        let mut ret = vec![];
        let mut to_rem = vec![];
        for (map_uid, v) in self.queue.iter_mut() {
            match v.pop() {
                Some((score, ret_chan)) => {
                    self.nb_queued -= 1;
                    ret.push((map_uid.clone(), score, ret_chan));
                }
                None => {}
            }
            if v.is_empty() {
                to_rem.push(map_uid.clone());
            }
            if ret.len() >= limit {
                break;
            }
        }
        for k in to_rem {
            let v = self.queue.remove(&k);
            if let Some(v) = v {
                self.nb_queued -= v.len();
                if v.len() > 0 {
                    panic!("v.len() > 0: {}: {:?}", k, v);
                }
            }
        }
        ret
    }
}

// MARK: Resp Types

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapInfos {
    pub mapList: Vec<MapInfo>,
    pub itemCount: i32,
}

/// get_map_info response types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapInfo {
    pub uid: String,
    pub mapId: String,
    pub name: String,
    pub author: String,
    pub submitter: String,
    pub authorTime: i64,
    pub goldTime: i64,
    pub silverTime: i64,
    pub bronzeTime: i64,
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
    /// usually blank or same as mapType
    pub mapStyle: String,
    /// Typically "TrackMania\\TM_Race"
    pub mapType: String,
    pub collectionName: String,
}

/// get_group_surround response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsSurround {
    pub groupUid: String,
    pub mapUid: String,
    pub tops: Vec<RecordsSurround_Top>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsSurround_Top {
    pub zoneId: String,
    pub zoneName: String,
    pub top: Vec<RecordsSurround_TopEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsSurround_TopEntry {
    pub accountId: String,
    pub zoneId: String,
    pub zoneName: String,
    pub position: i32,
    pub score: u32,
    pub timestamp: Option<i64>,
}

/// get_lb_positions_by_time response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsByTime {
    pub groupUid: String,
    pub mapUid: String,
    pub score: i32,
    pub zones: Vec<RecordsByTime_Zone>,
}

impl RecordsByTime {
    pub fn get_global_pos(&self) -> Option<i32> {
        Some(self.zones.get(0)?.ranking.position)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsByTime_Zone {
    pub zoneId: String,
    pub zoneName: String,
    pub ranking: RecordsByTime_ZoneRanking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct RecordsByTime_ZoneRanking {
    pub position: i32,
    pub length: i32,
}

/// Map Group Leaderboard response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapGroupLeaderboard {
    pub groupUid: String,
    pub mapUid: String,
    /// If the map doesn't exist, this has `top` as an empty array for zone World
    pub tops: Vec<MapGroupLeaderboard_Top>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapGroupLeaderboard_Top {
    pub zoneId: String,
    pub zoneName: String,
    pub top: Vec<MapGroupLeaderboard_TopEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MapGroupLeaderboard_TopEntry {
    pub accountId: String,
    pub zoneId: String,
    pub zoneName: String,
    pub position: i32,
    /// Can be negative if map has secret records
    pub score: i64,
}

/// TOTD/Royal response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MonthlyCampaign_List {
    pub monthList: Vec<MonthlyCampaign_Month>,
    pub itemCount: i32,
    pub nextRequestTimestamp: i64,
    pub relativeNextRequest: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct MonthlyCampaign_Month {
    pub year: i32,
    pub month: i32,
    pub lastDay: i32,
    pub days: Vec<MonthlyCampaign_Day>,
    pub media: MonthlyCampaign_Media,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    use std::{future::Future, u32};

    use crate::test_helpers::*;
    use auth::{NadeoClient, UserAgentDetails};
    use futures::stream::{FuturesOrdered, FuturesUnordered};
    use lazy_static::lazy_static;
    use oneshot::error::RecvError;

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
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
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
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let uids = Vec::from(&MAP_UIDS[..]);
        let res = client.get_map_info_multiple(&uids).await.unwrap();
        println!("Map Info Multiple: {:?}", res);
        println!(
            "get_cached_avg_req_per_sec: {:?}",
            client.get_cached_avg_req_per_sec().await
        );
        for (mi, uid) in res.mapList.iter().zip(MAP_UIDS.iter()) {
            let mi2 = client.get_map_info(uid).await.unwrap().unwrap();
            assert_eq!(mi, &mi2);
            println!("Matches: {:?} -> {:?}", mi.uid, mi2);
            println!(
                "get_cached_avg_req_per_sec: {:?}",
                client.get_cached_avg_req_per_sec().await
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        let mi2 = client.get_map_info(uids[0]).await.unwrap();
        println!(
            "get_cached_avg_req_per_sec: {:?}",
            client.get_cached_avg_req_per_sec().await
        );
    }

    // test get_map_info for PrometheusByXertroVFtArcher
    #[ignore]
    #[tokio::test]
    async fn test_get_map_info() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_map_info("PrometheusByXertroVFtArcher")
            .await
            .unwrap()
            .unwrap();
        println!("Map Info: {:?}", res);
        let res = client
            .get_map_info("XXXXetheusByXertroVFtArcher")
            .await
            .unwrap();
        assert_eq!(res, None);
        println!(
            "missing map info was None (good). get_cached_avg_req_per_sec: {:?}",
            client.get_cached_avg_req_per_sec().await
        );
    }

    // test surround
    #[ignore]
    #[tokio::test]
    async fn test_surround() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_pb_surround("PrometheusByXertroVFtArcher", 0, 0, u32::MAX, true)
            .await
            .unwrap();
        println!("Surround: {:?}", res);
    }

    // test get_lb_positions_by_time
    #[ignore]
    #[tokio::test]
    async fn test_records_by_time() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_lb_positions_by_time(&[
                ("PrometheusByXertroVFtArcher", NonZero::new(47391).unwrap()),
                (
                    "DeepDip2__The_Storm_Is_Here",
                    NonZero::new(60000 * 50).unwrap(),
                ),
            ])
            .await
            .unwrap();
        println!("Records by Time: {:?}", res);
    }

    lazy_static! {
        static ref CLIENT: OnceLock<NadeoClient> = OnceLock::new();
    }

    // test get_lb_positions_by_time batcher
    // #[ignore]
    #[tokio::test]
    async fn test_records_by_time_batched() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client: NadeoClient = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        if CLIENT.set(client).is_err() {
            panic!("CLIENT already set");
        }
        let client = CLIENT.get().unwrap();
        let to_get = vec![
            ("PrometheusByXertroVFtArcher", NonZero::new(1000).unwrap()),
            ("DeepDip2__The_Storm_Is_Here", NonZero::new(1).unwrap()),
            ("PrometheusByXertroVFtArcher", NonZero::new(47391).unwrap()),
            ("PrometheusByXertroVFtArcher", NonZero::new(48391).unwrap()),
            (
                "DeepDip2__The_Storm_Is_Here",
                NonZero::new(60000 * 50).unwrap(),
            ),
        ];

        fn run_and_time<F: Future<Output = Result<ScoreToPos, RecvError>> + Send + 'static>(
            f: F,
        ) -> tokio::task::JoinHandle<ScoreToPos> {
            tokio::spawn(async move {
                let start = std::time::Instant::now();
                let r = f.await.unwrap();
                let end = std::time::Instant::now();
                println!("Time: {:?}; Res: {:?}", end - start, r);
                r
            })
        }

        let reqs = to_get
            .into_iter()
            .map(|s| client.get_lb_position_by_time_batched(s.0, s.1))
            .map(run_and_time)
            .collect::<FuturesUnordered<_>>();
        let r = futures::future::join_all(reqs).await;
        println!("Results: {:?}", r);
    }

    #[ignore]
    #[tokio::test]
    async fn test_long_running_refresh_token() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 1)
            .await
            .unwrap();
        let start = chrono::Utc::now();
        println!("Long running test -- starting at {:?}", start);
        let n = 200;
        for i in 0..n {
            println!("Running iteration {} / {}", i + 1, n);
            println!("\n\n--- token below? ---\n\n");
            let r = client
                .get_map_leaderboard("PrometheusByXertroVFtArcher", true, 10, i * 10)
                .await
                .unwrap();
            println!("\n\n--- end ---\n\n");
            println!(
                "Response: {}",
                r.tops[0]
                    .top
                    .iter()
                    .map(|t| format!("{}. {} -- {} ms", t.position, t.zoneName, t.score))
                    .collect::<Vec<String>>()
                    .join("\n")
            );
            if i == n - 1 {
                println!(
                    "Last iter. Duration so far: {:?}",
                    chrono::Utc::now() - start
                );
                break;
            }
            println!("Sleeping for 10 minutes @ {:?}", chrono::Utc::now());
            tokio::time::sleep(tokio::time::Duration::from_secs(60 * 10)).await;
            println!(
                "Next iter. Duration so far: {:?} @ {:?}",
                chrono::Utc::now() - start,
                chrono::Utc::now()
            );
        }
    }
}
