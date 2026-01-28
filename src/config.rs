use std::path::PathBuf;

const DEFAULT_EMOJIS_JSON: &str = include_str!("../emojis.json");

fn get_config_path() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push("rivetui");
    path.push("emojis.json");
    Some(path)
}

fn parse_emoji_content(content: &str) -> Vec<(String, String)> {
    match serde_json::from_str::<Vec<(String, String)>>(content) {
        Ok(map) => map,
        Err(e) => {
            eprintln!("Error parsing emojis dictionary: {e}");
            Vec::new()
        }
    }
}

pub fn load_emoji_map() -> Vec<(String, String)> {
    let config_path = match get_config_path() {
        Some(p) => p,
        None => {
            eprintln!("Error: Could not determine configuration directory.");
            return Vec::new();
        }
    };

    match std::fs::read_to_string(&config_path) {
        Ok(file) => parse_emoji_content(&file),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!(
                "Configuration file not found, creating default at: {}",
                config_path.display()
            );

            if let Some(parent) = config_path.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                eprintln!("Error creating configuration directory: {e}");
                return parse_emoji_content(DEFAULT_EMOJIS_JSON);
            }

            match std::fs::write(&config_path, DEFAULT_EMOJIS_JSON) {
                Ok(_) => {
                    eprintln!("Default emojis.json created successfully.");
                    parse_emoji_content(DEFAULT_EMOJIS_JSON)
                }
                Err(e) => {
                    eprintln!("Error writing default emojis.json: {e}");
                    parse_emoji_content(DEFAULT_EMOJIS_JSON)
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading configuration file: {e}");
            parse_emoji_content(DEFAULT_EMOJIS_JSON)
        }
    }
}
