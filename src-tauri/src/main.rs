#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod engine;
mod native;

use config::{AppSettings, Mode, ThemeMode};
use engine::{Engine, Snapshot};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, RunEvent, State, Theme, WebviewUrl, WebviewWindowBuilder,
};

struct AppCore {
    engine: Arc<Engine>,
    exiting: AtomicBool,
    mode_items: Mutex<Vec<(Mode, CheckMenuItem<tauri::Wry>)>>,
    startup_item: Mutex<Option<CheckMenuItem<tauri::Wry>>>,
    status_item: Mutex<Option<MenuItem<tauri::Wry>>>,
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
    let core = app.state::<AppCore>();
    let snapshot = core.engine.snapshot();
    for (mode, item) in core
        .mode_items
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
    {
        let _ = item.set_checked(*mode == snapshot.settings.mode);
    }
    if let Some(item) = core
        .startup_item
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        let _ = item.set_checked(native::startup_enabled().unwrap_or(false));
    }
    if let Some(item) = core
        .status_item
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        let _ = item.set_text(&snapshot.status);
    };
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
        .inner_size(760.0, 880.0)
        .min_inner_size(540.0, 520.0)
        .center()
        .theme(window_theme(theme))
        .build();
    if let Err(error) = result {
        config::log(&format!("打开设置面板失败：{error}"));
    }
}

#[tauri::command]
fn get_state(app: AppHandle, core: State<'_, AppCore>) -> UiState {
    sync_tray(&app);
    state_for(&core)
}

#[tauri::command]
async fn apply_settings(
    app: AppHandle,
    core: State<'_, AppCore>,
    settings: AppSettings,
    startup_enabled: bool,
) -> Result<UiState, String> {
    let engine = core.engine.clone();
    let old = native::startup_enabled()?;
    if old != startup_enabled {
        native::set_startup_enabled(startup_enabled)?;
    }
    let theme = settings.theme_mode;
    let applied = tauri::async_runtime::spawn_blocking(move || engine.apply_settings(settings))
        .await
        .map_err(|e| e.to_string())?;
    if let Err(error) = applied {
        if old != startup_enabled {
            let _ = native::set_startup_enabled(old);
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
    *app.state::<AppCore>()
        .mode_items
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = vec![
        (Mode::Daily, daily),
        (Mode::Movie, movie),
        (Mode::Custom, custom),
        (Mode::Off, off),
    ];
    *app.state::<AppCore>()
        .startup_item
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(startup);
    *app.state::<AppCore>()
        .status_item
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(status);
    let mut builder = TrayIconBuilder::with_id("fishcreen")
        .tooltip("fishcreen · Rust + Tauri")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            sync_tray(tray.app_handle());
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
                        config::log(&e);
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
                        Err(e) => config::log(&e),
                    };
                    sync_tray(app);
                }
                "restore" => {
                    let engine = app.state::<AppCore>().engine.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Err(e) = engine.force_restore() {
                            config::log(&e)
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
            config::log(&e);
            return;
        }
    };
    let mut settings = match config::load() {
        Ok(value) => value,
        Err(e) => {
            config::log(&e);
            return;
        }
    };
    if args.iter().any(|v| v == "--paused") {
        settings.mode = Mode::Off;
    }
    let engine = match Engine::new(settings) {
        Ok(engine) => engine,
        Err(e) => {
            config::log(&e);
            return;
        }
    };
    let recovery_engine = engine.clone();
    let failed_start_engine = engine.clone();
    std::panic::set_hook(Box::new(move |info| {
        config::log(&format!("Rust panic: {info}"));
        let _ = recovery_engine.force_restore();
    }));
    let should_show = args.iter().any(|v| v == "--settings");
    let app = tauri::Builder::default()
        .manage(AppCore {
            engine,
            exiting: AtomicBool::new(false),
            mode_items: Mutex::new(Vec::new()),
            startup_item: Mutex::new(None),
            status_item: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            apply_settings,
            restore_brightness,
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
            config::log(&format!("Tauri 启动失败：{e}"));
            failed_start_engine.shutdown();
        }
    }
}
