///! # Core API Calls
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use crate::{auth::NadeoClient, client::NadeoApiClient, live::NadeoError};

lazy_static! {
    static ref ZONES_INFO: OnceLock<ZoneTree> = OnceLock::new();
}

/// API calls for the Core API
pub trait CoreApiClient: NadeoApiClient {
    /// Get a list of zones
    ///
    /// <https://webservices.openplanet.dev/core/meta/zones>
    async fn get_zones(&self) -> Result<Vec<Zone>, NadeoError> {
        let j = self.run_core_get("zones").await?;
        Ok(serde_json::from_value(j)?)
    }

    /// Get the zone tree -- cached!
    async fn get_zone_tree(&self) -> Result<&ZoneTree, NadeoError> {
        if ZONES_INFO.get().is_none() {
            let zones = self.get_zones().await?;
            let zt = ZoneTree::new(zones);
            let _ = ZONES_INFO.set(zt);
        }
        Ok(ZONES_INFO.get().unwrap())
    }

    /// Get players' zone details
    ///
    /// <https://webservices.openplanet.dev/core/accounts/zones>
    ///
    /// calls `/accounts/zones/?accountIdList={accountIdList}`
    async fn get_user_zones<T: Into<String> + Clone>(
        &self,
        player_ids: &[T],
    ) -> Result<HashMap<String, PlayerZone>, NadeoError> {
        let mut zones = HashMap::new();
        if player_ids.is_empty() {
            panic!("No player ids given");
            // return Ok(zones);
        }
        let player_ids = player_ids
            .iter()
            .cloned()
            .map(Into::into)
            .collect::<Vec<String>>()
            .join(",");
        let j = self
            .run_core_get(&format!("accounts/zones/?accountIdList={}", player_ids))
            .await?;
        let j = j
            .as_array()
            .ok_or_else(|| {
                panic!(
                    "Response was not an array. ids = {}, j = {:?}",
                    player_ids, j
                )
            })
            .unwrap();
        for data in j {
            let pz = serde_json::from_value::<PlayerZone>(data.clone())?;
            zones.insert(pz.account_id.clone(), pz);
        }
        Ok(zones)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerZone {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "zoneId")]
    pub zone_id: String,
    pub timestamp: String,
}

impl CoreApiClient for NadeoClient {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types, non_snake_case)]
pub struct Zone {
    pub icon: String,
    pub name: String,
    pub parentId: Option<String>,
    pub timestamp: String,
    pub zoneId: String,
    pub depth: Option<u32>,
}

/// The zone tree, constructed and cached from the list of zones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneTree {
    pub world_id: String,
    pub zones: HashMap<String, Zone>,
    pub children: HashMap<String, Vec<String>>,
}

impl ZoneTree {
    pub fn new(zones: Vec<Zone>) -> Self {
        let mut z = HashMap::new();
        let mut c = HashMap::new();
        let mut world_id = String::new();
        for mut zone in zones.into_iter() {
            if zone.name == "World" {
                world_id = zone.zoneId.clone();
                zone.depth = Some(0);
            }
            if let Some(parent) = zone.parentId.clone() {
                c.entry(parent)
                    .or_insert_with(Vec::new)
                    .push(zone.zoneId.clone());
            }
            z.insert(zone.zoneId.clone(), zone);
        }
        if world_id.is_empty() {
            panic!("No World zone found");
        }
        let mut edge = vec![world_id.clone()];
        let mut visited = HashSet::new();
        while !edge.is_empty() {
            let mut new_edge = Vec::new();
            for zone_id in edge {
                if let Some(children) = c.get(&zone_id) {
                    for child in children {
                        if visited.contains(child) {
                            continue;
                        }
                        visited.insert(child);
                        new_edge.push(child.clone());
                        let child_depth = z
                            .get(&zone_id)
                            .expect(&format!("has key: {} (child: {})", zone_id, child))
                            .depth
                            .expect(&format!("has key depth: {} (child: {})", zone_id, child))
                            + 1;
                        if let Some(node) = z.get_mut(child) {
                            node.depth = Some(child_depth);
                        } else {
                            panic!("Child zone not found: {}", child);
                        }
                    }
                }
            }
            edge = new_edge;
        }

        Self {
            world_id,
            zones: z,
            children: c,
        }
    }

    pub fn get_zone(&self, zone_id: &str) -> Option<&Zone> {
        self.zones.get(zone_id)
    }

    pub fn get_children(&self, zone_id: &str) -> Option<&Vec<String>> {
        self.children.get(zone_id)
    }

    /// If the zone is a region, province, state, etc, ascend the tree to get the country.
    /// If the zone is a country, continent, or world, return it.
    pub fn get_country_or_higher(&self, zone_id: &str) -> Option<&Zone> {
        let mut id = zone_id;
        while let Some(zone) = self.get_zone(id) {
            if zone.depth.unwrap() <= 2 {
                return Some(zone);
            }
            if let Some(parent) = &zone.parentId {
                id = parent;
            } else {
                break;
            }
        }
        None
    }

    pub fn world(&self) -> &Zone {
        self.get_zone(&self.world_id).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        auth::{NadeoClient, UserAgentDetails},
        core::CoreApiClient,
        test_helpers::get_test_creds,
        user_agent_auto,
    };

    #[ignore]
    #[tokio::test]
    async fn test_get_zones() {
        let creds = get_test_creds();
        let email = std::env::var("NADEO_TEST_UA_EMAIL").unwrap();
        let client = NadeoClient::create(creds, user_agent_auto!(&email), 10)
            .await
            .unwrap();
        // let zones = client.get_zones().await.unwrap();

        let _t1 = std::time::Instant::now();
        let _zt = client.get_zone_tree().await.unwrap();
        let _t2 = std::time::Instant::now();
        let _zt2 = client.get_zone_tree().await.unwrap();
        let _t3 = std::time::Instant::now();
        assert!(_zt2.zones.len() > 1000);
        println!("World: {:?}", _zt2.get_zone(&_zt2.world_id).unwrap());
        println!("Children: {:?}", _zt2.get_children(&_zt2.world_id).unwrap());

        let user_zones = client
            .get_user_zones(&[
                "5b4d42f4-c2de-407d-b367-cbff3fe817bc",
                "0a2d1bc0-4aaa-4374-b2db-3d561bdab1c9",
            ])
            .await
            .unwrap();
        println!("{:?}", user_zones);
        assert_eq!(user_zones.len(), 2);
    }
}
