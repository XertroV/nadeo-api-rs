use std::collections::HashMap;

use log::warn;
use serde_json::Value;

use crate::{auth::OAuthToken, client::NadeoApiClient};

pub trait OAuthApiClient: NadeoApiClient {
    /// For adding API requests, use `get_oauth_permit_and_token`.
    async fn get_oauth_token(&self) -> Result<OAuthToken, String>;

    async fn get_oauth_permit_and_token(
        &self,
    ) -> Result<(OAuthToken, tokio::sync::SemaphorePermit), String> {
        let permit = self.rate_limit().await;
        // wait for permit before getting the token in case of refresh
        let token = self.get_oauth_token().await?;
        Ok((token, permit))
    }

    /// Get Display Names for a list of Uuids
    ///
    /// <https://webservices.openplanet.dev/oauth/reference/accounts/id-to-name>
    ///
    /// calls `GET 	https://api.trackmania.com/api/display-names?accountId[]={accountId}`
    async fn get_display_names<T>(
        &self,
        account_ids: &[T],
    ) -> Result<HashMap<String, String>, String>
    where
        T: Into<String> + Clone,
    {
        match account_ids.len() {
            0 => return Ok(HashMap::new()),
            x if x > 50 => return Err("Too many account ids (max 50)".to_string()),
            _ => (),
        };

        let account_ids = account_ids
            .iter()
            .cloned()
            .map(Into::into)
            .collect::<Vec<String>>();
        let mut account_ids_str = account_ids.join("&accountId[]=");
        account_ids_str.insert_str(0, "accountId[]=");

        let (token, permit) = self.get_oauth_permit_and_token().await?;

        let rb = self
            .oauth_get(
                &format!("display-names?{}", account_ids_str),
                &token,
                &permit,
            )
            .await;

        let resp = rb.send().await.map_err(|e| e.to_string())?;
        drop(permit);

        let status = resp.status().clone();
        let headers = resp.headers().clone();
        let url = resp.url().clone();
        let body = resp.bytes().await.map_err(|e| e.to_string())?;
        // let j: Value = resp.json().await.map_err(|e| e.to_string())?;
        let j: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;

        if j.is_array() && j.as_array().unwrap().is_empty() {
            // then no display names found, we need to return empty strings for all ids
            return Ok(account_ids
                .into_iter()
                .map(|id| (id, "".to_string()))
                .collect());
        }

        // map of WSID -> display name
        let obj = j.as_object().ok_or_else(|| {
            warn!(
                "Bad response from get_display_names\nNames = {:?}\nResponse = {:?} {:?} {:?}",
                &account_ids_str, &status, &url, &headers
            );
            format!("Not a json obj: {:?}", &j)
        })?;
        let mut hm: HashMap<_, _> = obj
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or(k).to_string()))
            .collect();

        // some user ids may not have display names, we add "" for them
        for id in account_ids {
            if !hm.contains_key(&id) {
                hm.insert(id, "".to_string());
            }
        }

        Ok(hm)
    }

    // todo: <https://webservices.openplanet.dev/oauth/reference/accounts/name-to-id>	https://api.trackmania.com/api/display-names/account-ids?displayName[]={accountName}
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;
    use crate::{
        auth::{NadeoClient, OAuthCredentials},
        prelude::UserAgentDetails,
        test_helpers::{get_test_creds, get_test_email},
        user_agent_auto,
    };

    // works
    #[ignore]
    #[tokio::test]
    async fn test_get_display_names() {
        let creds = get_test_creds();
        let client = NadeoClient::create(creds, user_agent_auto!(&get_test_email()), 10)
            .await
            .unwrap();
        let client_id =
            env::var("NADEO_API_TEST_OAUTH_ID").expect("NADEO_API_TEST_OAUTH_ID not set");
        let client_secret =
            env::var("NADEO_API_TEST_OAUTH_SECRET").expect("NADEO_API_TEST_OAUTH_SECRET not set");
        let client = client
            .with_oauth(OAuthCredentials::new(&client_id, &client_secret))
            .unwrap();
        let names_hm = client
            .get_display_names(&vec![
                "5b4d42f4-c2de-407d-b367-cbff3fe817bc",
                "0a2d1bc0-4aaa-4374-b2db-3d561bdab1c9",
            ])
            .await
            .unwrap();
        println!("{:?}", names_hm);
        assert_eq!(names_hm.len(), 2);
        assert_eq!(
            names_hm.get("5b4d42f4-c2de-407d-b367-cbff3fe817bc"),
            Some(&"tooInfinite".to_string())
        );
        assert_eq!(
            names_hm.get("0a2d1bc0-4aaa-4374-b2db-3d561bdab1c9"),
            Some(&"XertroV".to_string())
        );
    }
}
