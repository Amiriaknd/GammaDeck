export type ChannelMode = "linked" | "rgb";

export interface DisplayInfo {
  id: string;
  name: string;
  isPrimary: boolean;
  isSupported: boolean;
}

export interface ChannelSettings {
  gamma: number;
  brightness: number;
  contrast: number;
}

export interface Profile {
  id: string;
  name: string;
  targetDisplayId: string;
  channelMode: ChannelMode;
  linked: ChannelSettings;
  red: ChannelSettings;
  green: ChannelSettings;
  blue: ChannelSettings;
  hotkey: string | null;
}

export interface GammaRamp {
  red: number[];
  green: number[];
  blue: number[];
}

export interface DisplayBaseline {
  displayId: string;
  ramp: GammaRamp;
}

export interface AppConfig {
  version: number;
  initialDisplayBaselines: DisplayBaseline[];
  displayBaselines: DisplayBaseline[];
  profiles: Profile[];
  selectedProfileId: string | null;
}

export interface ApplyResult {
  profileId: string | null;
  displayId: string;
}
