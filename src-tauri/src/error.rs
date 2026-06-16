use serde::ser::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration directory is unavailable")]
    ConfigDirUnavailable,
    #[error("failed to create configuration directory: {0}")]
    ConfigDirCreate(String),
    #[error("failed to read configuration: {0}")]
    ConfigRead(String),
    #[error("failed to write configuration: {0}")]
    ConfigWrite(String),
    #[error("failed to parse configuration: {0}")]
    ConfigParse(String),
    #[error("profile name is required")]
    ProfileNameRequired,
    #[error("profile must target a display")]
    TargetDisplayRequired,
    #[error("profile was not found")]
    ProfileNotFound,
    #[error("default profile cannot be deleted")]
    ProtectedProfile,
    #[cfg_attr(not(windows), allow(dead_code))]
    #[error("display was not found: {0}")]
    DisplayNotFound(String),
    #[error("gamma control is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("gamma backend failed: {0}")]
    Backend(String),
    #[error("invalid hotkey '{binding}': {message}")]
    InvalidHotkey { binding: String, message: String },
    #[error("hotkey '{0}' is already used by another profile")]
    DuplicateHotkey(String),
    #[error("failed to register hotkey '{binding}': {message}")]
    HotkeyRegister { binding: String, message: String },
    #[error("failed to unregister hotkeys: {0}")]
    HotkeyUnregister(String),
    #[error("window operation failed: {0}")]
    Window(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
