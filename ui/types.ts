export type Mode = 'Daily' | 'Movie' | 'Custom' | 'Off';
export type ThemeMode = 'System' | 'Light' | 'Dark';
export interface Profile {
  leaveDelayMilliseconds: number;
  idleBlackoutMilliseconds: number;
  blackoutRecoveryMilliseconds: number;
  brightnessRecoveryMilliseconds: number;
  brightnessRecoveryFrames: number;
  brightnessFadeDurationMilliseconds: number;
  brightnessFadeEnabled: boolean;
  blackoutFadeEnabled: boolean;
  brightnessFadeFrames: number;
  brightnessFadeExponent: number;
}
export interface Settings {
  targetStableId: string;
  mode: Mode;
  themeMode: ThemeMode;
  customProfile: Profile;
}
export interface Monitor {
  stableId: string;
  name: string;
  gdiName: string;
  bounds: { left: number; top: number; right: number; bottom: number };
  primary: boolean;
}
export interface AppState {
  settings: Settings;
  monitors: Monitor[];
  status: string;
  brightnessDimmed: boolean;
  overlayOpacity: number;
  startupEnabled: boolean;
  version: string;
  shell: string;
}
