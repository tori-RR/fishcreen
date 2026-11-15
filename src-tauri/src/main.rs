#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_log;
mod brightness_transition;
mod config;
mod engine;
mod native;
mod overlay_animation;
mod tray_refresh;

use config::{AppSettings, Mode, ThemeMode};
use engine::{Engine, Snapshot};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, RunEvent, State, Theme, WebviewUrl, WebviewWindowBuilder,
};

struct AppCore {
    engine: Arc<Engine>,
    exiting: AtomicBool,
    tray_items: OnceLock<TrayItems>,
    tray_refresh: Arc<tray_refresh::RefreshGate>,
}

struct TrayItems {
    modes: Vec<(Mode, CheckMenuItem<tauri::Wry>)>,
    startup: CheckMenuItem<tauri::Wry>,
    status: MenuItem<tauri::Wry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UiState {
    #[serde(flatten)]
    snapshot: Snapshot,
    startup_enabled: bool,
    version: &'static str,
    shell: &'static str,
}

fn state_for(core: &AppCore) -> UiState {
    UiState {
        snapshot: core.engine.snapshot(),
        startup_enabled: native::startup_enabled().unwrap_or(false),
        version: env!("CARGO_PKG_VERSION"),
        shell: "Tauri",
    }
}

fn sync_tray(app: &AppHandle) {
    let gate = app.state::<AppCore>().tray_refresh.clone();
    let dispatcher = app.clone();
    let refresh_app = app.clone();
    if let Err(error) = gate.request(
        move |task| dispatcher.run_on_main_thread(task),
        move || refresh_tray_on_main(&refresh_app),
    ) {
        config::log_error(&format!("刷新托盘失败：{error}"));
    }
}

fn refresh_tray_on_main(app: &AppHandle) {
    let core = app.state::<AppCore>();
    let Some(items) = core.tray_items.get() else {
        return;
    };
    let snapshot = core.engine.snapshot();
    for (mode, item) in &items.modes {
        let _ = item.set_checked(*mode == snapshot.settings.mode);
    }
    let _ = items
        .startup
        .set_checked(native::startup_enabled().unwrap_or(false));
    let _ = items.status.set_text(&snapshot.status);
}

fn window_theme(theme: ThemeMode) -> Option<Theme> {
    match theme {
        ThemeMode::System => None,
        ThemeMode::Light => Some(Theme::Light),
        ThemeMode::Dark => Some(Theme::Dark),
    }
}

fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let theme = app.state::<AppCore>().engine.snapshot().settings.theme_mode;
    let result = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("fishcreen 设置")
        .inner_size(480.0, 880.0)
        .min_inner_size(420.0, 520.0)
        .center()
        .theme(window_theme(theme))
        .build();
    if let Err(error) = result {
        config::log_error(&format!("打开设置面板失败：{error}"));
    }
}

#[tauri::command]
fn get_state(core: State<'_, AppCore>) -> UiState {
    state_for(&core)
}

fn update_startup_preference(
    requested: Option<bool>,
    read: impl FnOnce() -> Result<bool, String>,
    write: impl FnOnce(bool) -> Result<(), String>,
) -> Result<Option<(bool, bool)>, String> {
    let Some(desired) = requested else {
        return Ok(None);
    };
    let old = read()?;
    if old == desired {
        return Ok(None);
    }
    write(desired)?;
    Ok(Some((old, desired)))
}

#[tauri::command]
async fn apply_settings(
    app: AppHandle,
    core: State<'_, AppCore>,
    settings: AppSettings,
    startup_enabled: Option<bool>,
) -> Result<UiState, String> {
    let engine = core.engine.clone();
    // An untouched startup field must not overwrite a newer tray/registry value.
    let startup_change = update_startup_preference(
        startup_enabled,
        native::startup_enabled,
        native::set_startup_enabled,
    )?;
    let theme = settings.theme_mode;
    let applied = tauri::async_runtime::spawn_blocking(move || engine.apply_settings(settings))
        .await
        .map_err(|e| e.to_string())
        .and_then(|result| result);
    if let Err(error) = applied {
        if let Some((old, desired)) = startup_change {
            if native::startup_enabled().ok() == Some(desired) {
                let _ = native::set_startup_enabled(old);
            }
        }
        return Err(error);
    }
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_theme(window_theme(theme));
    }
    sync_tray(&app);
    Ok(state_for(&core))
}

