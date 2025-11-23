use serde::{Deserialize, Serialize};

use crate::api::User;

#[derive(Debug, Deserialize, Clone)]
pub struct GuildMember {
    pub user: User,
    pub roles: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Guild {
    pub id: String,
    pub name: String,
}
