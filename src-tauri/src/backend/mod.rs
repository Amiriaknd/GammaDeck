use crate::{
    error::AppResult,
    model::{DisplayInfo, GammaRamp},
};

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod windows_gdi;

pub trait DisplayGammaBackend: Send {
    fn list_displays(&mut self) -> AppResult<Vec<DisplayInfo>>;
    fn current_ramp(&mut self, display_id: &str) -> AppResult<GammaRamp>;
    fn set_ramp(&mut self, display_id: &str, ramp: &GammaRamp) -> AppResult<()>;
    fn restore_startup_ramp(&mut self, display_id: &str) -> AppResult<()>;
    fn set_linear_ramp(&mut self, display_id: &str) -> AppResult<()>;
}

pub fn create_backend() -> Box<dyn DisplayGammaBackend> {
    #[cfg(windows)]
    {
        Box::new(windows_gdi::WindowsGdiBackend::new())
    }

    #[cfg(not(windows))]
    {
        Box::new(unsupported::UnsupportedBackend::new())
    }
}
