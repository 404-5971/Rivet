use serde::Deserialize;

use crate::{
    Error,
    api::{ApiClient, Guild},
};

#[derive(Debug, Deserialize, Clone)]
pub struct User {
    //pub id: Snowflake,
    pub username: String,
    //pub discriminator: String,
    //pub global_name: Option<String>,
    //pub avatar : Option<String>,
    //pub bot: Option<bool>,
}

impl User {
    pub async fn get_guilds(api_client: &ApiClient) -> Result<Vec<Guild>, Error> {
        let url = format!("{}/users/@me/guilds", api_client.base_url);
        let response = api_client
            .http_client
            .get(url)
            .header("Authorization", &api_client.auth_token)
            .send()
            .await
            .map_err(|e| format!("API Error: {e}"))?;

        if response.status().is_success() {
            response
                .json::<Vec<Guild>>()
                .await
                .map_err(|e| format!("JSON Error: {e}").into())
        } else {
            Err(format!("API Error: {}", response.status()).into())
        }
    }
}
