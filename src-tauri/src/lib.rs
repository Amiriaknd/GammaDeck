mod backend;
mod config_store;
mod error;
mod gamma_curve;
mod model;

const DEFAULT_PROFILE_ID: &str = "default";

use std::{
    collections::HashSet,
    path::PathBuf,
    str::FromStr,
    sync::{Mutex, MutexGuard},
};

use backend::{create_backend, DisplayGammaBackend};
use config_store::ConfigStore;
use error::{AppError, AppResult};
use gamma_curve::generate_ramp;
use model::{AppConfig, ApplyResult, DisplayBaseline, DisplayInfo, GammaRamp, Profile};
use serde::Serialize;
use tauri::{
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub struct AppState {
    config: Mutex<AppConfig>,
    config_store: ConfigStore,
    gamma: Mutex<Box<dyn DisplayGammaBackend>>,
    active_display_id: Mutex<Option<String>>,
}

#[tauri::command]
fn list_displays(state: State<'_, AppState>) -> AppResult<Vec<DisplayInfo>> {
    let displays = {
        let mut gamma = lock(&state.gamma, "gamma backend")?;
        gamma.list_displays()?
    };
    ensure_display_baselines(&state, &displays)?;
    Ok(displays)
}

#[tauri::command]
fn load_config(state: State<'_, AppState>) -> AppResult<AppConfig> {
    let config = lock(&state.config, "configuration")?;
    Ok(config.clone())
}

#[tauri::command]
fn save_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: Profile,
) -> AppResult<AppConfig> {
    let normalized = normalize_profile(profile)?;
    let mut config = lock(&state.config, "configuration")?;
    validate_profile(&normalized, &config)?;

    let existing_index = config
        .profiles
        .iter()
        .position(|item| item.id == normalized.id);
    let hotkeys_changed = existing_index
        .map(|index| config.profiles[index].hotkey != normalized.hotkey)
        .unwrap_or_else(|| normalized.hotkey.is_some());

    if let Some(index) = existing_index {
        config.profiles[index] = normalized.clone();
    } else {
        config.profiles.push(normalized.clone());
    }

    config.selected_profile_id = Some(normalized.id.clone());
    state.config_store.save(&config)?;
    if hotkeys_changed {
        register_hotkeys(&app, &state, &config)?;
    }
    Ok(config.clone())
}

#[tauri::command]
fn delete_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> AppResult<AppConfig> {
    if profile_id == DEFAULT_PROFILE_ID {
        return Err(AppError::ProtectedProfile);
    }

    let mut config = lock(&state.config, "configuration")?;
    let index = config
        .profiles
        .iter()
        .position(|profile| profile.id == profile_id)
        .ok_or(AppError::ProfileNotFound)?;
    let removed = config.profiles.remove(index);
    let hotkeys_changed = removed.hotkey.is_some();

    if config.selected_profile_id.as_deref() == Some(profile_id.as_str()) {
        config.selected_profile_id = config.profiles.first().map(|profile| profile.id.clone());
    }

    state.config_store.save(&config)?;
    if hotkeys_changed {
        register_hotkeys(&app, &state, &config)?;
    }
    Ok(config.clone())
}

#[tauri::command]
fn update_display_baseline(state: State<'_, AppState>, display_id: String) -> AppResult<AppConfig> {
    let display_id = display_id.trim().to_string();
    if display_id.is_empty() {
        return Err(AppError::TargetDisplayRequired);
    }

    let ramp = {
        let mut gamma = lock(&state.gamma, "gamma backend")?;
        gamma.current_ramp(&display_id)?
    };

    let mut config = lock(&state.config, "configuration")?;
    upsert_display_baseline(&mut config, display_id, ramp);
    config.version = config.version.max(2);
    state.config_store.save(&config)?;
    Ok(config.clone())
}

#[tauri::command]
fn reset_display_baseline(
    state: State<'_, AppState>,
    display_id: String,
    target: String,
) -> AppResult<AppConfig> {
    let display_id = display_id.trim().to_string();
    if display_id.is_empty() {
        return Err(AppError::TargetDisplayRequired);
    }

    let ramp = match target.as_str() {
        "initial" => {
            let config = lock(&state.config, "configuration")?;
            initial_baseline_for_display(&config, &display_id).ok_or_else(|| {
                AppError::Backend("first-run baseline is unavailable for this display".to_string())
            })?
        }
        "neutral" => GammaRamp::linear(),
        _ => {
            return Err(AppError::Backend(format!(
                "unknown baseline reset target: {target}"
            )))
        }
    };

    let mut config = lock(&state.config, "configuration")?;
    upsert_display_baseline(&mut config, display_id, ramp);
    config.version = config.version.max(2);
    state.config_store.save(&config)?;
    Ok(config.clone())
}

