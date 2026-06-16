# GammaDeck

GammaDeck is a modern Gamma Panel-inspired tool for switching display gamma profiles with global hotkeys.

It is built as a lightweight Tauri desktop app with a Rust native core and a React settings panel. The current MVP focuses on explicit per-display profiles and fast profile switching.

## Status

This repository currently contains the first scaffolded MVP:

- Tauri 2 desktop shell
- React + TypeScript UI
- Rust profile/config model
- Global hotkey registration
- Tray menu
- Gamma ramp generation
- Windows GDI gamma backend
- macOS/Linux unsupported no-op backend

Real gamma changes are only implemented for Windows. macOS and Linux can run the app shell, but gamma apply/reset actions report unsupported behavior for now.

## Features

- Manage multiple gamma profiles.
- Bind each profile to one explicit target display.
- Save linked RGB or per-channel RGB settings.
- Adjust gamma, brightness-like offset, and contrast-like slope.
- Preview the generated LUT curve.
- Apply profiles from the UI.
- Switch profiles with global hotkeys.
- Restore the startup gamma ramp captured by the app.
- Apply a Linear LUT reset separately.
- Keep the app available from the tray.

## Non-Goals

GammaDeck v1 intentionally does not include:

- Physical monitor brightness or contrast control
- DDC/CI
- HDR support
- ICC/WCS color profile management
- One-click apply-to-all-displays
- Automatic current-display detection
- Formal color calibration workflows

## Tech Stack

- Rust
- Tauri 2
- React
- TypeScript
- Vite
- Windows GDI APIs for gamma ramp control

## Development

Install dependencies:

```bash
pnpm install
```

Run static/type checks:

```bash
pnpm exec tsc --noEmit
pnpm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Run the app during local development:

```bash
pnpm tauri dev
```

Build the desktop app:

```bash
pnpm tauri build
```

## Architecture

The app is split into three main areas:

- `src/`: React UI, Tauri command calls, profile editor controls, LUT preview.
- `src-tauri/src/`: Rust app state, config storage, gamma curve generation, Tauri commands, tray, hotkey registration.
- `src-tauri/src/backend/`: Platform gamma backends.

The key Rust backend boundary is `DisplayGammaBackend`, which provides:

- `list_displays`
- `set_ramp`
- `restore_startup_ramp`
- `set_linear_ramp`

Windows uses the legacy GDI gamma ramp APIs through a platform-gated backend. Other platforms return explicit unsupported errors so the app can stay cross-platform without pretending gamma control works everywhere.

## Windows Backend Notes

The Windows backend uses legacy GDI gamma ramp APIs because they are practical for instant hotkey-driven gamma switching:

- `EnumDisplayDevicesW`
- `CreateDCW`
- `GetDeviceGammaRamp`
- `SetDeviceGammaRamp`
- `DeleteDC`

On startup/display enumeration, GammaDeck captures the current ramp for each supported display. Reset restores that captured startup ramp rather than writing a fixed hardcoded value.

The generated ramp is clamped conservatively before it is sent to Windows.

## Profile Model

Each profile contains:

- Target display id
- Gamma
- Brightness-like offset
- Contrast-like slope
- Linked or per-channel RGB settings
- Optional global hotkey

Profiles are persisted in the app config directory as JSON.
