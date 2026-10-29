use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Daily,
    Movie,
    Custom,
    Off,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum ThemeMode {
    System,
    Light,
    #[default]
    Dark,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub leave_delay_milliseconds: u32,
    pub idle_blackout_milliseconds: u32,
    pub blackout_recovery_milliseconds: u32,
    pub brightness_recovery_milliseconds: u32,
    pub brightness_recovery_frames: u32,
    pub brightness_fade_duration_milliseconds: u32,
    pub brightness_fade_enabled: bool,
    pub blackout_fade_enabled: bool,
    pub brightness_fade_frames: u32,
    pub brightness_fade_exponent: f64,
}

impl Default for Profile {
    fn default() -> Self {
        Self::daily()
    }
}

impl Profile {
    pub fn daily() -> Self {
        Self {
            leave_delay_milliseconds: 1500,
            idle_blackout_milliseconds: 30_000,
            blackout_recovery_milliseconds: 500,
            brightness_recovery_milliseconds: 500,
            brightness_recovery_frames: 5,
            brightness_fade_duration_milliseconds: 300,
            brightness_fade_enabled: true,
            blackout_fade_enabled: true,
            brightness_fade_frames: 30,
            brightness_fade_exponent: 2.4,
        }
    }

    pub fn movie() -> Self {
        Self {
            blackout_fade_enabled: false,
            ..Self::daily()
        }
    }

    pub fn normalize(&mut self) {
        self.leave_delay_milliseconds = self.leave_delay_milliseconds.clamp(500, 10_000);
        self.idle_blackout_milliseconds = self.idle_blackout_milliseconds.clamp(5000, 300_000);
        self.blackout_recovery_milliseconds = self.blackout_recovery_milliseconds.clamp(100, 5000);
        self.brightness_recovery_milliseconds = self.brightness_recovery_milliseconds.min(1000);
        self.brightness_recovery_frames = self.brightness_recovery_frames.clamp(1, 30);
        self.brightness_fade_duration_milliseconds =
            self.brightness_fade_duration_milliseconds.min(3000);
        self.brightness_fade_frames = self.brightness_fade_frames.clamp(5, 60);
        self.brightness_fade_exponent = if self.brightness_fade_exponent.is_finite() {
            self.brightness_fade_exponent.clamp(1.0, 4.0)
        } else {
            Self::daily().brightness_fade_exponent
        };
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub target_stable_id: String,
    pub mode: Mode,
    pub theme_mode: ThemeMode,
    pub custom_profile: Profile,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            target_stable_id: "DELA0E7".into(),
            mode: Mode::Daily,
            theme_mode: ThemeMode::Dark,
            custom_profile: Profile::daily(),
        }
    }
}

impl AppSettings {
    pub fn normalize(&mut self) {
        self.target_stable_id = self.target_stable_id.trim().to_owned();
        self.custom_profile.normalize();
    }

    pub fn get_effective_profile(&self) -> Profile {
        match self.mode {
            Mode::Movie => Profile::movie(),
            Mode::Custom => self.custom_profile.clone(),
            Mode::Daily | Mode::Off => Profile::daily(),
        }
    }
}

pub fn data_dir() -> PathBuf {
    local_data_dir().join("fishcreen")
}

pub(crate) fn legacy_data_dir() -> PathBuf {
    local_data_dir().join("SecondScreenDimmer")
}

fn local_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData/Local")))
        .unwrap_or_else(std::env::temp_dir)
}

