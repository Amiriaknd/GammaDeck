use crate::{
    backend::DisplayGammaBackend,
    error::{AppError, AppResult},
    model::{DisplayInfo, GammaRamp},
};

pub struct UnsupportedBackend;

impl UnsupportedBackend {
    pub fn new() -> Self {
        Self
    }
}

impl DisplayGammaBackend for UnsupportedBackend {
    fn list_displays(&mut self) -> AppResult<Vec<DisplayInfo>> {
        Ok(vec![DisplayInfo {
            id: "unsupported-display".to_string(),
            name: format!("{} display (gamma unsupported)", std::env::consts::OS),
            is_primary: true,
            is_supported: false,
        }])
    }

    fn current_ramp(&mut self, _display_id: &str) -> AppResult<GammaRamp> {
        Err(AppError::UnsupportedPlatform)
    }

    fn set_ramp(&mut self, _display_id: &str, _ramp: &GammaRamp) -> AppResult<()> {
        Err(AppError::UnsupportedPlatform)
    }

    fn restore_startup_ramp(&mut self, _display_id: &str) -> AppResult<()> {
        Err(AppError::UnsupportedPlatform)
    }

    fn set_linear_ramp(&mut self, _display_id: &str) -> AppResult<()> {
        Err(AppError::UnsupportedPlatform)
    }
}
