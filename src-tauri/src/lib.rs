mod backend;
mod config_store;
mod error;
mod gamma_curve;
mod model;

use std::{
    collections::HashSet,
    str::FromStr,
    sync::{Mutex, MutexGuard},
};

use backend::{create_backend, DisplayGammaBackend};
use config_store::ConfigStore;
use error::{AppError, AppResult};
use gamma_curve::generate_ramp;
use model::{AppConfig, ApplyResult, DisplayInfo, Profile};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, State,
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
    let mut gamma = lock(&state.gamma, "gamma backend")?;
    gamma.list_displays()
}

#[tauri::command]
fn load_config(state: State<'_, AppState>) -> AppResult<AppConfig> {
    let config = lock(&state.config, "configuration")?;
    Ok(config.clone())
}

#[tauri::command]
fn save_profile(app: AppHandle, state: State<'_, AppState>, profile: Profile) -> AppResult<AppConfig> {
    let normalized = normalize_profile(profile)?;
    let mut config = lock(&state.config, "configuration")?;
    validate_profile(&normalized, &config)?;

    if let Some(index) = config.profiles.iter().position(|item| item.id == normalized.id) {
        config.profiles[index] = normalized.clone();
    } else {
        config.profiles.push(normalized.clone());
    }

    config.selected_profile_id = Some(normalized.id.clone());
    state.config_store.save(&config)?;
    register_hotkeys(&app, &state, &config)?;
    Ok(config.clone())
}

#[tauri::command]
fn delete_profile(app: AppHandle, state: State<'_, AppState>, profile_id: String) -> AppResult<AppConfig> {
    let mut config = lock(&state.config, "configuration")?;
    let original_len = config.profiles.len();
    config.profiles.retain(|profile| profile.id != profile_id);

    if original_len == config.profiles.len() {
        return Err(AppError::ProfileNotFound);
    }

    if config.selected_profile_id.as_deref() == Some(profile_id.as_str()) {
        config.selected_profile_id = config.profiles.first().map(|profile| profile.id.clone());
    }

    state.config_store.save(&config)?;
    register_hotkeys(&app, &state, &config)?;
    Ok(config.clone())
}

#[tauri::command]
fn apply_profile(state: State<'_, AppState>, profile_id: String) -> AppResult<ApplyResult> {
    apply_profile_by_id(&state, &profile_id)
}

#[tauri::command]
fn apply_draft_profile(state: State<'_, AppState>, profile: Profile) -> AppResult<ApplyResult> {
    let normalized = normalize_profile(profile)?;
    apply_profile_value(&state, &normalized, None)
}

#[tauri::command]
fn reset_display(state: State<'_, AppState>, display_id: String, linear: bool) -> AppResult<ApplyResult> {
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
    let profile = {
        let config = lock(&state.config, "configuration")?;
        config
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .cloned()
            .ok_or(AppError::ProfileNotFound)?
    };

    apply_profile_value(state, &profile, Some(profile_id.to_string()))
}

fn apply_profile_value(
    state: &AppState,
    profile: &Profile,
    profile_id: Option<String>,
) -> AppResult<ApplyResult> {
    if profile.target_display_id.trim().is_empty() {
        return Err(AppError::TargetDisplayRequired);
    }

    let ramp = generate_ramp(profile);
    let mut gamma = lock(&state.gamma, "gamma backend")?;
    gamma.set_ramp(&profile.target_display_id, &ramp)?;

    let mut active_display_id = lock(&state.active_display_id, "active display")?;
    *active_display_id = Some(profile.target_display_id.clone());

    Ok(ApplyResult {
        profile_id,
        display_id: profile.target_display_id.clone(),
    })
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

                let state = app.state::<AppState>();
                match apply_profile_by_id(&state, &profile_id) {
                    Ok(result) => {
                        let _ = app.emit("profile-applied", result);
                    }
                    Err(error) => {
                        let _ = app.emit("profile-apply-error", error.to_string());
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

fn lock<'a, T>(mutex: &'a Mutex<T>, name: &str) -> AppResult<MutexGuard<'a, T>> {
    mutex
        .lock()
        .map_err(|_| AppError::Backend(format!("{name} lock is poisoned")))
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "Reset Selected Display", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &reset, &quit])?;

    TrayIconBuilder::new()
        .tooltip("GammaDeck")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "reset" => {
                let state = app.state::<AppState>();
                let display_id = lock(&state.active_display_id, "active display")
                    .ok()
                    .and_then(|guard| guard.clone());

                if let Some(display_id) = display_id {
                    let result = {
                        let mut gamma = match lock(&state.gamma, "gamma backend") {
                            Ok(gamma) => gamma,
                            Err(error) => {
                                let _ = app.emit("profile-apply-error", error.to_string());
                                return;
                            }
                        };
                        gamma.restore_startup_ramp(&display_id)
                    };

                    if let Err(error) = result {
                        let _ = app.emit("profile-apply-error", error.to_string());
                    }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
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
            let config = lock(&state.config, "configuration")?.clone();
            register_hotkeys(&handle, &state, &config)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_displays,
            load_config,
            save_profile,
            delete_profile,
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
