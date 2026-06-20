use std::{collections::HashMap, ffi::c_void, iter};

use windows::{
    core::PCWSTR,
    Win32::{
        Devices::Display::{
            DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
            DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME,
            QDC_ONLY_ACTIVE_PATHS,
        },
        Foundation::{BOOL, ERROR_SUCCESS},
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
    let monitor_names = display_config_monitor_names();
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
            let adapter_name = wide_array_to_string(&device.DeviceString);
            let monitor_name = monitor_names
                .get(&id)
                .cloned()
                .or_else(|| enum_display_monitor_name(&id));
            displays.push(DisplayInfo {
                id: id.clone(),
                name: display_name(&id, monitor_name.as_deref(), &adapter_name),
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

fn display_config_monitor_names() -> HashMap<String, String> {
    let mut path_count = 0;
    let mut mode_count = 0;
    let sizes = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    if sizes != ERROR_SUCCESS || path_count == 0 {
        return HashMap::new();
    }

    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    let queried = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if queried != ERROR_SUCCESS {
        return HashMap::new();
    }

    paths.truncate(path_count as usize);
    let mut names = HashMap::new();
    for path in paths {
        let Some(source_name) =
            display_config_source_name(path.sourceInfo.adapterId, path.sourceInfo.id)
        else {
            continue;
        };
        let Some(target_name) =
            display_config_target_name(path.targetInfo.adapterId, path.targetInfo.id)
        else {
            continue;
        };
        names.insert(source_name, target_name);
    }

    names
}

fn display_config_source_name(
    adapter_id: windows::Win32::Foundation::LUID,
    source_id: u32,
) -> Option<String> {
    let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: adapter_id,
            id: source_id,
        },
        ..Default::default()
    };

    let result = unsafe { DisplayConfigGetDeviceInfo(&mut source.header) };
    if result != ERROR_SUCCESS.0 as i32 {
        return None;
    }

    non_empty_wide_array_to_string(&source.viewGdiDeviceName)
}

fn display_config_target_name(
    adapter_id: windows::Win32::Foundation::LUID,
    target_id: u32,
) -> Option<String> {
    let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: adapter_id,
            id: target_id,
        },
        ..Default::default()
    };

    let result = unsafe { DisplayConfigGetDeviceInfo(&mut target.header) };
    if result != ERROR_SUCCESS.0 as i32 {
        return None;
    }

    non_empty_wide_array_to_string(&target.monitorFriendlyDeviceName)
}

fn enum_display_monitor_name(display_id: &str) -> Option<String> {
    let display_name = to_wide_null(display_id);
    let mut monitor_index = 0;

    loop {
        let mut monitor = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };

        let ok = unsafe {
            EnumDisplayDevicesW(
                PCWSTR(display_name.as_ptr()),
                monitor_index,
                &mut monitor,
                0,
            )
        };
        if !ok.as_bool() {
            return None;
        }

        if monitor.StateFlags & DISPLAY_DEVICE_ACTIVE != 0 {
            if let Some(name) = non_empty_wide_array_to_string(&monitor.DeviceString) {
                return Some(name);
            }
        }

        monitor_index += 1;
    }
}

fn display_name(id: &str, monitor_name: Option<&str>, adapter_name: &str) -> String {
    monitor_name
        .filter(|name| !name.eq_ignore_ascii_case(adapter_name))
        .or_else(|| (!adapter_name.is_empty()).then_some(adapter_name))
        .unwrap_or(id)
        .to_string()
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

fn non_empty_wide_array_to_string(raw: &[u16]) -> Option<String> {
    let value = wide_array_to_string(raw);
    (!value.trim().is_empty()).then_some(value)
}

fn to_wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(iter::once(0)).collect()
}