#[tauri::command]
fn apply_profile(state: State<'_, AppState>, profile_id: String) -> AppResult<ApplyResult> {
    apply_profile_by_id(&state, &profile_id)
}

#[tauri::command]
fn apply_draft_profile(state: State<'_, AppState>, profile: Profile) -> AppResult<ApplyResult> {
    let normalized = normalize_profile(profile)?;
    let baseline = {
        let config = lock(&state.config, "configuration")?;
        baseline_for_display(&config, &normalized.target_display_id)
    };
    apply_profile_value(&state, &normalized, baseline.as_ref(), None)
}

#[tauri::command]
fn reset_display(
    state: State<'_, AppState>,
    display_id: String,
    linear: bool,
) -> AppResult<ApplyResult> {
    if display_id.trim().is_empty() {
        return Err(AppError::TargetDisplayRequired);
    }

    let mut gamma = lock(&state.gamma, "gamma backend")?;
    if linear {
        gamma.set_linear_ramp(&display_id)?;
    } else {
        gamma.restore_startup_ramp(&display_id)?;
    }

    let mut active_display_id = lock(&state.active_display_id, "active display")?;
    *active_display_id = Some(display_id.clone());

    Ok(ApplyResult {
        profile_id: None,
        display_id,
    })
}

#[tauri::command]
fn refresh_hotkeys(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppConfig> {
    let config = lock(&state.config, "configuration")?;
    register_hotkeys(&app, &state, &config)?;
    Ok(config.clone())
}

#[tauri::command]
fn hide_window(app: AppHandle) -> AppResult<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::Window("main window was not found".to_string()))?;
    window
        .hide()
        .map_err(|error| AppError::Window(error.to_string()))
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

fn apply_profile_by_id(state: &AppState, profile_id: &str) -> AppResult<ApplyResult> {
    let (profile, baseline) = {
        let config = lock(&state.config, "configuration")?;
        let profile = config
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .ok_or(AppError::ProfileNotFound)?;
        let baseline = baseline_for_display(&config, &profile.target_display_id);
        (profile, baseline)
    };

    let result = apply_profile_value(
        state,
        &profile,
        baseline.as_ref(),
        Some(profile_id.to_string()),
    )?;
    remember_selected_profile(state, profile_id)?;
    Ok(result)
}

fn apply_profile_value(
    state: &AppState,
    profile: &Profile,
    baseline: Option<&GammaRamp>,
    profile_id: Option<String>,
) -> AppResult<ApplyResult> {
    if profile.target_display_id.trim().is_empty() {
        return Err(AppError::TargetDisplayRequired);
    }

    let fallback = GammaRamp::linear();
    let ramp = generate_ramp(profile, baseline.unwrap_or(&fallback));
    let mut gamma = lock(&state.gamma, "gamma backend")?;
    gamma.set_ramp(&profile.target_display_id, &ramp)?;

    let mut active_display_id = lock(&state.active_display_id, "active display")?;
    *active_display_id = Some(profile.target_display_id.clone());

    Ok(ApplyResult {
        profile_id,
        display_id: profile.target_display_id.clone(),
    })
}

fn remember_selected_profile(state: &AppState, profile_id: &str) -> AppResult<()> {
    let mut config = lock(&state.config, "configuration")?;
    if config.selected_profile_id.as_deref() == Some(profile_id) {
        return Ok(());
    }

    config.selected_profile_id = Some(profile_id.to_string());
    state.config_store.save(&config)
}

