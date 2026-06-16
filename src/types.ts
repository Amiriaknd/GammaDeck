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

export interface AppConfig {
  version: number;
  profiles: Profile[];
  selectedProfileId: string | null;
}

export interface ApplyResult {
  profileId: string | null;
  displayId: string;
}
