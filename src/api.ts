import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, ApplyResult, DisplayInfo, Profile } from "./types";

export const api = {
  listDisplays: () => invoke<DisplayInfo[]>("list_displays"),
  loadConfig: () => invoke<AppConfig>("load_config"),
  saveProfile: (profile: Profile) => invoke<AppConfig>("save_profile", { profile }),
  deleteProfile: (profileId: string) => invoke<AppConfig>("delete_profile", { profileId }),
  updateDisplayBaseline: (displayId: string) =>
    invoke<AppConfig>("update_display_baseline", { displayId }),
  resetDisplayBaseline: (displayId: string, target: "initial" | "neutral") =>
    invoke<AppConfig>("reset_display_baseline", { displayId, target }),
  applyProfile: (profileId: string) => invoke<ApplyResult>("apply_profile", { profileId }),
  applyDraftProfile: (profile: Profile) =>
    invoke<ApplyResult>("apply_draft_profile", { profile }),
  resetDisplay: (displayId: string, linear: boolean) =>
    invoke<ApplyResult>("reset_display", { displayId, linear }),
  refreshHotkeys: () => invoke<AppConfig>("refresh_hotkeys"),
  hideWindow: () => invoke<void>("hide_window"),
  exitApp: () => invoke<void>("exit_app"),
};