fn ensure_display_baselines(state: &AppState, displays: &[DisplayInfo]) -> AppResult<()> {
    let baseline_jobs = {
        let config = lock(&state.config, "configuration")?;
        displays
            .iter()
            .filter(|display| display.is_supported)
            .filter_map(|display| {
                let initial = initial_baseline_for_display(&config, &display.id);
                let current = baseline_for_display(&config, &display.id);
                (initial.is_none() || current.is_none()).then_some((
                    display.id.clone(),
                    initial,
                    current,
                ))
            })
            .collect::<Vec<_>>()
    };

    if baseline_jobs.is_empty() {
        return Ok(());
    }

    let mut initial_baselines = Vec::new();
    let mut current_baselines = Vec::new();
    let mut displays_to_read = Vec::new();
    for (display_id, initial, current) in baseline_jobs {
        if initial.is_none() {
            if let Some(ramp) = current.clone() {
                initial_baselines.push(DisplayBaseline {
                    display_id: display_id.clone(),
                    ramp,
                });
            } else {
                displays_to_read.push(display_id.clone());
            }
        }

        if current.is_none() && !displays_to_read.iter().any(|id| id == &display_id) {
            displays_to_read.push(display_id);
        }
    }

    {
        let mut gamma = lock(&state.gamma, "gamma backend")?;
        for display_id in displays_to_read {
            match gamma.current_ramp(&display_id) {
                Ok(ramp) => {
                    initial_baselines.push(DisplayBaseline {
                        display_id: display_id.clone(),
                        ramp: ramp.clone(),
                    });
                    current_baselines.push(DisplayBaseline { display_id, ramp });
                }
                Err(error) => dev_log(&format!("failed to capture baseline ramp: {error}")),
            }
        }
    }

    if initial_baselines.is_empty() && current_baselines.is_empty() {
        return Ok(());
    }

    let mut config = lock(&state.config, "configuration")?;
    for baseline in initial_baselines {
        upsert_initial_display_baseline(&mut config, baseline.display_id, baseline.ramp);
    }
    for baseline in current_baselines {
        upsert_display_baseline(&mut config, baseline.display_id, baseline.ramp);
    }
    config.version = config.version.max(2);
    state.config_store.save(&config)
}

fn baseline_for_display(config: &AppConfig, display_id: &str) -> Option<GammaRamp> {
    baseline_for_display_in(&config.display_baselines, display_id)
}

fn initial_baseline_for_display(config: &AppConfig, display_id: &str) -> Option<GammaRamp> {
    baseline_for_display_in(&config.initial_display_baselines, display_id)
}

fn baseline_for_display_in(baselines: &[DisplayBaseline], display_id: &str) -> Option<GammaRamp> {
    baselines
        .iter()
        .find(|baseline| baseline.display_id == display_id && baseline.ramp.is_valid())
        .map(|baseline| baseline.ramp.clone())
}

fn upsert_display_baseline(config: &mut AppConfig, display_id: String, ramp: GammaRamp) {
    upsert_baseline(&mut config.display_baselines, display_id, ramp);
}

fn upsert_initial_display_baseline(config: &mut AppConfig, display_id: String, ramp: GammaRamp) {
    upsert_baseline(&mut config.initial_display_baselines, display_id, ramp);
}

fn upsert_baseline(baselines: &mut Vec<DisplayBaseline>, display_id: String, ramp: GammaRamp) {
    if let Some(existing) = baselines
        .iter_mut()
        .find(|baseline| baseline.display_id == display_id)
    {
        existing.ramp = ramp;
        return;
    }

    baselines.push(DisplayBaseline { display_id, ramp });
}

fn normalize_profile(profile: Profile) -> AppResult<Profile> {
    let mut profile = profile.with_id();
    profile.name = profile.name.trim().to_string();
    profile.target_display_id = profile.target_display_id.trim().to_string();
    profile.hotkey = profile
        .hotkey
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(canonical_hotkey)
        .transpose()?;

    Ok(profile)
}

fn validate_profile(profile: &Profile, config: &AppConfig) -> AppResult<()> {
    if profile.name.is_empty() {
        return Err(AppError::ProfileNameRequired);
    }

    if profile.target_display_id.is_empty() {
        return Err(AppError::TargetDisplayRequired);
    }

    if let Some(hotkey) = &profile.hotkey {
        if config
            .profiles
            .iter()
            .any(|item| item.id != profile.id && item.hotkey.as_deref() == Some(hotkey.as_str()))
        {
            return Err(AppError::DuplicateHotkey(hotkey.clone()));
        }
    }

    Ok(())
}

fn canonical_hotkey(binding: &str) -> AppResult<String> {
    let shortcut = Shortcut::from_str(binding).map_err(|error| AppError::InvalidHotkey {
        binding: binding.to_string(),
        message: error.to_string(),
    })?;
    Ok(shortcut.to_string())
}