#[tauri::command]
async fn restore_brightness(core: State<'_, AppCore>) -> Result<(), String> {
    let engine = core.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.force_restore())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn confirm_recovery_target(
    app: AppHandle,
    core: State<'_, AppCore>,
    target: String,
) -> Result<UiState, String> {
    let engine = core.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.confirm_recovery_target(target))
        .await
        .map_err(|e| e.to_string())??;
    sync_tray(&app);
    Ok(state_for(&core))
}

#[tauri::command]
fn close_settings(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let mode = app.state::<AppCore>().engine.snapshot().settings.mode;
    let daily = CheckMenuItem::with_id(
        app,
        "daily",
        "日常模式",
        true,
        mode == Mode::Daily,
        None::<&str>,
    )?;
    let movie = CheckMenuItem::with_id(
        app,
        "movie",
        "观影模式",
        true,
        mode == Mode::Movie,
        None::<&str>,
    )?;
    let custom = CheckMenuItem::with_id(
        app,
        "custom",
        "自定义",
        true,
        mode == Mode::Custom,
        None::<&str>,
    )?;
    let off = CheckMenuItem::with_id(
        app,
        "off",
        "关闭自动暗屏",
        true,
        mode == Mode::Off,
        None::<&str>,
    )?;
    let startup = CheckMenuItem::with_id(
        app,
        "startup",
        "开机自动启动",
        true,
        native::startup_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let status = MenuItem::with_id(app, "status", "正在启动…", false, None::<&str>)?;
    let restore = MenuItem::with_id(app, "restore", "立即恢复副屏", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &settings, &sep1, &daily, &movie, &custom, &off, &startup, &status, &sep2, &restore,
            &sep3, &exit,
        ],
    )?;
    let items = TrayItems {
        modes: vec![
            (Mode::Daily, daily),
            (Mode::Movie, movie),
            (Mode::Custom, custom),
            (Mode::Off, off),
        ],
        startup,
        status,
    };
    app.state::<AppCore>()
        .tray_items
        .set(items)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::AlreadyExists, "托盘已初始化"))?;
    let mut builder = TrayIconBuilder::with_id("fishcreen")
        .tooltip("fishcreen · Rust + Tauri")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    ..
                }
            ) {
                sync_tray(tray.app_handle());
            }
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_settings(tray.app_handle());
            }
        })
        .on_menu_event(move |app, event| {
            let selected = match event.id().as_ref() {
                "daily" => Some(Mode::Daily),
                "movie" => Some(Mode::Movie),
                "custom" => Some(Mode::Custom),
                "off" => Some(Mode::Off),
                _ => None,
            };
            if let Some(mode) = selected {
                let engine = app.state::<AppCore>().engine.clone();
                let app = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(e) = engine.set_mode(mode) {
                        config::log_error(&e);
                    }
                    sync_tray(&app);
                });
                return;
            }
            match event.id().as_ref() {
                "settings" => show_settings(app),
                "startup" => {
                    match native::startup_enabled()
                        .and_then(|enabled| native::set_startup_enabled(!enabled))
                    {
                        Ok(()) => {}
                        Err(e) => config::log_error(&e),
                    };
                    sync_tray(app);
                }
                "restore" => {
                    let engine = app.state::<AppCore>().engine.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Err(e) = engine.force_restore() {
                            config::log_error(&e)
                        }
                    });
                }
                "exit" => {
                    app.state::<AppCore>()
                        .exiting
                        .store(true, Ordering::Release);
                    let engine = app.state::<AppCore>().engine.clone();
                    let app = app.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        engine.shutdown();
                        app.exit(0);
                    });
                }
                _ => {}
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

