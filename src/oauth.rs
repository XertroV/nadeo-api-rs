use std::collections::HashMap;

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
    async fn get_display_names(
        &self,
        account_ids: Vec<String>,
    ) -> Result<HashMap<String, String>, String> {
        let mut account_ids = account_ids.join("&accountId[]=");
        account_ids.insert_str(0, "accountId[]=");

        let (token, permit) = self.get_oauth_permit_and_token().await?;

        let rb = self
            .oauth_get(&format!("display-names?{}", account_ids), &token, &permit)
            .await;

        let resp = rb.send().await.map_err(|e| e.to_string())?;
        let j: Value = resp.json().await.map_err(|e| e.to_string())?;
        drop(permit);

        // map of WSID -> display name
        let obj = j.as_object().ok_or(format!("Not a json obj: {:?}", &j))?;
        Ok(obj
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or(k).to_string()))
            .collect())
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
            .get_display_names(vec![
                "5b4d42f4-c2de-407d-b367-cbff3fe817bc".to_string(),
                "0a2d1bc0-4aaa-4374-b2db-3d561bdab1c9".to_string(),
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
