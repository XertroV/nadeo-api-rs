use std::{collections::HashMap, error::Error, sync::OnceLock};

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use crate::{auth::NadeoClient, client::NadeoApiClient};

lazy_static! {
    static ref ZONES_INFO: OnceLock<ZoneTree> = OnceLock::new();
}

/// API calls for the Core API
pub trait CoreApiClient: NadeoApiClient {
    /// Get a list of zones
    ///
    /// <https://webservices.openplanet.dev/core/meta/zones>
    async fn get_zones(&self) -> Result<Vec<Zone>, Box<dyn Error>> {
        let j = self.run_core_get("zones").await?;
        Ok(serde_json::from_value(j)?)
    }

    /// Get the zone tree -- cached!
    async fn get_zone_tree(&self) -> Result<&ZoneTree, Box<dyn Error>> {
        if ZONES_INFO.get().is_none() {
            let zones = self.get_zones().await?;
            let zt = ZoneTree::new(zones);
            ZONES_INFO.set(zt).unwrap();
        }
        Ok(ZONES_INFO.get().unwrap())
    }
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
}

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
        for zone in zones {
            z.insert(zone.zoneId.clone(), zone.clone());
            if zone.name == "World" {
                world_id = zone.zoneId.clone();
            }
            if let Some(parent) = zone.parentId {
                c.entry(parent)
                    .or_insert_with(Vec::new)
                    .push(zone.zoneId.clone());
            }
        }
        if world_id.is_empty() {
            panic!("No World zone found");
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
        // let zones = client.get_zones().await.unwrap();

        let _t1 = std::time::Instant::now();
        let _zt = client.get_zone_tree().await.unwrap();
        let _t2 = std::time::Instant::now();
        let _zt2 = client.get_zone_tree().await.unwrap();
        let _t3 = std::time::Instant::now();
        assert!(_zt2.zones.len() > 1000);
        println!("World: {:#?}", _zt2.get_zone(&_zt2.world_id).unwrap());
        println!(
            "Children: {:#?}",
            _zt2.get_children(&_zt2.world_id).unwrap()
        );
        // dbg!(zt);
    }
}
