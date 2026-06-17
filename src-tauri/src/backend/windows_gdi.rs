use std::{collections::HashMap, ffi::c_void, iter};

use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::BOOL,
        Graphics::Gdi::{
            CreateDCW, DeleteDC, EnumDisplayDevicesW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ACTIVE,
            DISPLAY_DEVICE_PRIMARY_DEVICE, HDC,
        },
        UI::ColorSystem::{GetDeviceGammaRamp, SetDeviceGammaRamp},
    },
};

use crate::{
    backend::DisplayGammaBackend,
    error::{AppError, AppResult},
    model::{DisplayInfo, GammaRamp, RAMP_SIZE},
};

pub struct WindowsGdiBackend {
    startup_ramps: HashMap<String, GammaRamp>,
}

impl WindowsGdiBackend {
    pub fn new() -> Self {
        Self {
            startup_ramps: HashMap::new(),
        }
    }

    fn capture_startup_ramp(&mut self, display_id: &str) -> AppResult<()> {
        if self.startup_ramps.contains_key(display_id) {
            return Ok(());
        }

        let ramp = with_display_dc(display_id, |dc| read_ramp(dc))?;
        self.startup_ramps.insert(display_id.to_string(), ramp);
        Ok(())
    }
}

impl DisplayGammaBackend for WindowsGdiBackend {
    fn list_displays(&mut self) -> AppResult<Vec<DisplayInfo>> {
        let displays = enumerate_displays()?;
        for display in displays.iter().filter(|display| display.is_supported) {
            let _ = self.capture_startup_ramp(&display.id);
        }
        Ok(displays)
    }

    fn set_ramp(&mut self, display_id: &str, ramp: &GammaRamp) -> AppResult<()> {
        self.capture_startup_ramp(display_id)?;
        with_display_dc(display_id, |dc| {
            write_ramp(dc, ramp)?;
            let read_back = read_ramp(dc)?;
            if !ramps_are_close(ramp, &read_back) {
                return Err(AppError::Backend(
                    "gamma ramp verification did not match the requested ramp".to_string(),
                ));
            }
            Ok(())
        })
    }

    fn restore_startup_ramp(&mut self, display_id: &str) -> AppResult<()> {
        let ramp = self
            .startup_ramps
            .get(display_id)
            .cloned()
            .ok_or_else(|| AppError::DisplayNotFound(display_id.to_string()))?;
        with_display_dc(display_id, |dc| write_ramp(dc, &ramp))
    }

    fn set_linear_ramp(&mut self, display_id: &str) -> AppResult<()> {
        self.capture_startup_ramp(display_id)?;
        with_display_dc(display_id, |dc| write_ramp(dc, &GammaRamp::linear()))
    }
}

fn enumerate_displays() -> AppResult<Vec<DisplayInfo>> {
    let mut displays = Vec::new();
    let mut index = 0;

    loop {
        let mut device = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };

        let ok = unsafe { EnumDisplayDevicesW(PCWSTR::null(), index, &mut device, 0) };
        if !ok.as_bool() {
            break;
        }

        if device.StateFlags & DISPLAY_DEVICE_ACTIVE != 0 {
            let id = wide_array_to_string(&device.DeviceName);
            let name = wide_array_to_string(&device.DeviceString);
            displays.push(DisplayInfo {
                id: id.clone(),
                name: if name.is_empty() { id } else { name },
                is_primary: device.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0,
                is_supported: true,
            });
        }

        index += 1;
    }

    if displays.is_empty() {
        return Err(AppError::Backend(
            "no active displays were reported by Windows".to_string(),
        ));
    }

    Ok(displays)
}

fn with_display_dc<T>(
    display_id: &str,
    operation: impl FnOnce(HDC) -> AppResult<T>,
) -> AppResult<T> {
    let display_name = to_wide_null(display_id);
    let dc = unsafe {
        CreateDCW(
            PCWSTR::null(),
            PCWSTR(display_name.as_ptr()),
            PCWSTR::null(),
            None,
        )
    };

    if dc.0.is_null() {
        return Err(AppError::DisplayNotFound(display_id.to_string()));
    }

    let result = operation(dc);
    let deleted = unsafe { DeleteDC(dc) };
    if !deleted.as_bool() {
        return Err(AppError::Backend(
            "failed to release display device context".to_string(),
        ));
    }

    result
}

fn read_ramp(dc: HDC) -> AppResult<GammaRamp> {
    let mut raw = [[0u16; RAMP_SIZE]; 3];
    let ok = unsafe { GetDeviceGammaRamp(dc, raw.as_mut_ptr().cast::<c_void>()) };
    bool_result(ok, "GetDeviceGammaRamp failed")?;

    Ok(GammaRamp {
        red: raw[0].to_vec(),
        green: raw[1].to_vec(),
        blue: raw[2].to_vec(),
    })
}

fn write_ramp(dc: HDC, ramp: &GammaRamp) -> AppResult<()> {
    let raw = ramp_to_raw(ramp)?;
    let ok = unsafe { SetDeviceGammaRamp(dc, raw.as_ptr().cast::<c_void>()) };
    bool_result(ok, "SetDeviceGammaRamp failed")
}

fn ramp_to_raw(ramp: &GammaRamp) -> AppResult<[[u16; RAMP_SIZE]; 3]> {
    if ramp.red.len() != RAMP_SIZE || ramp.green.len() != RAMP_SIZE || ramp.blue.len() != RAMP_SIZE
    {
        return Err(AppError::Backend(
            "gamma ramp must contain 256 values per channel".to_string(),
        ));
    }

    let mut raw = [[0u16; RAMP_SIZE]; 3];
    raw[0].copy_from_slice(&ramp.red);
    raw[1].copy_from_slice(&ramp.green);
    raw[2].copy_from_slice(&ramp.blue);
    Ok(raw)
}

fn ramps_are_close(expected: &GammaRamp, actual: &GammaRamp) -> bool {
    expected
        .red
        .iter()
        .chain(expected.green.iter())
        .chain(expected.blue.iter())
        .zip(
            actual
                .red
                .iter()
                .chain(actual.green.iter())
                .chain(actual.blue.iter()),
        )
        .all(|(left, right)| left.abs_diff(*right) <= 512)
}

fn bool_result(ok: BOOL, message: &str) -> AppResult<()> {
    if ok.as_bool() {
        Ok(())
    } else {
        Err(AppError::Backend(message.to_string()))
    }
}

fn wide_array_to_string(raw: &[u16]) -> String {
    let end = raw
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end])
}

fn to_wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(iter::once(0)).collect()
}
