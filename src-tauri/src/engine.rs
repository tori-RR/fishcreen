use crate::{
    config::{self, AppSettings, Mode, Profile},
    native::{self, BrightnessHandle, Monitor, Overlay, Rect},
};
use serde::Serialize;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const CURSOR_INTERVAL: Duration = Duration::from_millis(75);
const ANIMATION_INTERVAL: Duration = Duration::from_millis(16);
const MONITOR_INTERVAL: Duration = Duration::from_secs(3);
const RETRY_INTERVAL: Duration = Duration::from_secs(2);
const MAX_ATTEMPTS: u8 = 3;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: AppSettings,
    pub monitors: Vec<Monitor>,
    pub status: String,
    pub brightness_dimmed: bool,
    pub overlay_opacity: f64,
}

pub struct Engine {
    commands: Sender<Command>,
    shared: Arc<Mutex<Snapshot>>,
    generation: Arc<AtomicU64>,
    stopped: AtomicBool,
    controller: Mutex<Option<JoinHandle<()>>>,
}

enum Command {
    Apply(AppSettings, Sender<Result<(), String>>),
    Restore(Sender<Result<(), String>>),
    Refresh,
    Shutdown(Sender<()>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrightnessIntent {
    Restored,
    Dimmed,
}

struct IntentTracker {
    current: BrightnessIntent,
    generation: Arc<AtomicU64>,
}

impl IntentTracker {
    fn new(generation: Arc<AtomicU64>) -> Self {
        Self {
            current: BrightnessIntent::Restored,
            generation,
        }
    }

    fn transition(&mut self, next: BrightnessIntent, force: bool) -> bool {
        if self.current == next && !force {
            return false;
        }
        self.current = next;
        self.generation.fetch_add(1, Ordering::AcqRel);
        true
    }
}

struct BrightnessRequest {
    generation: u64,
    target: String,
    intent: BrightnessIntent,
    profile: Profile,
    immediate: bool,
}

enum WorkerCommand {
    Change(BrightnessRequest),
    Shutdown(Sender<Result<(), String>>),
}

struct Feedback {
    generation: u64,
    intent: BrightnessIntent,
    outcome: Result<bool, String>,
}

impl Engine {
    pub fn new(mut settings: AppSettings) -> Result<Arc<Self>, String> {
        settings.normalize();
        let overlay = Overlay::new()?;
        let monitors = native::enumerate_monitors().unwrap_or_default();
        let recovery_pending = load_recovery().map(|v| v.is_some()).unwrap_or(true);
        let shared = Arc::new(Mutex::new(Snapshot {
            settings: settings.clone(),
            monitors,
            status: "正在启动 Rust 调光核心…".into(),
            brightness_dimmed: recovery_pending,
            overlay_opacity: 0.0,
        }));
        let generation = Arc::new(AtomicU64::new(0));
        let (commands, receiver) = mpsc::channel();
        let engine = Arc::new(Self {
            commands,
            shared: shared.clone(),
            generation: generation.clone(),
            stopped: AtomicBool::new(false),
            controller: Mutex::new(None),
        });
        let handle = thread::Builder::new()
            .name("fishcreen-control".into())
            .spawn(move || {
                Controller::run(
                    settings,
                    overlay,
                    shared,
                    generation,
                    receiver,
                    recovery_pending,
                )
            })
            .map_err(|e| format!("启动调光线程失败：{e}"))?;
        *engine.controller.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        Ok(engine)
    }

    pub fn snapshot(&self) -> Snapshot {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn apply_settings(&self, mut settings: AppSettings) -> Result<(), String> {
        if self.stopped.load(Ordering::Acquire) {
            return Err("调光核心已停止".into());
        }
        settings.normalize();
        let (reply, result) = mpsc::channel();
        self.commands
            .send(Command::Apply(settings, reply))
            .map_err(|_| "调光核心未响应".to_string())?;
        result
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "应用设置超时，请稍后重试".to_string())?
    }

    pub fn set_mode(&self, mode: Mode) -> Result<(), String> {
        let mut settings = self.snapshot().settings;
        settings.mode = mode;
        self.apply_settings(settings)
    }

    pub fn force_restore(&self) -> Result<(), String> {
        let (reply, result) = mpsc::channel();
        self.commands
            .send(Command::Restore(reply))
            .map_err(|_| "调光核心未响应".to_string())?;
        result
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "恢复请求超时".to_string())?
    }

    pub fn refresh_monitors(&self) {
        let _ = self.commands.send(Command::Refresh);
    }

    pub fn shutdown(&self) {
        if self.stopped.swap(true, Ordering::AcqRel) {
            return;
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        let (reply, result) = mpsc::channel();
        if self.commands.send(Command::Shutdown(reply)).is_ok()
            && result.recv_timeout(Duration::from_secs(6)).is_err()
        {
            config::log("退出恢复超时；亮度恢复记录仍保留，下次启动继续恢复。");
        }
        if let Some(handle) = self
            .controller
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Controller {
    settings: AppSettings,
    overlay: Overlay,
    shared: Arc<Mutex<Snapshot>>,
    generation: Arc<AtomicU64>,
    worker: Sender<WorkerCommand>,
    feedback: Receiver<Feedback>,
    previous_cursor: Option<(i32, i32)>,
    last_movement: Instant,
    outside_since: Option<Instant>,
    next_monitor_refresh: Instant,
    next_cursor_check: Instant,
    intent: IntentTracker,
    operation_pending: bool,
    may_need_restore: bool,
    attempts: u8,
    retry_after: Option<Instant>,
    immediate_restore: bool,
    opacity: f64,
    overlay_bounds: Option<Rect>,
    overlay_recovery: Option<(Instant, f64)>,
    target_available: bool,
    input_available: bool,
    system_events: Option<Receiver<native::SystemEvent>>,
    suspended: bool,
}

impl Controller {
    fn run(
        settings: AppSettings,
        mut overlay: Overlay,
        shared: Arc<Mutex<Snapshot>>,
        generation: Arc<AtomicU64>,
        commands: Receiver<Command>,
        recovery_pending: bool,
    ) {
        let (worker, requests) = mpsc::channel();
        let (feedback_sender, feedback) = mpsc::channel();
        let worker_generation = generation.clone();
        let worker_handle = match thread::Builder::new()
            .name("fishcreen-ddc".into())
            .spawn(move || brightness_worker(requests, feedback_sender, worker_generation))
        {
            Ok(handle) => handle,
            Err(error) => {
                shared.lock().unwrap_or_else(|e| e.into_inner()).status =
                    format!("启动 DDC 线程失败：{error}");
                return;
            }
        };
        let system_events = overlay.take_system_events();
        let now = Instant::now();
        let mut this = Self {
            settings,
            overlay,
            shared,
            generation: generation.clone(),
            worker,
            feedback,
            previous_cursor: None,
            last_movement: now,
            outside_since: None,
            next_monitor_refresh: now,
            next_cursor_check: now,
            intent: IntentTracker::new(generation),
            operation_pending: false,
            may_need_restore: recovery_pending,
            attempts: 0,
            retry_after: None,
            immediate_restore: true,
            opacity: 0.0,
            overlay_bounds: None,
            overlay_recovery: None,
            target_available: false,
            input_available: true,
            system_events,
            suspended: false,
        };
        if recovery_pending {
            this.queue_brightness();
        }
        let mut shutdown_reply = None;
        loop {
            while let Ok(feedback) = this.feedback.try_recv() {
                this.receive_feedback(feedback);
            }
            let events: Vec<_> = this
                .system_events
                .as_ref()
                .map(|events| events.try_iter().collect())
                .unwrap_or_default();
            for event in events {
                match event {
                    native::SystemEvent::Suspend | native::SystemEvent::SessionEnding => {
                        this.suspended = true;
                        this.hide_overlay();
                        this.transition(BrightnessIntent::Restored, true, true);
                    }
                    native::SystemEvent::Resume => {
                        this.suspended = false;
                        this.reset_activity();
                        this.refresh_monitors();
                        if this.may_need_restore {
                            this.transition(BrightnessIntent::Restored, true, true);
                        }
                    }
                }
            }
            let now = Instant::now();
            if now >= this.next_monitor_refresh {
                this.refresh_monitors();
            }
            if now >= this.next_cursor_check {
                this.next_cursor_check = now + CURSOR_INTERVAL;
                this.check_cursor(now);
            }
            this.animate_overlay(now);
            if !this.operation_pending
                && this.attempts < MAX_ATTEMPTS
                && this.retry_after.is_some_and(|at| now >= at)
            {
                this.queue_brightness();
            }
            let timeout =
                if this.overlay_recovery.is_some() || (this.opacity > 0.0 && this.opacity < 1.0) {
                    ANIMATION_INTERVAL
                } else {
                    CURSOR_INTERVAL
                };
            match commands.recv_timeout(timeout) {
                Ok(Command::Apply(mut settings, reply)) => {
                    settings.normalize();
                    match config::save(&settings) {
                        Ok(()) => {
                            this.transition(BrightnessIntent::Restored, true, true);
                            this.hide_overlay();
                            this.reset_activity();
                            this.settings = settings.clone();
                            this.shared
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .settings = settings;
                            this.refresh_monitors();
                            let _ = reply.send(Ok(()));
                        }
                        Err(error) => {
                            this.status(&error);
                            let _ = reply.send(Err(error));
                        }
                    }
                }
                Ok(Command::Restore(reply)) => {
                    this.hide_overlay();
                    this.reset_activity();
                    this.transition(BrightnessIntent::Restored, true, true);
                    let _ = reply.send(Ok(()));
                }
                Ok(Command::Refresh) => this.refresh_monitors(),
                Ok(Command::Shutdown(reply)) => {
                    shutdown_reply = Some(reply);
                    break;
                }
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
        this.generation.fetch_add(1, Ordering::AcqRel);
        this.hide_overlay();
        let (done, result) = mpsc::channel();
        let _ = this.worker.send(WorkerCommand::Shutdown(done));
        match result.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => config::log("调光核心已停止，退出亮度恢复完成。"),
            Ok(Err(error)) => config::log(&format!("退出恢复失败：{error}")),
            Err(_) => config::log("DDC 退出恢复未在 5 秒内完成；保留恢复记录。"),
        }
        if worker_handle.is_finished() {
            let _ = worker_handle.join();
        }
        if let Some(reply) = shutdown_reply {
            let _ = reply.send(());
        }
    }

    fn status(&self, message: &str) {
        let mut snapshot = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        if snapshot.status != message {
            config::log(message);
            snapshot.status = message.into();
        }
    }

    fn reset_activity(&mut self) {
        self.last_movement = Instant::now();
        self.outside_since = None;
        self.previous_cursor = None;
    }

    fn refresh_monitors(&mut self) {
        self.next_monitor_refresh = Instant::now() + MONITOR_INTERVAL;
        match native::enumerate_monitors() {
            Ok(monitors) => {
                let available = monitors.iter().any(|m| {
                    m.stable_id
                        .eq_ignore_ascii_case(&self.settings.target_stable_id)
                });
                self.shared
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .monitors = monitors;
                if self.target_available && !available {
                    self.hide_overlay();
                    self.transition(BrightnessIntent::Restored, true, true);
                } else if !self.target_available && available {
                    self.reset_activity();
                    if self.may_need_restore {
                        self.transition(BrightnessIntent::Restored, true, true);
                    } else {
                        self.status(mode_status(self.settings.mode));
                    }
                }
                self.target_available = available;
                if !available {
                    self.status("未找到已选择的屏幕，已暂停自动暗屏");
                }
            }
            Err(error) => self.status(&format!("刷新显示器失败：{error}")),
        }
    }

    fn check_cursor(&mut self, now: Instant) {
        if self.settings.mode == Mode::Off || self.suspended {
            return;
        }
        let cursor = match native::cursor_position() {
            Ok(cursor) => {
                if !self.input_available {
                    self.reset_activity();
                    self.input_available = true;
                }
                cursor
            }
            Err(error) => {
                if self.input_available {
                    self.input_available = false;
                    self.hide_overlay();
                    self.transition(BrightnessIntent::Restored, true, true);
                    self.status(&format!("输入桌面不可用，已暂停暗屏：{error}"));
                }
                return;
            }
        };
        let target = {
            let snapshot = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            snapshot
                .monitors
                .iter()
                .find(|m| {
                    m.stable_id
                        .eq_ignore_ascii_case(&self.settings.target_stable_id)
                })
                .cloned()
        };
        let Some(target) = target else { return };
        let profile = self.settings.get_effective_profile();
        let moved = self.previous_cursor != Some(cursor);
        if moved {
            self.previous_cursor = Some(cursor);
            self.last_movement = now;
            if self.settings.mode != Mode::Movie {
                self.begin_overlay_recovery(now);
            }
        }
        let inside = target.bounds.contains(cursor.0, cursor.1);
        let outside_ms = if inside {
            self.outside_since = None;
            0
        } else {
            now.duration_since(*self.outside_since.get_or_insert(now))
                .as_millis() as u64
        };
        let idle_ms = now.duration_since(self.last_movement).as_millis() as u64;
        let intent = desired_brightness(self.settings.mode, inside, outside_ms, idle_ms, &profile);
        if let Some(intent) = intent {
            self.transition(intent, false, false);
        }
        if self.settings.mode == Mode::Movie {
            if inside {
                self.begin_overlay_recovery(now);
            } else if outside_ms >= profile.leave_delay_milliseconds as u64 {
                self.overlay_recovery = None;
                self.set_overlay(&target, 1.0);
            }
        } else if idle_ms >= profile.idle_blackout_milliseconds as u64 {
            self.overlay_recovery = None;
            let progress = if profile.blackout_fade_enabled {
                (idle_ms - profile.idle_blackout_milliseconds as u64) as f64 / 5000.0
            } else {
                1.0
            };
            self.set_overlay(&target, progress.clamp(0.0, 1.0));
        }
    }

    fn transition(&mut self, intent: BrightnessIntent, immediate: bool, force: bool) {
        // Repeated cursor samples are not a new operation and must not cancel restoration.
        if !self.intent.transition(intent, force) {
            return;
        }
        self.immediate_restore = immediate;
        self.operation_pending = false;
        self.attempts = 0;
        self.retry_after = None;
        if intent == BrightnessIntent::Dimmed {
            self.may_need_restore = true;
        }
        if self.may_need_restore || force {
            self.queue_brightness();
        }
    }

    fn queue_brightness(&mut self) {
        self.operation_pending = true;
        self.retry_after = None;
        self.attempts += 1;
        let request = BrightnessRequest {
            generation: self.generation.load(Ordering::Acquire),
            target: self.settings.target_stable_id.clone(),
            intent: self.intent.current,
            profile: self.settings.get_effective_profile(),
            immediate: self.immediate_restore,
        };
        self.status(match self.intent.current {
            BrightnessIntent::Dimmed => "正在降低目标屏幕亮度…",
            BrightnessIntent::Restored => "正在恢复目标屏幕亮度…",
        });
        if self.worker.send(WorkerCommand::Change(request)).is_err() {
            self.operation_pending = false;
            self.status("DDC 线程已停止，请重新启动 fishcreen");
        }
    }

    fn receive_feedback(&mut self, feedback: Feedback) {
        if feedback.generation != self.generation.load(Ordering::Acquire) {
            return;
        }
        self.operation_pending = false;
        match feedback.outcome {
            Ok(completed) => {
                if completed {
                    let dimmed = feedback.intent == BrightnessIntent::Dimmed;
                    self.shared
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .brightness_dimmed = dimmed;
                    if !dimmed {
                        self.may_need_restore = false;
                    }
                    self.status(if dimmed {
                        "目标屏幕已调至最低亮度"
                    } else {
                        "目标屏幕亮度已恢复"
                    });
                }
            }
            Err(error) => {
                if self.attempts < MAX_ATTEMPTS {
                    self.retry_after = Some(Instant::now() + RETRY_INTERVAL);
                    self.status(&format!(
                        "{error}；2 秒后重试（{}/{MAX_ATTEMPTS}）",
                        self.attempts
                    ));
                } else {
                    self.status(&format!("{error}；已停止重试，可点击立即恢复"));
                }
            }
        }
    }

    fn set_overlay(&mut self, target: &Monitor, opacity: f64) {
        let opacity = opacity.clamp(0.0, 1.0);
        if opacity <= 0.0 {
            self.hide_overlay();
            return;
        }
        if (opacity - self.opacity).abs() < 0.001
            && self.overlay_bounds.as_ref() == Some(&target.bounds)
        {
            return;
        }
        match self.overlay.cover(target.bounds.clone(), opacity) {
            Ok(()) => {
                self.opacity = opacity;
                self.overlay_bounds = Some(target.bounds.clone());
                self.shared
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .overlay_opacity = opacity;
            }
            Err(error) => self.status(&format!("黑幕显示失败：{error}")),
        }
    }

    fn hide_overlay(&mut self) {
        self.overlay_recovery = None;
        self.opacity = 0.0;
        self.overlay_bounds = None;
        let _ = self.overlay.hide();
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .overlay_opacity = 0.0;
    }

    fn begin_overlay_recovery(&mut self, now: Instant) {
        if self.opacity > 0.0 && self.overlay_recovery.is_none() {
            self.overlay_recovery = Some((now, self.opacity));
        }
    }

    fn animate_overlay(&mut self, now: Instant) {
        let Some((started, initial)) = self.overlay_recovery else {
            return;
        };
        let duration = self
            .settings
            .get_effective_profile()
            .blackout_recovery_milliseconds as f64;
        let progress = now.duration_since(started).as_secs_f64() * 1000.0 / duration.max(1.0);
        if progress >= 1.0 {
            self.hide_overlay();
        } else {
            let opacity = initial * (1.0 - progress);
            let target = self
                .shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .monitors
                .iter()
                .find(|m| {
                    m.stable_id
                        .eq_ignore_ascii_case(&self.settings.target_stable_id)
                })
                .cloned();
            if let Some(target) = target {
                self.set_overlay(&target, opacity);
            }
        }
    }
}

fn mode_status(mode: Mode) -> &'static str {
    match mode {
        Mode::Daily => "日常模式已启用",
        Mode::Movie => "观影模式已启用",
        Mode::Custom => "自定义模式已启用",
        Mode::Off => "自动暗屏已关闭",
    }
}

fn desired_brightness(
    mode: Mode,
    inside: bool,
    outside_ms: u64,
    idle_ms: u64,
    profile: &Profile,
) -> Option<BrightnessIntent> {
    if mode == Mode::Off {
        return Some(BrightnessIntent::Restored);
    }
    if mode != Mode::Movie && idle_ms >= profile.idle_blackout_milliseconds as u64 {
        return Some(BrightnessIntent::Dimmed);
    }
    if inside {
        Some(BrightnessIntent::Restored)
    } else if outside_ms >= profile.leave_delay_milliseconds as u64 {
        Some(BrightnessIntent::Dimmed)
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Recovery {
    target: String,
    brightness: u32,
}

fn recovery_paths() -> [PathBuf; 2] {
    [
        config::data_dir().join("brightness-recovery.txt"),
        config::legacy_data_dir().join("brightness-recovery.txt"),
    ]
}

fn load_recovery() -> Result<Option<Recovery>, String> {
    for path in recovery_paths() {
        match fs::read_to_string(&path) {
            Ok(text) => return parse_recovery(&text).map(Some),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("读取亮度恢复记录失败：{e}")),
        }
    }
    Ok(None)
}

fn parse_recovery(text: &str) -> Result<Recovery, String> {
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    let target = lines.next().unwrap_or_default().trim();
    let brightness = lines
        .next()
        .unwrap_or_default()
        .trim()
        .parse::<u32>()
        .map_err(|_| "亮度恢复记录损坏，已保留原文件，请检查数据目录".to_string())?;
    if target.is_empty() {
        return Err("亮度恢复记录没有目标显示器，已保留原文件".into());
    }
    Ok(Recovery {
        target: target.into(),
        brightness,
    })
}

fn save_recovery(record: &Recovery) -> Result<(), String> {
    config::atomic_write(
        &recovery_paths()[0],
        format!("{}\r\n{}\r\n", record.target, record.brightness).as_bytes(),
    )
}

fn clear_recovery(record: &Recovery) -> Result<(), String> {
    for path in recovery_paths() {
        match fs::read_to_string(&path) {
            Ok(text) => {
                // Never remove a record for another monitor or an externally replaced value.
                if parse_recovery(&text).as_ref() == Ok(record) {
                    fs::remove_file(&path).map_err(|e| format!("清理亮度恢复记录失败：{e}"))?;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("读取恢复记录失败：{e}")),
        }
    }
    Ok(())
}

fn brightness_worker(
    requests: Receiver<WorkerCommand>,
    feedback: Sender<Feedback>,
    generation: Arc<AtomicU64>,
) {
    let mut original = None;
    while let Ok(command) = requests.recv() {
        match command {
            WorkerCommand::Change(request) => {
                if generation.load(Ordering::Acquire) != request.generation {
                    continue;
                }
                let outcome = brightness_operation(&request, &mut original, &generation);
                if let Err(error) = &outcome {
                    config::log(&format!("DDC 操作失败：{error}"));
                }
                let _ = feedback.send(Feedback {
                    generation: request.generation,
                    intent: request.intent,
                    outcome,
                });
            }
            WorkerCommand::Shutdown(done) => {
                let result = restore_original(&mut original, 0, 1, None).map(|_| ());
                let _ = done.send(result);
                break;
            }
        }
    }
    if original.is_some() {
        if let Err(error) = restore_original(&mut original, 0, 1, None) {
            config::log(&format!("DDC 线程结束时恢复失败，保留恢复记录：{error}"));
        }
    }
}

fn brightness_operation(
    request: &BrightnessRequest,
    original: &mut Option<Recovery>,
    generation: &AtomicU64,
) -> Result<bool, String> {
    let cancel = Some((generation, request.generation));
    if request.intent == BrightnessIntent::Restored {
        return restore_original(
            original,
            if request.immediate {
                0
            } else {
                request.profile.brightness_recovery_milliseconds
            },
            request.profile.brightness_recovery_frames,
            cancel,
        );
    }
    if original.is_none() {
        *original = load_recovery()?;
    }
    if original
        .as_ref()
        .is_some_and(|r| !r.target.eq_ignore_ascii_case(&request.target))
    {
        // Finish recovery on the previous monitor before capturing another original value.
        restore_original(original, 0, 1, None)?;
    }
    if cancelled(cancel) {
        return Ok(false);
    }
    let handle = BrightnessHandle::open(&request.target)?;
    let brightness = handle.read()?;
    if original.is_none() {
        let record = Recovery {
            target: request.target.clone(),
            brightness: brightness.current,
        };
        save_recovery(&record)?;
        *original = Some(record);
    }
    let duration = if request.profile.brightness_fade_enabled {
        request.profile.brightness_fade_duration_milliseconds
    } else {
        0
    };
    fade_brightness(
        &handle,
        brightness.current,
        brightness.minimum,
        duration,
        request.profile.brightness_fade_frames,
        request.profile.brightness_fade_exponent,
        cancel,
    )
}

fn restore_original(
    original: &mut Option<Recovery>,
    duration_ms: u32,
    frames: u32,
    cancel: Option<(&AtomicU64, u64)>,
) -> Result<bool, String> {
    if original.is_none() {
        *original = load_recovery()?;
    }
    let Some(record) = original.clone() else {
        return Ok(true);
    };
    if cancelled(cancel) {
        return Ok(false);
    }
    let handle = BrightnessHandle::open(&record.target)?;
    let current = handle.read()?;
    let target = record.brightness.clamp(current.minimum, current.maximum);
    if !fade_brightness(
        &handle,
        current.current,
        target,
        duration_ms,
        frames,
        1.0,
        cancel,
    )? {
        return Ok(false);
    }
    clear_recovery(&record)?;
    *original = None;
    config::log(&format!(
        "已恢复 {} 原亮度 {target}，并读回验证。",
        record.target
    ));
    Ok(true)
}

fn cancelled(cancel: Option<(&AtomicU64, u64)>) -> bool {
    cancel.is_some_and(|(generation, expected)| generation.load(Ordering::Acquire) != expected)
}

fn fade_brightness(
    handle: &BrightnessHandle,
    start: u32,
    target: u32,
    duration_ms: u32,
    frames: u32,
    exponent: f64,
    cancel: Option<(&AtomicU64, u64)>,
) -> Result<bool, String> {
    let values = brightness_frames(start, target, duration_ms, frames, exponent);
    let started = Instant::now();
    let frame_count = values.len();
    for (index, value) in values.into_iter().enumerate() {
        let due =
            Duration::from_millis((duration_ms as u64 * (index as u64 + 1)) / frame_count as u64);
        while started.elapsed() < due {
            if cancelled(cancel) {
                return Ok(false);
            }
            thread::sleep(
                due.saturating_sub(started.elapsed())
                    .min(Duration::from_millis(10)),
            );
        }
        if cancelled(cancel) {
            return Ok(false);
        }
        handle.set(value)?;
    }
    if cancelled(cancel) {
        return Ok(false);
    }
    let verified = handle.read()?;
    if verified.current != target {
        return Err(format!(
            "亮度读回不一致：目标 {target}，实际 {}",
            verified.current
        ));
    }
    config::log(&format!(
        "亮度 {start} → {target}，{frame_count} 帧，耗时 {} ms，已读回验证。",
        started.elapsed().as_millis()
    ));
    Ok(true)
}

fn brightness_frames(
    start: u32,
    target: u32,
    duration_ms: u32,
    frames: u32,
    exponent: f64,
) -> Vec<u32> {
    if start == target {
        return Vec::new();
    }
    if duration_ms == 0 {
        return vec![target];
    }
    let count = frames.clamp(1, 100).min(start.abs_diff(target));
    let exponent = exponent.clamp(1.0, 4.0);
    (1..=count)
        .map(|frame| {
            let progress = frame as f64 / count as f64;
            let eased = 1.0 - (1.0 - progress).powf(exponent);
            let raw = (start as f64 + (target as f64 - start as f64) * eased).round() as u32;
            let remaining = count - frame;
            if target < start {
                raw.max(target + remaining)
            } else {
                raw.min(target - remaining)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_curve_keeps_unique_frames_and_exact_endpoint() {
        let values = brightness_frames(100, 0, 300, 30, 3.3);
        assert_eq!(values.len(), 30);
        assert_eq!(values.last(), Some(&0));
        assert!(values.windows(2).all(|v| v[0] > v[1]));
        assert!(values[0] < 97);
        let restore = brightness_frames(0, 100, 500, 30, 1.0);
        assert!(restore.windows(2).all(|v| v[0] < v[1]));
        assert_eq!(restore.last(), Some(&100));
        assert_eq!(brightness_frames(99, 100, 1000, 30, 1.0), vec![100]);
        assert_eq!(brightness_frames(0, 100, 0, 30, 1.0), vec![100]);
    }

    #[test]
    fn mouse_on_main_only_wakes_overlay_not_hardware() {
        let profile = Profile::daily();
        assert_eq!(
            desired_brightness(Mode::Daily, false, 20_000, 0, &profile),
            Some(BrightnessIntent::Dimmed)
        );
        assert_eq!(
            desired_brightness(Mode::Daily, true, 0, 0, &profile),
            Some(BrightnessIntent::Restored)
        );
        assert_eq!(
            desired_brightness(Mode::Daily, true, 0, 30_000, &profile),
            Some(BrightnessIntent::Dimmed)
        );
        assert_eq!(
            desired_brightness(Mode::Movie, true, 0, 30_000, &profile),
            Some(BrightnessIntent::Restored)
        );
        assert_eq!(
            desired_brightness(Mode::Daily, false, 1499, 0, &profile),
            None
        );
    }

    #[test]
    fn generation_changes_cancel_operations_but_repeated_samples_do_not() {
        let generation = Arc::new(AtomicU64::new(0));
        let mut intent = IntentTracker::new(generation.clone());
        assert!(intent.transition(BrightnessIntent::Dimmed, false));
        assert!(intent.transition(BrightnessIntent::Restored, false));
        let restoring_generation = generation.load(Ordering::Acquire);
        for _ in 0..20 {
            assert!(!intent.transition(BrightnessIntent::Restored, false));
        }
        assert!(!cancelled(Some((&generation, restoring_generation))));
        assert!(intent.transition(BrightnessIntent::Dimmed, false));
        assert!(cancelled(Some((&generation, restoring_generation))));
        assert!(intent.transition(BrightnessIntent::Dimmed, true));
    }

    #[test]
    fn recovery_record_retains_identity_and_rejects_malformed_value() {
        assert_eq!(
            parse_recovery("DELA0E7\r\n100\r\n").unwrap(),
            Recovery {
                target: "DELA0E7".into(),
                brightness: 100
            }
        );
        assert!(parse_recovery("DELA0E7\ninvalid").is_err());
        assert!(parse_recovery("\n100").is_err());
    }
}