pub fn load() -> Result<AppSettings, String> {
    let primary = data_dir().join("settings.txt");
    let legacy = legacy_data_dir().join("settings.txt");
    let path = if primary.exists() { primary } else { legacy };
    match fs::read_to_string(&path) {
        Ok(text) => Ok(parse_settings(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(e) => Err(format!("读取设置失败（{}）：{e}", path.display())),
    }
}

fn parse_settings(text: &str) -> AppSettings {
    let mut settings = AppSettings::default();
    let text = text.trim_start_matches('\u{feff}').trim();
    if !text.contains('=') {
        if let Some(mode) = parse_mode(text) {
            settings.mode = mode;
        }
        return settings;
    }
    let values: HashMap<String, &str> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim()))
        .collect();
    let get = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| values.get(&k.to_ascii_lowercase()).copied())
    };
    if let Some(value) = get(&["TargetStableId"]) {
        settings.target_stable_id = value.to_owned();
    }
    if let Some(mode) = get(&["Mode"]).and_then(parse_mode) {
        settings.mode = mode;
    }
    if let Some(theme) = get(&["ThemeMode"]) {
        settings.theme_mode = match theme.to_ascii_lowercase().as_str() {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        };
    }
    let p = &mut settings.custom_profile;
    macro_rules! number {
        ($field:ident, $($key:literal),+) => {
            if let Some(value) = get(&[$($key),+]).and_then(|v| v.parse::<i64>().ok()) {
                p.$field = value.clamp(0, u32::MAX as i64) as u32;
            }
        };
    }
    number!(
        leave_delay_milliseconds,
        "CustomLeaveDelayMilliseconds",
        "LeaveDelayMilliseconds"
    );
    number!(
        idle_blackout_milliseconds,
        "CustomIdleBlackoutMilliseconds",
        "IdleBlackoutMilliseconds"
    );
    number!(
        blackout_recovery_milliseconds,
        "CustomBlackoutRecoveryMilliseconds"
    );
    number!(
        brightness_recovery_milliseconds,
        "CustomBrightnessRecoveryMilliseconds"
    );
    number!(
        brightness_recovery_frames,
        "CustomBrightnessRecoveryFrames",
        "CustomBrightnessRecoverySteps"
    );
    number!(
        brightness_fade_duration_milliseconds,
        "CustomBrightnessFadeDurationMilliseconds"
    );
    number!(
        brightness_fade_frames,
        "CustomBrightnessFadeFrames",
        "CustomBrightnessFadeSteps"
    );
    if let Some(value) =
        get(&["CustomBrightnessFadeEnabled", "BrightnessFadeEnabled"]).and_then(parse_bool)
    {
        p.brightness_fade_enabled = value;
    }
    if let Some(value) =
        get(&["CustomBlackoutFadeEnabled", "BlackoutFadeEnabled"]).and_then(parse_bool)
    {
        p.blackout_fade_enabled = value;
    }
    if let Some(value) = get(&["CustomBrightnessFadeExponent"]).and_then(|v| v.parse().ok()) {
        p.brightness_fade_exponent = value;
    }
    settings.normalize();
    settings
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_mode(value: &str) -> Option<Mode> {
    match value.to_ascii_lowercase().as_str() {
        "daily" => Some(Mode::Daily),
        "movie" => Some(Mode::Movie),
        "custom" => Some(Mode::Custom),
        "off" => Some(Mode::Off),
        _ => None,
    }
}

pub fn save(settings: &AppSettings) -> Result<(), String> {
    let mut settings = settings.clone();
    settings.normalize();
    let directory = data_dir();
    fs::create_dir_all(&directory).map_err(|e| format!("创建设置目录失败：{e}"))?;
    let path = directory.join("settings.txt");
    let backup = directory.join("settings.before-tauri.txt");
    if path.exists() && !backup.exists() {
        fs::copy(&path, &backup).map_err(|e| format!("备份原版设置失败：{e}"))?;
    }
    atomic_write(&path, settings_text(&settings).as_bytes())
}

