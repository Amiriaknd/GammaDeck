use std::{fs, path::PathBuf};

use crate::{
    error::{AppError, AppResult},
    model::AppConfig,
};

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            path: config_dir.join("config.json"),
        }
    }

    pub fn load(&self) -> AppResult<AppConfig> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }

        let raw = fs::read_to_string(&self.path)
            .map_err(|error| AppError::ConfigRead(error.to_string()))?;
        serde_json::from_str(&raw).map_err(|error| AppError::ConfigParse(error.to_string()))
    }

    pub fn save(&self, config: &AppConfig) -> AppResult<()> {
        let parent = self.path.parent().ok_or(AppError::ConfigDirUnavailable)?;
        fs::create_dir_all(parent).map_err(|error| AppError::ConfigDirCreate(error.to_string()))?;
        let raw = serde_json::to_string_pretty(config)
            .map_err(|error| AppError::ConfigWrite(error.to_string()))?;
        fs::write(&self.path, raw).map_err(|error| AppError::ConfigWrite(error.to_string()))
    }
}