fn register_hotkeys(app: &AppHandle, state: &AppState, config: &AppConfig) -> AppResult<()> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|error| AppError::HotkeyUnregister(error.to_string()))?;

    let mut seen = HashSet::new();
    for profile in &config.profiles {
        let Some(binding) = profile.hotkey.as_deref() else {
            continue;
        };

        let shortcut = Shortcut::from_str(binding).map_err(|error| AppError::InvalidHotkey {
            binding: binding.to_string(),
            message: error.to_string(),
        })?;

        if !seen.insert(shortcut) {
            return Err(AppError::DuplicateHotkey(binding.to_string()));
        }

        let profile_id = profile.id.clone();
        app.global_shortcut()
            .on_shortcut(shortcut, move |app, _shortcut, event| {
                if event.state() != ShortcutState::Pressed {
                    return;
                }

                dev_log(&format!("hotkey pressed for profile {}", profile_id));
                dispatch_ui_event(app, "gammadeck-profile-hotkey", profile_id.clone());
                let state = app.state::<AppState>();
                match apply_profile_by_id(&state, &profile_id) {
                    Ok(result) => {
                        dev_log(&format!(
                            "profile applied from hotkey: profile={:?}, display={}",
                            result.profile_id, result.display_id
                        ));
                        dispatch_ui_event(app, "gammadeck-profile-applied", result);
                    }
                    Err(error) => {
                        let message = error.to_string();
                        dev_log(&format!("profile apply failed from hotkey: {message}"));
                        dispatch_ui_event(app, "gammadeck-profile-apply-error", message);
                    }
                }
            })
            .map_err(|error| AppError::HotkeyRegister {
                binding: binding.to_string(),
                message: error.to_string(),
            })?;
    }

    let _ = state;
    Ok(())
}

fn dispatch_ui_event<T>(app: &AppHandle, event: &str, payload: T)
where
    T: Serialize,
{
    let event_json = match serde_json::to_string(event) {
        Ok(value) => value,
        Err(error) => {
            dev_log(&format!(
                "failed to serialize ui event name {event}: {error}"
            ));
            return;
        }
    };
    let payload_json = match serde_json::to_string(&payload) {
        Ok(value) => value,
        Err(error) => {
            dev_log(&format!("failed to serialize payload for {event}: {error}"));
            return;
        }
    };
    let script = format!(
        "window.dispatchEvent(new CustomEvent({event_json}, {{ detail: {payload_json} }}));"
    );

    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.eval(script) {
            dev_log(&format!(
                "failed to dispatch {event} to main webview: {error}"
            ));
        } else {
            dev_log(&format!("dispatched {event} to main webview"));
        }
    } else {
        dev_log(&format!("main window was not found for {event}"));
    }
}

#[cfg(debug_assertions)]
fn dev_log(message: &str) {
    eprintln!("[GammaDeck] {message}");
}

#[cfg(not(debug_assertions))]
fn dev_log(_message: &str) {}

fn lock<'a, T>(mutex: &'a Mutex<T>, name: &str) -> AppResult<MutexGuard<'a, T>> {
    mutex
        .lock()
        .map_err(|_| AppError::Backend(format!("{name} lock is poisoned")))
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or("default window icon is unavailable")?;
    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("GammaDeck")
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if matches!(event, WindowEvent::Resized(_)) && window.is_minimized().unwrap_or(false) {
                let _ = window.hide();
            }
        })
        .setup(|app| {
            setup_main_window(app)?;

            let config_dir = config_dir(app)?;
            let config_store = ConfigStore::new(config_dir);
            let config = config_store.load()?;
            let state = AppState {
                config: Mutex::new(config),
                config_store,
                gamma: Mutex::new(create_backend()),
                active_display_id: Mutex::new(None),
            };
            app.manage(state);
            setup_tray(app)?;

            let handle = app.handle().clone();
            let state = handle.state::<AppState>();
            if let Ok(displays) = {
                let mut gamma = lock(&state.gamma, "gamma backend")?;
                gamma.list_displays()
            } {
                ensure_display_baselines(&state, &displays)?;
            }
            let config = lock(&state.config, "configuration")?.clone();
            register_hotkeys(&handle, &state, &config)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_displays,
            load_config,
            save_profile,
            delete_profile,
            update_display_baseline,
            reset_display_baseline,
            apply_profile,
            apply_draft_profile,
            reset_display,
            refresh_hotkeys,
            hide_window,
            exit_app
        ])
        .run(tauri::generate_context!())
        .expect("failed to run GammaDeck");
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn setup_main_window(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let window_config = app
        .config()
        .app
        .windows
        .first()
        .ok_or("main window configuration is unavailable")?;
    #[cfg(not(debug_assertions))]
    let builder = WebviewWindowBuilder::from_config(app, window_config)?
        .data_directory(executable_dir()?.join("GammaDeck.data"));

    #[cfg(debug_assertions)]
    let builder = WebviewWindowBuilder::from_config(app, window_config)?;

    builder.build()?;
    Ok(())
}

fn config_dir(_app: &tauri::App) -> Result<PathBuf, Box<dyn std::error::Error>> {
    #[cfg(debug_assertions)]
    {
        Ok(_app.path().app_config_dir()?)
    }

    #[cfg(not(debug_assertions))]
    {
        executable_dir()
    }
}

#[cfg(not(debug_assertions))]
fn executable_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    exe_path
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| "executable directory is unavailable".into())
}
