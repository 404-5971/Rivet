use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Emoji {
    pub id: String,
    pub name: String,
    pub animated: Option<bool>,
}
