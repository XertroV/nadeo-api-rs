// API calls for the Live API

use ahash::{HashMap, HashMapExt};
use log::warn;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::{fmt, marker::PhantomData, num::NonZero, sync::OnceLock, vec};
use tokio::sync::{oneshot, RwLock};

use crate::auth::LoginError;
use crate::{auth::NadeoClient, client::*};

/// Returns query params for length and offset
fn query_lo(length: u32, offset: u32) -> Vec<(&'static str, String)> {
    vec![
        ("length", length.to_string()),
        ("offset", offset.to_string()),
    ]
}

#[derive(Debug)]
pub enum NadeoError {
    ArgsErr(String),
    ReqwestError(reqwest::Error),
    SerdeJsonWithURL(serde_json::Error, String),
    SerdeJson(serde_json::Error),
    Login(LoginError),
    NotFound,
}

impl From<reqwest::Error> for NadeoError {
    fn from(e: reqwest::Error) -> Self {
        NadeoError::ReqwestError(e)
    }
}

impl From<serde_json::Error> for NadeoError {
    fn from(e: serde_json::Error) -> Self {
        NadeoError::SerdeJson(e)
    }
}

impl From<LoginError> for NadeoError {
    fn from(e: LoginError) -> Self {
        NadeoError::Login(e)
    }
}

