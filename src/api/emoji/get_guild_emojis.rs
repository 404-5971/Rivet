use reqwest::Client;

use crate::{Error, api::DISCORD_API_BASE_URL, model::emoji::Emoji};

pub async fn get_guild_emojis(
    client: &Client,
    guild_id: &str,
    token: &str,
) -> Result<Vec<Emoji>, Error> {
    let url = format!("{DISCORD_API_BASE_URL}/guilds/{guild_id}/emojis");

    let response = client
        .get(url)
        .header("Authorization", token)
        .send()
        .await?
        .error_for_status()?;

    let emojis: Vec<Emoji> = response.json().await?;

    Ok(emojis)
}
