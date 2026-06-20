# GammaDeck

GammaDeck is a lightweight Windows app for switching display gamma profiles with global hotkeys.

It is designed for people who use multiple monitors, switch between different lighting conditions, or want fast per-display gamma adjustments without opening Windows display settings.

![GammaDeck runtime screenshot](docs/assets/gammadeck-runtime.png)

## Features

- Multi-display support: choose which monitor a profile applies to.
- Multiple profiles: create different presets for work, games, night use, or specific monitors.
- Global hotkeys: switch profiles instantly from anywhere.
- Linked RGB mode: adjust gamma, brightness, and contrast together.
- Per-channel RGB mode: fine-tune red, green, and blue independently.
- LUT preview: see the generated curve before applying it.
- Reset controls: restore the gamma ramp captured when GammaDeck started, or apply a Linear LUT reset.
- Tray support: keep GammaDeck available in the background.
- Portable config: release builds keep `GammaDeck.config.json` beside `GammaDeck.exe`.

## Download And Run

GammaDeck is distributed as a portable Windows zip.

1. Download `GammaDeck-windows-x64-portable.zip` from a GitHub release.
2. Unzip it anywhere.
3. Run `GammaDeck.exe`.

There is no installer. Profiles and hotkeys are saved in the same portable folder as the app.

## WebView2 Requirement

GammaDeck uses Tauri, so it needs Microsoft Edge WebView2 Runtime on Windows. Most Windows 10/11 systems already include it.

If the app does not open, or Windows shows a WebView2-related startup error, install WebView2 Runtime from Microsoft:

<https://developer.microsoft.com/en-us/microsoft-edge/webview2/>

Use the Evergreen Bootstrapper for normal installs, or the Evergreen Standalone Installer for offline machines.

## How To Use

1. Select or create a profile from the left sidebar.
2. Choose the target display.
3. Set a global hotkey, such as `control+alt+Numpad0`.
4. Adjust gamma, brightness, and contrast.
5. Use `Linked` mode for simple tuning, or `RGB` mode for per-channel correction.
6. GammaDeck applies the profile when you select it or press its hotkey.

## Current Limitations

- Real gamma changes are currently implemented only on Windows.
- macOS and Linux can run the app shell, but gamma apply/reset actions are unsupported.
- GammaDeck adjusts the GPU gamma ramp only. It does not change physical monitor brightness, DDC/CI settings, HDR behavior, or ICC/WCS color profiles.
- Profiles target one display at a time.

## Development

Install dependencies:

```bash
pnpm install
```

Run the app locally:

```bash
pnpm tauri dev
```

Run checks:

```bash
pnpm exec tsc --noEmit
pnpm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Build a portable Windows executable:

```bash
pnpm tauri build --no-bundle
```

The executable is written to `src-tauri/target/release/gammadeck.exe` on Windows.