fn settings_text(settings: &AppSettings) -> String {
    let p = &settings.custom_profile;
    format!(
        concat!(
            "TargetStableId={}\r\nMode={:?}\r\nThemeMode={:?}\r\n",
            "CustomLeaveDelayMilliseconds={}\r\nCustomIdleBlackoutMilliseconds={}\r\n",
            "CustomBlackoutRecoveryMilliseconds={}\r\nCustomBrightnessRecoveryMilliseconds={}\r\n",
            "CustomBrightnessRecoveryFrames={}\r\nCustomBrightnessFadeDurationMilliseconds={}\r\n",
            "CustomBrightnessFadeEnabled={}\r\nCustomBlackoutFadeEnabled={}\r\n",
            "CustomBrightnessFadeFrames={}\r\nCustomBrightnessFadeExponent={:.1}\r\n"
        ),
        settings.target_stable_id,
        settings.mode,
        settings.theme_mode,
        p.leave_delay_milliseconds,
        p.idle_blackout_milliseconds,
        p.blackout_recovery_milliseconds,
        p.brightness_recovery_milliseconds,
        p.brightness_recovery_frames,
        p.brightness_fade_duration_milliseconds,
        p.brightness_fade_enabled,
        p.blackout_fade_enabled,
        p.brightness_fade_frames,
        p.brightness_fade_exponent,
    )
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建数据目录失败：{e}"))?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut file =
            fs::File::create(&temporary).map_err(|e| format!("创建临时文件失败：{e}"))?;
        file.write_all(bytes)
            .map_err(|e| format!("写入临时文件失败：{e}"))?;
        file.sync_all().map_err(|e| format!("保存数据失败：{e}"))?;
        drop(file);
        replace_file(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 1 | 8) } == 0 {
        Err(format!(
            "替换数据文件失败：{}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|e| format!("替换数据文件失败：{e}"))
}

pub fn log(message: &str) {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let Ok(_guard) = LOCK.get_or_init(|| Mutex::new(())).lock() else {
        return;
    };
    let directory = data_dir();
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("app.log"))
    {
        let _ = writeln!(
            file,
            "{} [Rust] {message}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_steps_migrate_and_frames_take_precedence() {
        let settings = parse_settings(concat!(
            "\u{feff}Mode=Custom\r\nThemeMode=Dark\r\nTargetStableId=DELA0E7\r\n",
            "CustomBrightnessFadeSteps=15\r\nCustomBrightnessFadeFrames=42\r\n",
            "CustomBrightnessRecoverySteps=9\r\nCustomBrightnessRecoveryMilliseconds=0\r\n"
        ));
        assert_eq!(settings.mode, Mode::Custom);
        assert_eq!(settings.theme_mode, ThemeMode::Dark);
        assert_eq!(settings.custom_profile.brightness_fade_frames, 42);
        assert_eq!(settings.custom_profile.brightness_recovery_frames, 9);
        assert_eq!(settings.custom_profile.brightness_recovery_milliseconds, 0);
        assert_eq!(parse_settings(&settings_text(&settings)), settings);
    }

    #[test]
    fn custom_changes_do_not_modify_fixed_presets() {
        let mut settings = AppSettings::default();
        settings
            .custom_profile
            .brightness_fade_duration_milliseconds = 2500;
        settings.custom_profile.brightness_fade_frames = 60;
        assert_eq!(settings.get_effective_profile(), Profile::daily());
        settings.mode = Mode::Movie;
        assert_eq!(settings.get_effective_profile(), Profile::movie());
        settings.mode = Mode::Custom;
        assert_eq!(
            settings
                .get_effective_profile()
                .brightness_fade_duration_milliseconds,
            2500
        );
    }

    #[test]
    fn invalid_legacy_values_are_bounded_without_erasing_target() {
        let settings = parse_settings(concat!(
            "TargetStableId=MISSING\nCustomLeaveDelayMilliseconds=-10\n",
            "CustomBrightnessFadeDurationMilliseconds=99999\n",
            "CustomBrightnessRecoveryFrames=99999\nCustomBrightnessFadeExponent=NaN\n"
        ));
        assert_eq!(settings.target_stable_id, "MISSING");
        assert_eq!(settings.custom_profile.leave_delay_milliseconds, 500);
        assert_eq!(
            settings
                .custom_profile
                .brightness_fade_duration_milliseconds,
            3000
        );
        assert_eq!(settings.custom_profile.brightness_recovery_frames, 30);
        assert_eq!(settings.custom_profile.brightness_fade_exponent, 2.4);
    }

    #[test]
    fn one_line_legacy_mode_and_missing_new_fields_keep_defaults() {
        assert_eq!(parse_settings("movie\r\n").mode, Mode::Movie);
        assert_eq!(
            parse_settings("Mode=Custom\n").custom_profile,
            Profile::daily()
        );
    }
}
