use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::server::auth;

#[derive(Deserialize, Serialize, Clone)]
pub struct PreviousPage {
    pub path: String,
}

impl PreviousPage {
    pub async fn set(username: &str, path: &str) -> Result<()> {
        let dir = auth::user_data_dir(username);
        fs::create_dir_all(&dir).await?;
        let previous_file = dir.join("previous.json");
        let previous = Self {
            path: path.to_string(),
        };
        let json = serde_json::to_string_pretty(&previous)?;
        fs::write(previous_file, json).await?;
        Ok(())
    }

    pub async fn get(username: &str) -> Option<String> {
        let data = auth::user_data_dir(username).join("previous.json");
        if let Ok(prev_json) = fs::read_to_string(data).await
            && let Ok(previous) = serde_json::from_str::<Self>(&prev_json)
        {
            return Some(previous.path);
        }
        None
    }
}