fn main() {
    native::set_dpi_awareness();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|v| v == "--diagnose") {
        let diagnostics = serde_json::json!({ "version": env!("CARGO_PKG_VERSION"), "shell": "Tauri", "settings": config::load(), "monitors": native::enumerate_monitors(), "startupEnabled": native::startup_enabled() });
        let path = config::data_dir().join("diagnostics.json");
        let _ = std::fs::create_dir_all(config::data_dir());
        let _ = std::fs::write(
            path,
            serde_json::to_string_pretty(&diagnostics).unwrap_or_default(),
        );
        return;
    }
    let _instance = match native::SingleInstanceGuard::acquire() {
        Ok(Some(guard)) => guard,
        Ok(None) => return,
        Err(e) => {
            config::log_error(&e);
            return;
        }
    };
    let mut settings = match config::load() {
        Ok(value) => value,
        Err(e) => {
            config::log_error(&e);
            return;
        }
    };
    if args.iter().any(|v| v == "--paused") {
        settings.mode = Mode::Off;
    }
    let engine = match Engine::new(settings) {
        Ok(engine) => engine,
        Err(e) => {
            config::log_error(&e);
            return;
        }
    };
    let recovery_engine = engine.clone();
    let failed_start_engine = engine.clone();
    std::panic::set_hook(Box::new(move |info| {
        config::log_error(&format!("Rust panic: {info}"));
        let _ = recovery_engine.force_restore();
    }));
    let should_show = args.iter().any(|v| v == "--settings");
    let app = tauri::Builder::default()
        .manage(AppCore {
            engine,
            exiting: AtomicBool::new(false),
            tray_items: OnceLock::new(),
            tray_refresh: Arc::new(tray_refresh::RefreshGate::default()),
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            apply_settings,
            restore_brightness,
            confirm_recovery_target,
            close_settings
        ])
        .setup(move |app| {
            build_tray(app.handle())?;
            if should_show {
                show_settings(app.handle());
            }
            config::log("fishcreen Rust + Tauri 核心已启动；设置 WebView 按需创建。");
            Ok(())
        })
        .build(tauri::generate_context!());
    match app {
        Ok(app) => app.run(|app, event| match event {
            RunEvent::ExitRequested { api, code, .. } => {
                if code.is_none() && !app.state::<AppCore>().exiting.load(Ordering::Acquire) {
                    api.prevent_exit();
                }
            }
            RunEvent::Exit => {
                app.state::<AppCore>()
                    .exiting
                    .store(true, Ordering::Release);
                app.state::<AppCore>().engine.shutdown();
            }
            RunEvent::Resumed => {
                app.state::<AppCore>().engine.refresh_monitors();
            }
            _ => {}
        }),
        Err(e) => {
            config::log_error(&format!("Tauri 启动失败：{e}"));
            failed_start_engine.shutdown();
        }
    }
}

#[cfg(test)]
mod command_tests {
    use super::*;

    #[test]
    fn untouched_startup_preference_never_reads_or_writes_the_registry() {
        let result = update_startup_preference(
            None,
            || panic!("untouched field must not be read"),
            |_| panic!("untouched field must not be written"),
        );
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn unchanged_explicit_startup_preference_does_not_write() {
        assert_eq!(
            update_startup_preference(Some(true), || Ok(true), |_| panic!("no change")),
            Ok(None)
        );
    }

    #[test]
    fn explicitly_edited_startup_preference_writes_the_requested_value() {
        let mut actual = None;
        let result = update_startup_preference(
            Some(false),
            || Ok(true),
            |value| {
                actual = Some(value);
                Ok(())
            },
        );
        assert_eq!(actual, Some(false));
        assert_eq!(result, Ok(Some((true, false))));
    }

    #[test]
    fn startup_failure_is_returned_before_settings_are_applied() {
        assert_eq!(
            update_startup_preference(Some(true), || Ok(false), |_| Err("denied".into())),
            Err("denied".into())
        );
    }
}