/// API calls for the Live API
pub trait LiveApiClient: NadeoApiClient {
    /// Get TOTDs / Royal maps
    async fn get_monthly_campaign(
        &self,
        ty: MonthlyCampaignType,
        length: u32,
        offset: u32,
    ) -> Result<MonthlyCampaign_List, NadeoError> {
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
    ) -> Result<MapGroupLeaderboard, NadeoError> {
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
    ) -> Result<MapGroupLeaderboard, NadeoError> {
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
    ) -> Result<Vec<RecordsByTime>, NadeoError> {
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
    ) -> Result<Vec<RecordsByTime>, NadeoError> {
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
    /// It is highly efficient in terms of API calls provided the calls are initiated within a short (20ms) time frame.
    async fn get_lb_position_by_time_batched(
        &'static self,
        map_uid: &str,
        score: NonZero<u32>,
    ) -> Result<ScoreToPos, oneshot::error::RecvError> {
        if score.get() >= 2147483648 {
            warn!("score >= 2147483648: {}", map_uid);
        }
        let ret_chan = self
            .push_rec_position_req(map_uid, score.get() as i64)
            .await;
        Ok(ret_chan.await?)
    }

    /// Internal method supporting `get_lb_position_by_time_batched`
    async fn push_rec_position_req(
        &'static self,
        map_uid: &str,
        score: i64,
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
    ) -> Result<RecordsSurround, NadeoError> {
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
    ) -> Result<RecordsSurround, NadeoError> {
        self.get_group_surround("Personal_Best", map_uid, lower, upper, score, only_world)
            .await
    }

    /// <https://webservices.openplanet.dev/live/maps/info>
    ///
    /// calls `api/token/map/{mapUid}`
    ///
    /// Returns `None` if the map isn't uploaded
    async fn get_map_info(&self, map_uid: &str) -> Result<Option<MapInfo>, NadeoError> {
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
    async fn get_map_info_multiple(&self, map_uids: &[&str]) -> Result<MapInfos, NadeoError> {
        if map_uids.len() > 100 {
            return Err(NadeoError::ArgsErr("map_uids length must be <= 100".into()));
        }
        let url = "api/token/map/get-multiple?mapUidList=".to_string() + &map_uids.join(",");
        let j = self.run_live_get(&url).await?;
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    /// <https://webservices.openplanet.dev/live/clubs/club>
    ///
    /// calls `/api/token/club/{clubId}`
    async fn get_club_info(&self, club_id: i32) -> Result<ClubInfo, NadeoError> {
        let url = format!("api/token/club/{}", club_id);
        let (rb, permit) = self.live_get(&url).await;
        let j: Value = rb.send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    /// <https://webservices.openplanet.dev/live/clubs/campaign-by-id>
    ///
    /// calls `/api/token/club/{clubId}/campaign/{campaignId}`
    async fn get_club_campaign_by_id(
        &self,
        club_id: i32,
        campaign_id: i32,
    ) -> Result<ClubCampaignById, NadeoError> {
        let url = format!("api/token/club/{}/campaign/{}", club_id, campaign_id);
        let (rb, permit) = self.live_get(&url).await;
        let j: Value = rb.send().await?.json().await?;
        eprintln!("{:?}", j);
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    /// <https://webservices.openplanet.dev/live/clubs/campaigns>
    ///
    /// calls `api/token/club/campaign?length={length}&offset={offset}&name={name}`
    async fn get_club_campaigns(
        &self,
        // club_id: i32,
        length: u32,
        offset: u32,
        name: Option<&str>,
    ) -> Result<ClubCampaignList, NadeoError> {
        let mut query = query_lo(length, offset);
        if let Some(name) = name {
            query.push(("name", name.to_string()));
        }
        let url = format!("api/token/club/campaign");
        let (rb, permit) = self.live_get(&url).await;
        let j: Value = rb.query(&query).send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    /// <https://webservices.openplanet.dev/live/clubs/rooms>
    ///
    /// calls `api/token/club/room?length={length}&offset={offset}&name={name}`
    async fn get_club_rooms(
        &self,
        length: u32,
        offset: u32,
        name: Option<&str>,
    ) -> Result<ClubRoomList, NadeoError> {
        let mut query = query_lo(length, offset);
        if let Some(name) = name {
            query.push(("name", name.to_string()));
        }
        let url = format!("api/token/club/room");
        let (rb, permit) = self.live_get(&url).await;
        let j: Value = rb.query(&query).send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    /// <https://webservices.openplanet.dev/live/clubs/room-by-id>
    ///
    /// calls `/api/token/club/{clubId}/room/{roomId}`
    async fn get_club_room_by_id(
        &self,
        club_id: i32,
        activity_id: i32,
    ) -> Result<ClubRoom, NadeoError> {
        let url = format!("api/token/club/{}/room/{}", club_id, activity_id);
        let (rb, permit) = self.live_get(&url).await;
        let j: Value = rb.send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    async fn edit_club_room_by_id(
        &self,
        club_id: i32,
        activity_id: i32,
        body: &ClubRoom_Room_ForEdit,
    ) -> Result<ClubRoom, NadeoError> {
        let url = format!("api/token/club/{}/room/{}/edit", club_id, activity_id);
        let body =
            serde_json::to_value(body).map_err(|e| NadeoError::SerdeJsonWithURL(e, url.clone()))?;
        let (rb, permit) = self.live_post(&url, &body).await;
        let j: Value = rb.send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    async fn create_club_room(
        &self,
        club_id: i32,
        body: &ClubRoom_Room_ForEdit,
    ) -> Result<ClubRoom, NadeoError> {
        let url = format!("api/token/club/{}/room/create", club_id);
        let body =
            serde_json::to_value(body).map_err(|e| NadeoError::SerdeJsonWithURL(e, url.clone()))?;
        let (rb, permit) = self.live_post(&url, &body).await;
        let j: Value = rb.send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    /// <https://webservices.openplanet.dev/live/clubs/activities>
    ///
    /// calls `/api/token/club/{clubId}/activity?length={length}&offset={offset}&active={active}`
    ///
    /// `active` can only be None if the account is an admin of the club; must be Some(true) if not a member.
    async fn get_club_activities(
        &self,
        club_id: i32,
        length: u32,
        offset: u32,
        active: Option<bool>,
    ) -> Result<ClubActivityList, NadeoError> {
        let mut query = query_lo(length, offset);
        if let Some(active) = active {
            query.push(("active", active.to_string()));
        }
        let url = format!("api/token/club/{}/activity", club_id);
        let (rb, permit) = self.live_get(&url).await;
        let j: Value = rb.query(&query).send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    async fn edit_club_activity(
        &self,
        club_id: i32,
        activity_id: i32,
        public: Option<bool>,
        active: Option<bool>,
    ) -> Result<Value, NadeoError> {
        let url = format!("api/token/club/{}/activity/{}/edit", club_id, activity_id);
        let mut body = serde_json::Value::Object(Default::default());
        if let Some(public) = public {
            body["public"] = serde_json::Value::Bool(public);
        }
        if let Some(active) = active {
            body["active"] = serde_json::Value::Bool(active);
        }
        let (rb, permit) = self.live_post(&url, &body).await;
        let j: Value = rb.send().await?.json().await?;
        drop(permit);
        Ok(j)
    }

    /// <https://webservices.openplanet.dev/live/clubs/clubs>
    ///
    /// calls `/api/token/club?length={length}&offset={offset}&name={name}`
    async fn get_clubs(
        &self,
        length: u32,
        offset: u32,
        name: Option<&str>,
    ) -> Result<ClubList, NadeoError> {
        let mut query = query_lo(length, offset);
        if let Some(name) = name {
            query.push(("name", name.to_string()));
        }
        let url = format!("api/token/club");
        let (rb, permit) = self.live_get(&url).await;
        let j: Value = rb.query(&query).send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    async fn get_clubs_mine(&self, length: u32, offset: u32) -> Result<ClubList, NadeoError> {
        let query = query_lo(length, offset);
        let url = format!("api/token/club/mine");
        let (rb, permit) = self.live_get(&url).await;
        let j: Value = rb.query(&query).send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    async fn become_club_member(
        &self,
        club_id: i32,
        // account_id: &str,
    ) -> Result<ClubMember, NadeoError> {
        let url = format!("api/token/club/{}/member/create", club_id);
        // let body = json!({ "accountId": account_id });
        let (rb, permit) = self.live_post_no_body(&url).await;
        let j: Value = rb.send().await?.json().await?;
        eprintln!("become member resp: {:?}", j);
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    async fn get_club_member(
        &self,
        club_id: i32,
        member_wsid: &str,
    ) -> Result<ClubMember, NadeoError> {
        let url = format!("api/token/club/{}/member/{}", club_id, member_wsid);
        // let body = json!({ "accountId": member_wsid });
        let (rb, permit) = self.live_get(&url).await;
        let j: Value = rb.send().await?.json().await?;
        drop(permit);
        Ok(serde_json::from_value(j).map_err(|e| NadeoError::SerdeJsonWithURL(e, url))?)
    }

    // returns "Club member deleted" on success
    async fn leave_club(&self, club_id: i32, member_wsid: &str) -> Result<String, NadeoError> {
        let url = format!("api/token/club/{}/member/{}/delete", club_id, member_wsid);
        let (rb, permit) = self.live_post_no_body(&url).await;
        let r = rb.send().await?.text().await?;
        drop(permit);
        Ok(r)
    }
}

impl LiveApiClient for NadeoClient {
    async fn push_rec_position_req(
        &'static self,
        map_uid: &str,
        score: i64,
    ) -> oneshot::Receiver<ScoreToPos> {
        let (tx, rx) = oneshot::channel();
        self.batcher_lb_pos_by_time.push(map_uid, score, tx).await;
        self.check_start_batcher_lb_pos_by_time_loop().await;
        rx
    }
}

// MARK: BatcherLbPosByTime

/// High performance batcher for getting leaderboard positions by time.
/// Should be used via [LiveApiClient::get_lb_position_by_time_batched].
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

    pub async fn push(&self, map_uid: &str, score: i64, ret_chan: oneshot::Sender<ScoreToPos>) {
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

    pub async fn get_batch_size_avg(&self) -> (f64, usize) {
        let q = self.queued.read().await;
        (q.avg_batch_size, q.nb_batches)
    }

    pub async fn run_batch<T: LiveApiClient>(&self, api: &T) -> Result<Vec<String>, NadeoError> {
        // L1 start: queued -> in progress
        let mut q = self.queued.write().await;
        let batch = q.take_up_to(50);
        // let uids = batch
        //     .iter()
        //     .map(|(uid, _, _)| uid.clone())
        //     .collect::<HashSet<String>>();

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
    pub score: i64,
    pub pos: Option<i32>,
}
impl ScoreToPos {
    pub fn get_s_p(&self) -> (i64, Option<i32>) {
        (self.score, self.pos)
    }
}

pub struct BatcherLbPosByTimeQueue {
    pub queue: HashMap<String, Vec<(i64, oneshot::Sender<ScoreToPos>)>>,
    pub nb_queued: usize,
    pub nb_in_progress: usize,
    pub avg_batch_size: f64,
    pub nb_batches: usize,
}

impl BatcherLbPosByTimeQueue {
    pub fn new() -> Self {
        Self {
            queue: HashMap::new(),
            nb_queued: 0,
            nb_in_progress: 0,
            avg_batch_size: 0.0,
            nb_batches: 0,
        }
    }

    pub fn push(&mut self, map_uid: &str, score: i64, ret_chan: oneshot::Sender<ScoreToPos>) {
        self.nb_queued += 1;
        self.queue
            .entry(map_uid.to_string())
            .or_insert_with(Vec::new)
            .push((score, ret_chan));
    }

    pub fn take_up_to(&mut self, limit: usize) -> Vec<(String, i64, oneshot::Sender<ScoreToPos>)> {
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
        self.update_avg_batch_size(ret.len());
        ret
    }

    fn update_avg_batch_size(&mut self, batch_size: usize) {
        self.nb_batches += 1;
        self.avg_batch_size = (self.avg_batch_size * (self.nb_batches - 1) as f64
            + batch_size as f64)
            / self.nb_batches as f64;
    }
}

// MARK: Resp Types

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubMember {
    pub accountId: String,
    pub clubId: i32,
    pub role: String,
    pub creationTimestamp: i64,
    pub vip: bool,
    pub moderator: bool,
    pub hasFeatured: bool,
    pub pin: bool,
    pub useTag: bool,
}

impl ClubMember {
    /// returns true if the user has joined the club
    pub fn is_a_member(&self) -> bool {
        self.role != ""
    }

    /// returns true if the user role in the club is Member
    pub fn is_member(&self) -> bool {
        self.role == "Member"
    }

    /// returns true if the user role in the club is Admin
    pub fn is_admin(&self) -> bool {
        self.role == "Admin"
    }

    pub fn is_creator(&self) -> bool {
        self.role == "Creator"
    }

    pub fn is_any_admin(&self) -> bool {
        self.is_admin() || self.is_creator()
    }

    pub fn is_limited_admin(&self) -> bool {
        self.is_creator()
    }
}

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
    pub score: i64,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubActivityList {
    pub activityList: Vec<ClubActivity>,
    pub maxPage: i32,
    pub itemCount: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubActivity {
    pub id: i32,
    pub name: String,
    pub activityType: String,
    pub activityId: i32,
    pub targetActivityId: i32,
    pub campaignId: i32,
    pub position: i32,
    pub public: bool,
    pub active: bool,
    pub externalId: i32,
    pub featured: bool,
    pub password: bool,
    pub itemsCount: i32,
    pub clubId: i32,
    pub editionTimestamp: i64,
    pub creatorAccountId: String,
    pub latestEditorAccountId: String,
    pub mediaUrl: String,
    pub mediaUrlPngLarge: String,
    pub mediaUrlPngMedium: String,
    pub mediaUrlPngSmall: String,
    pub mediaUrlDds: String,
    pub mediaTheme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubInfo {
    pub id: i32,
    pub name: String,
    pub tag: String,
    pub description: String,
    pub authorAccountId: String,
    pub latestEditorAccountId: String,
    pub iconUrl: String,
    pub iconUrlPngLarge: String,
    pub iconUrlPngMedium: String,
    pub iconUrlPngSmall: String,
    pub iconUrlDds: String,
    pub logoUrl: String,
    pub decalUrl: String,
    pub decalUrlPngLarge: String,
    pub decalUrlPngMedium: String,
    pub decalUrlPngSmall: String,
    pub decalUrlDds: String,
    pub screen16x9Url: String,
    pub screen16x9UrlPngLarge: String,
    pub screen16x9UrlPngMedium: String,
    pub screen16x9UrlPngSmall: String,
    pub screen16x9UrlDds: String,
    pub screen64x41Url: String,
    pub screen64x41UrlPngLarge: String,
    pub screen64x41UrlPngMedium: String,
    pub screen64x41UrlPngSmall: String,
    pub screen64x41UrlDds: String,
    pub decalSponsor4x1Url: String,
    pub decalSponsor4x1UrlPngLarge: String,
    pub decalSponsor4x1UrlPngMedium: String,
    pub decalSponsor4x1UrlPngSmall: String,
    pub decalSponsor4x1UrlDds: String,
    pub screen8x1Url: String,
    pub screen8x1UrlPngLarge: String,
    pub screen8x1UrlPngMedium: String,
    pub screen8x1UrlPngSmall: String,
    pub screen8x1UrlDds: String,
    pub screen16x1Url: String,
    pub screen16x1UrlPngLarge: String,
    pub screen16x1UrlPngMedium: String,
    pub screen16x1UrlPngSmall: String,
    pub screen16x1UrlDds: String,
    pub verticalUrl: String,
    pub verticalUrlPngLarge: String,
    pub verticalUrlPngMedium: String,
    pub verticalUrlPngSmall: String,
    pub verticalUrlDds: String,
    pub backgroundUrl: String,
    pub backgroundUrlJpgLarge: String,
    pub backgroundUrlJpgMedium: String,
    pub backgroundUrlJpgSmall: String,
    pub backgroundUrlDds: String,
    pub creationTimestamp: i64,
    pub popularityLevel: i32,
    pub state: String,
    pub featured: bool,
    pub walletUid: String,
    pub metadata: String,
    pub editionTimestamp: i64,
    pub iconTheme: String,
    pub decalTheme: String,
    pub screen16x9Theme: String,
    pub screen64x41Theme: String,
    pub screen8x1Theme: String,
    pub screen16x1Theme: String,
    pub verticalTheme: String,
    pub backgroundTheme: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubCampaignById {
    pub clubDecalUrl: String,
    pub campaignId: i32,
    pub activityId: i32,
    pub campaign: ClubCampaign,
    pub popularityLevel: i32,
    pub publicationTimestamp: i64,
    pub creationTimestamp: i64,
    pub creatorAccountId: String,
    pub latestEditorAccountId: String,
    pub id: i32,
    pub clubId: i32,
    pub clubName: String,
    pub name: String,
    pub mapsCount: i32,
    pub mediaUrl: String,
    pub mediaUrlPngLarge: String,
    pub mediaUrlPngMedium: String,
    pub mediaUrlPngSmall: String,
    pub mediaUrlDds: String,
    pub mediaTheme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubCampaign {
    pub id: i32,
    pub seasonUid: String,
    pub name: String,
    pub color: String,
    pub useCase: i32,
    pub clubId: i32,
    pub leaderboardGroupUid: String,
    pub publicationTimestamp: i64,
    pub startTimestamp: i64,
    pub endTimestamp: i64,
    pub rankingSentTimestamp: Option<i64>,
    // pub year: Option<i32>,
    // pub week: Option<i32>,
    // pub day: Option<i32>,
    // pub monthYear: Option<i32>,
    // pub month: Option<i32>,
    // pub monthDay: Option<i32>,
    pub year: i32,
    pub week: i32,
    pub day: i32,
    pub monthYear: i32,
    pub month: i32,
    pub monthDay: i32,
    pub published: bool,
    pub playlist: Vec<ClubCampaign_PlaylistEntry>,
    pub latestSeasons: Vec<ClubCampaign_LatestSeason>,
    pub categories: Vec<ClubCampaign_Category>,
    pub media: ClubCampaign_Media,
    pub editionTimestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubCampaign_PlaylistEntry {
    pub id: i32,
    pub position: i32,
    pub mapUid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubCampaign_LatestSeason {
    pub uid: String,
    pub name: String,
    pub startTimestamp: i64,
    pub endTimestamp: i64,
    pub relativeStart: i64,
    pub relativeEnd: i64,
    pub campaignId: i32,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubCampaign_Category {
    pub position: i32,
    pub length: i32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubCampaign_Media {
    pub buttonBackgroundUrl: String,
    pub buttonForegroundUrl: String,
    pub decalUrl: String,
    pub popUpBackgroundUrl: String,
    pub popUpImageUrl: String,
    pub liveButtonBackgroundUrl: String,
    pub liveButtonForegroundUrl: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubCampaignList {
    pub clubCampaignList: Vec<ClubCampaignById>,
    pub maxPage: i32,
    pub itemCount: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubRoomList {
    pub clubRoomList: Vec<ClubRoom>,
    pub maxPage: i32,
    pub itemCount: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubRoom {
    pub id: i32,
    pub clubId: i32,
    pub clubName: String,
    pub nadeo: bool,
    pub roomId: Option<i32>,
    pub campaignId: Option<i32>,
    pub playerServerLogin: Option<String>,
    pub activityId: i32,
    pub name: String,
    pub room: ClubRoom_Room,
    pub popularityLevel: i32,
    pub creationTimestamp: i64,
    pub creatorAccountId: String,
    pub latestEditorAccountId: String,
    pub password: bool,
    pub mediaUrl: String,
    pub mediaUrlPngLarge: String,
    pub mediaUrlPngMedium: String,
    pub mediaUrlPngSmall: String,
    pub mediaUrlDds: String,
    pub mediaTheme: String,
    // pub maps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubRoom_Room {
    pub id: Option<i32>,
    pub name: String,
    pub region: Option<String>,
    pub serverAccountId: String,
    pub maxPlayers: i32,
    pub playerCount: i32,
    pub maps: Vec<String>,
    pub script: String,
    pub scalable: bool,
    #[serde(deserialize_with = "empty_list_or_map")]
    pub scriptSettings: HashMap<String, ClubRoom_ScriptSetting>,
    pub serverInfo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubRoom_Room_ForEdit {
    pub name: String,
    pub region: String,
    pub maxPlayersPerServer: i32,
    pub maps: Vec<String>,
    pub script: String,
    pub scalable: bool,
    /// cannot be changed after creation
    pub password: bool,
    pub settings: Vec<ClubRoom_ScriptSetting>,
}

fn empty_list_or_map<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + Default,
    D: Deserializer<'de>,
{
    struct EmptyListOrStruct<T>(PhantomData<fn() -> T>);
    impl<'de, T> Visitor<'de> for EmptyListOrStruct<T>
    where
        T: Deserialize<'de> + Default,
    {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("empty list or struct")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<T, A::Error>
        where
            A: SeqAccess<'de>,
        {
            if seq.next_element::<()>()?.is_some() {
                Err(de::Error::invalid_length(1, &self))
            } else {
                Ok(Default::default())
            }
        }

        fn visit_map<M>(self, map: M) -> Result<T, M::Error>
        where
            M: MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(EmptyListOrStruct(PhantomData))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubRoom_ScriptSetting {
    pub key: String,
    pub value: String,
    pub r#type: String,
}

impl ClubRoom_ScriptSetting {
    pub fn text(key: &str, value: &str) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            r#type: "text".to_string(),
        }
    }

    pub fn integer(key: &str, value: i32) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            r#type: "integer".to_string(),
        }
    }

    pub fn boolean(key: &str, value: bool) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            r#type: "boolean".to_string(),
        }
    }

    pub fn float(key: &str, value: f64) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
            r#type: "float".to_string(),
        }
    }

    pub fn time_limit(value: i32) -> Self {
        Self::integer("S_TimeLimit", value)
    }

    pub fn chat_time(value: i32) -> Self {
        Self::integer("S_ChatTime", value)
    }

    /// Url of the image displayed during the track loading screen
    pub fn loading_screen_image_url(value: &str) -> Self {
        Self::text("S_LoadingScreenImageUrl", value)
    }

    pub fn delay_before_next_map(value: i32) -> Self {
        Self::integer("S_DelayBeforeNextMap", value)
    }

    pub fn warm_up_nb(value: i32) -> Self {
        Self::integer("S_WarmUpNb", value)
    }

    pub fn warm_up_duration(value: i32) -> Self {
        Self::integer("S_WarmUpDuration", value)
    }

    pub fn disable_go_to_map(value: bool) -> Self {
        Self::boolean("S_DisableGoToMap", value)
    }

    /// Url of the image displayed on the checkpoints ground. Override the image set in the Club.
    pub fn deco_image_url_checkpoint(value: &str) -> Self {
        Self::text("S_DecoImageUrl_Checkpoint", value)
    }

    /// Url of the image displayed on the block border. Override the image set in the Club.
    pub fn deco_image_url_sponsor_4x1(value: &str) -> Self {
        Self::text("S_DecoImageUrl_DecalSponsor4x1", value)
    }

    /// Url of the image displayed below the podium and big screen. Override the image set in the Club.
    pub fn deco_image_url_screen_16x1(value: &str) -> Self {
        Self::text("S_DecoImageUrl_Screen16x1", value)
    }

    /// Url of the image displayed on the two big screens. Override the image set in the Club.
    pub fn deco_image_url_screen_16x9(value: &str) -> Self {
        Self::text("S_DecoImageUrl_Screen16x9", value)
    }

    /// Url of the image displayed on the bleachers. Override the image set in the Club.
    pub fn deco_image_url_screen_8x1(value: &str) -> Self {
        Self::text("S_DecoImageUrl_Screen8x1", value)
    }

    /// default: `/api/club/room/:ServerLogin/whoami`
    /// Url of the API route to get the deco image url. You can replace ":ServerLogin" with a login from a server in another club to use its images.
    pub fn deco_image_url_who_am_i(value: &str) -> Self {
        Self::text("S_DecoImageUrl_WhoAmIUrl", value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct ClubList {
    pub clubList: Vec<ClubInfo>,
    pub maxPage: u32,
    pub clubCount: u32,
}

#[cfg(test)]
mod tests {
    use std::{future::Future, u32};

    use crate::test_helpers::*;
    use auth::{NadeoClient, UserAgentDetails};
    use futures::stream::FuturesUnordered;
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
        let _mi2 = client.get_map_info(uids[0]).await.unwrap();
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
            ("PrometheusByXertroVFtArcher", NonZero::new(48220).unwrap()),
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

    const TEST_CLUB_ID: i32 = 46587;
    const TEST_CAMPAIGN_ID: i32 = 38997;
    const TEST_ROOM_ID: i32 = 287220;

    #[ignore]
    #[tokio::test]
    async fn test_get_club_campaigns() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client.get_club_campaigns(100, 4000, None).await.unwrap();
        println!("Club Campaigns: {:?}", res);
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_club_campaign_by_id() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_club_campaign_by_id(TEST_CLUB_ID, TEST_CAMPAIGN_ID)
            .await
            .unwrap();
        println!("Club Campaign: {:?}", res);
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_club_rooms() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client.get_club_rooms(10, 0, None).await.unwrap();
        println!("Club Rooms: {:?}", res);
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_club_room_by_id() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_club_room_by_id(TEST_CLUB_ID, TEST_ROOM_ID)
            .await
            .unwrap();
        println!("Club Room: {:?}", res);
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_club_info() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client.get_club_info(TEST_CLUB_ID).await.unwrap();
        println!("Club Info: {:?}", res);
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_clubs() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client.get_clubs(10, 0, None).await.unwrap();
        println!("Clubs: {:?}", res);
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_clubs_mine() {
        let creds = get_test_ubi_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client.get_clubs_mine(10, 0).await.unwrap();
        println!("Clubs: {:?}", res);
    }

    #[ignore]
    #[tokio::test]
    async fn test_get_club_activities() {
        let creds = get_test_ubi_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let res = client
            .get_club_activities(TEST_CLUB_ID, 10, 0, Some(true))
            .await
            .unwrap();
        eprintln!("Club Activities: {:?}", res);
    }

    #[ignore]
    #[tokio::test]
    async fn test_club_membership() {
        let test_club_id = 150; // ubi nadeo club
        let creds = get_test_ubi_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        let my_wsid = client.get_account_wsid().await;
        let _my_dn = client.get_account_display_name().await;
        eprintln!("My wsid: {}", my_wsid);
        eprintln!("My display name: {:?}", _my_dn);

        let get_membership_status = || async {
            client
                .get_club_member(test_club_id, &my_wsid)
                .await
                .unwrap()
        };

        let m_status_1 = get_membership_status().await;
        eprintln!("Membership status: {:?}", m_status_1);
        let r1 = client
            // .become_club_member(test_club_id, my_wsid)
            .become_club_member(test_club_id)
            .await
            .unwrap();
        eprintln!("Join result: {:?}", r1);
        let m_status_2 = get_membership_status().await;
        eprintln!("Membership status: {:?} -> {:?}", m_status_1, m_status_2);
        let r2 = client.leave_club(test_club_id, &my_wsid).await.unwrap();
        eprintln!("Leave result: {:?}", r2);
        let m_status_3 = get_membership_status().await;
        eprintln!(
            "Membership status: {:?} -> {:?} -> {:?}",
            m_status_1, m_status_2, m_status_3
        );
        eprintln!("Join result: {:?}", r1);
        eprintln!("Leave result: {:?}", r2);
    }
}
