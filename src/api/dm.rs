use serde::Deserialize;

use crate::{
    Error,
    api::{ApiClient, User},
};

#[derive(Debug, Deserialize, Clone)]
pub struct DM {
    pub id: String,
    pub recipients: Vec<User>,
}

impl DM {
    pub async fn from_user(api_client: &ApiClient) -> Result<Vec<Self>, Error> {
        let url = format!("{}/users/@me/channels", api_client.base_url);
        let response = api_client
            .http_client
            .get(url)
            .header("Authorization", &api_client.auth_token)
            .send()
            .await
            .map_err(|e| format!("API Error: {e}"))?;

        if response.status().is_success() {
            response
                .json::<Vec<Self>>()
                .await
                .map_err(|e| format!("JSON Error: {e}").into())
        } else {
            Err(format!("API Error: {}", response.status()).into())
        }
    }

    pub fn get_name(&self) -> String {
        let mut users = self.recipients.iter().map(|u| u.username.clone());
        let mut name = String::new();

        match users.next() {
            Some(user) => name.push_str(user.as_str()),
            None => return String::new(),
        };

        for user in users {
            name.push_str(format!(", {user}").as_str());
        }

        name
    }
}
