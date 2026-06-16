use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RAMP_SIZE: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub id: String,
    pub name: String,
    pub is_primary: bool,
    pub is_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub version: u32,
    pub profiles: Vec<Profile>,
    pub selected_profile_id: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: Vec::new(),
            selected_profile_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub target_display_id: String,
    pub channel_mode: ChannelMode,
    pub linked: ChannelSettings,
    pub red: ChannelSettings,
    pub green: ChannelSettings,
    pub blue: ChannelSettings,
    pub hotkey: Option<String>,
}

impl Profile {
    pub fn with_id(mut self) -> Self {
        if self.id.trim().is_empty() {
            self.id = Uuid::new_v4().to_string();
        }
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ChannelMode {
    Linked,
    Rgb,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSettings {
    pub gamma: f64,
    pub brightness: f64,
    pub contrast: f64,
}

impl Default for ChannelSettings {
    fn default() -> Self {
        Self {
            gamma: 1.0,
            brightness: 0.0,
            contrast: 1.0,
        }
    }
}

impl ChannelSettings {
    pub fn clamped(self) -> Self {
        Self {
            gamma: self.gamma.clamp(0.5, 2.5),
            brightness: self.brightness.clamp(-0.35, 0.35),
            contrast: self.contrast.clamp(0.5, 1.75),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GammaRamp {
    pub red: Vec<u16>,
    pub green: Vec<u16>,
    pub blue: Vec<u16>,
}

impl GammaRamp {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn linear() -> Self {
        let values: Vec<u16> = (0..RAMP_SIZE)
            .map(|index| ((index as f64 / 255.0) * u16::MAX as f64).round() as u16)
            .collect();

        Self {
            red: values.clone(),
            green: values.clone(),
            blue: values,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub profile_id: Option<String>,
    pub display_id: String,
}
