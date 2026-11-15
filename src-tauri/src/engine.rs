use crate::{
    brightness_transition::{run_transition, RealClock, TimingPolicy},
    config::{self, AppSettings, Mode, Profile},
    native::{self, BrightnessHandle, Monitor, Overlay, Rect},
    overlay_animation::{opacity_alpha, OverlayAnimator},
};
use serde::{Deserialize, Serialize};
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
const MONITOR_INTERVAL: Duration = Duration::from_secs(30);
const MONITOR_ERROR_INTERVAL: Duration = Duration::from_secs(3);
const MONITOR_EVENT_DEBOUNCE: Duration = Duration::from_millis(250);
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
    pub recovery_requires_confirmation: Option<String>,
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
    ConfirmRecovery(String, Sender<Result<(), String>>),
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
    ConfirmRecovery {
        target: String,
        generation: u64,
        reply: Sender<Result<(), String>>,
    },
}

struct Feedback {
    generation: u64,
    intent: BrightnessIntent,
    outcome: Result<bool, String>,
    recovery_requires_confirmation: Option<String>,
    confirmation_reply: Option<Sender<Result<(), String>>>,
}

impl Engine {
    pub fn new(mut settings: AppSettings) -> Result<Arc<Self>, String> {
        settings.normalize();
        let overlay = Overlay::new()?;
        let monitors = native::enumerate_monitors().unwrap_or_default();
        let recovery = load_recovery();
        let recovery_pending = recovery.as_ref().map(|v| v.is_some()).unwrap_or(true);
        let recovery_requires_confirmation = recovery
            .as_ref()
            .ok()
            .and_then(|record| record.as_ref())
            .and_then(Recovery::confirmation_target);
        let shared = Arc::new(Mutex::new(Snapshot {
            settings: settings.clone(),
            monitors,
            status: "正在启动 Rust 调光核心…".into(),
            brightness_dimmed: recovery_pending,
            overlay_opacity: 0.0,
            recovery_requires_confirmation,
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

    /// Called only after the user explicitly confirms the original physical panel.
    pub fn confirm_recovery_target(&self, target: String) -> Result<(), String> {
        if !native::is_concrete_monitor_id(&target) {
            return Err("请先选择原来的具体屏幕，再确认恢复".into());
        }
        if self.stopped.load(Ordering::Acquire) {
            return Err("调光核心已停止".into());
        }
        let (reply, result) = mpsc::channel();
        self.commands
            .send(Command::ConfirmRecovery(target, reply))
            .map_err(|_| "调光核心未响应".to_string())?;
        result
            .recv_timeout(Duration::from_secs(8))
            .map_err(|_| "确认恢复尚未完成，恢复记录已保留，请稍后检查状态".to_string())?
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
            config::log_error("退出恢复超时；亮度恢复记录仍保留，下次启动继续恢复。");
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
    overlay_animation: OverlayAnimator,
    target_available: bool,
    input_available: bool,
    system_events: Option<Receiver<native::SystemEvent>>,
    suspended: bool,
    confirming_recovery: bool,
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
            overlay_animation: OverlayAnimator::default(),
            target_available: false,
            input_available: true,
            system_events,
            suspended: false,
            confirming_recovery: false,
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
                    native::SystemEvent::OverlayInvalidated => {
                        // The native thread already hid the failed overlay. Drop the
                        // cached frame so the next idle check can recreate it.
                        this.opacity = 0.0;
                        this.overlay_bounds = None;
                        this.overlay_animation.reset();
                        this.shared
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .overlay_opacity = 0.0;
                        this.status_error("黑幕区域更新失败，将自动重试");
                    }
                    native::SystemEvent::DisplayChanged => {
                        this.next_monitor_refresh =
                            display_refresh_deadline(this.next_monitor_refresh, Instant::now());
                    }
                }
            }
            let mut now = Instant::now();
            if now >= this.next_monitor_refresh {
                this.refresh_monitors();
                now = Instant::now();
            }
            if now >= this.next_cursor_check {
                this.next_cursor_check = now + CURSOR_INTERVAL;
                this.check_cursor(now);
            }
            this.animate_overlay(Instant::now());
            let now = Instant::now();
            if !this.operation_pending
                && this.attempts < MAX_ATTEMPTS
                && this.retry_after.is_some_and(|at| now >= at)
            {
                this.queue_brightness();
            }
            let timeout = this.next_wait(Instant::now());
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
                            this.status_error(&error);
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
                Ok(Command::ConfirmRecovery(target, reply)) => {
                    this.hide_overlay();
                    this.reset_activity();
                    this.intent.transition(BrightnessIntent::Restored, true);
                    this.immediate_restore = true;
                    this.operation_pending = true;
                    this.confirming_recovery = true;
                    this.may_need_restore = true;
                    this.attempts = 1;
                    this.retry_after = None;
                    this.status("正在确认原屏幕并恢复记录的亮度…");
                    if let Err(error) = this.worker.send(WorkerCommand::ConfirmRecovery {
                        target,
                        generation: this.generation.load(Ordering::Acquire),
                        reply,
                    }) {
                        this.operation_pending = false;
                        this.confirming_recovery = false;
                        if let WorkerCommand::ConfirmRecovery { reply, .. } = error.0 {
                            let _ = reply.send(Err("DDC 线程已停止".into()));
                        }
                    }
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
            Ok(Err(error)) => config::log_error(&format!("退出恢复失败：{error}")),
            Err(_) => config::log_error("DDC 退出恢复未在 5 秒内完成；保留恢复记录。"),
        }
        if worker_handle.is_finished() {
            let _ = worker_handle.join();
        }
        if let Some(reply) = shutdown_reply {
            let _ = reply.send(());
        }
    }

    fn status(&self, message: &str) {
        publish_status(&self.shared, message, config::log);
    }

    fn status_error(&self, message: &str) {
        publish_status(&self.shared, message, config::log_error);
    }

    fn reset_activity(&mut self) {
        self.last_movement = Instant::now();
        self.outside_since = None;
        self.previous_cursor = None;
    }

    fn refresh_monitors(&mut self) {
        let next_delay = match native::enumerate_monitors() {
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
                    self.status(
                        if native::is_concrete_monitor_id(&self.settings.target_stable_id) {
                            "未找到已选择的屏幕，已暂停自动暗屏"
                        } else {
                            "旧版屏幕选择需在设置中重新选择具体屏幕，已暂停自动暗屏"
                        },
                    );
                }
                MONITOR_INTERVAL
            }
            Err(error) => {
                self.status_error(&format!("刷新显示器失败：{error}"));
                MONITOR_ERROR_INTERVAL
            }
        };
        self.next_monitor_refresh = Instant::now() + next_delay;
    }

    fn check_cursor(&mut self, now: Instant) {
        if !cursor_sampling_allowed(self.settings.mode, self.suspended, self.confirming_recovery) {
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
                    self.status_error(&format!("输入桌面不可用，已暂停暗屏：{error}"));
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
                self.overlay_animation.set_target(1.0, Duration::ZERO, now);
            }
        } else if idle_ms >= profile.idle_blackout_milliseconds as u64 {
            let duration = if profile.blackout_fade_enabled {
                Duration::from_secs(5)
            } else {
                Duration::ZERO
            };
            let threshold = self.last_movement
                + Duration::from_millis(profile.idle_blackout_milliseconds as u64);
            self.overlay_animation.set_target(1.0, duration, threshold);
        }
    }

    fn transition(&mut self, intent: BrightnessIntent, immediate: bool, force: bool) {
        // Repeated cursor samples are not a new operation and must not cancel restoration.
        if !self.intent.transition(intent, force) {
            return;
        }
        self.immediate_restore = immediate;
        self.confirming_recovery = false;
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
            self.status_error("DDC 线程已停止，请重新启动 fishcreen");
        }
    }

    fn receive_feedback(&mut self, feedback: Feedback) {
        if feedback.generation != self.generation.load(Ordering::Acquire) {
            if let Some(reply) = feedback.confirmation_reply {
                let _ = reply.send(Err(
                    "确认恢复被较新的操作取消，请检查当前状态；未完成时记录仍保留".into(),
                ));
            }
            return;
        }
        self.operation_pending = false;
        self.confirming_recovery = false;
        let confirmation_result = match &feedback.outcome {
            Ok(true) => Ok(()),
            Ok(false) => Err("确认恢复已取消，未完成的恢复记录仍保留".into()),
            Err(error) => Err(error.clone()),
        };
        let requires_confirmation = feedback.recovery_requires_confirmation.is_some();
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recovery_requires_confirmation = feedback.recovery_requires_confirmation;
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
                if requires_confirmation {
                    self.attempts = MAX_ATTEMPTS;
                    self.retry_after = None;
                    self.status_error(&format!("{error}；请在设置中选择原屏幕并确认恢复"));
                } else if self.attempts < MAX_ATTEMPTS {
                    self.retry_after = Some(Instant::now() + RETRY_INTERVAL);
                    self.status_error(&format!(
                        "{error}；2 秒后重试（{}/{MAX_ATTEMPTS}）",
                        self.attempts
                    ));
                } else {
                    self.status_error(&format!("{error}；已停止重试，可点击立即恢复"));
                }
            }
        }
        if let Some(reply) = feedback.confirmation_reply {
            let _ = reply.send(confirmation_result);
        }
    }

    fn set_overlay(&mut self, target: &Monitor, opacity: f64) {
        let alpha = opacity_alpha(opacity);
        let opacity = f64::from(alpha) / 255.0;
        if alpha == 0 {
            if self.overlay_bounds.is_some() || self.opacity > 0.0 {
                let _ = self.overlay.hide();
            }
            self.opacity = 0.0;
            self.overlay_bounds = None;
            self.shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .overlay_opacity = 0.0;
            return;
        }
        if alpha == opacity_alpha(self.opacity)
            && self.overlay_bounds.as_ref() == Some(&target.bounds)
        {
            return;
        }
        match self.overlay.cover(
            target.bounds.clone(),
            opacity,
            self.settings.show_active_window,
        ) {
            Ok(()) => {
                self.opacity = opacity;
                self.overlay_bounds = Some(target.bounds.clone());
                self.shared
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .overlay_opacity = opacity;
            }
            Err(error) => {
                // Clear the animation intent as well as its cached frame, so a
                // failed final frame is retried by the next policy sample.
                self.hide_overlay();
                self.status_error(&format!("黑幕显示失败：{error}"));
            }
        }
    }

    fn hide_overlay(&mut self) {
        self.overlay_animation.reset();
        self.opacity = 0.0;
        self.overlay_bounds = None;
        let _ = self.overlay.hide();
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .overlay_opacity = 0.0;
    }

    fn begin_overlay_recovery(&mut self, now: Instant) {
        let duration = Duration::from_millis(
            self.settings
                .get_effective_profile()
                .blackout_recovery_milliseconds as u64,
        );
        self.overlay_animation.set_target(0.0, duration, now);
    }

    fn animate_overlay(&mut self, now: Instant) {
        let frame = self.overlay_animation.tick(now);
        if frame.is_none() && self.opacity <= 0.0 {
            return;
        }
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
            if let Some(opacity) = overlay_frame_for_target(
                frame,
                self.opacity,
                self.overlay_bounds.as_ref(),
                &target.bounds,
            ) {
                self.set_overlay(&target, opacity);
            }
        } else {
            self.hide_overlay();
        }
    }

    fn next_wait(&self, now: Instant) -> Duration {
        let retry = if !self.operation_pending && self.attempts < MAX_ATTEMPTS {
            self.retry_after
        } else {
            None
        };
        controller_wait(
            now,
            self.next_cursor_check,
            self.next_monitor_refresh,
            self.overlay_animation.next_deadline(),
            retry,
        )
    }
}

fn publish_status(shared: &Mutex<Snapshot>, message: &str, log: impl FnOnce(&str)) {
    let changed = {
        let mut snapshot = shared.lock().unwrap_or_else(|error| error.into_inner());
        if snapshot.status == message {
            false
        } else {
            snapshot.status = message.into();
            true
        }
    };
    // File I/O must never hold the snapshot lock needed by the UI and tray.
    if changed {
        log(message);
    }
}

fn display_refresh_deadline(current: Instant, now: Instant) -> Instant {
    current.min(now + MONITOR_EVENT_DEBOUNCE)
}

fn overlay_frame_for_target(
    animation_frame: Option<f64>,
    current_opacity: f64,
    current_bounds: Option<&Rect>,
    target_bounds: &Rect,
) -> Option<f64> {
    animation_frame.or_else(|| {
        (current_opacity > 0.0 && current_bounds != Some(target_bounds)).then_some(current_opacity)
    })
}

fn controller_wait(
    now: Instant,
    cursor: Instant,
    monitor: Instant,
    animation: Option<Instant>,
    retry: Option<Instant>,
) -> Duration {
    let deadline = animation
        .into_iter()
        .chain(retry)
        .fold(cursor.min(monitor), Instant::min);
    deadline.saturating_duration_since(now)
}

fn mode_status(mode: Mode) -> &'static str {
    match mode {
        Mode::Daily => "日常模式已启用",
        Mode::Movie => "观影模式已启用",
        Mode::Custom => "自定义模式已启用",
        Mode::Off => "自动暗屏已关闭",
    }
}

fn cursor_sampling_allowed(mode: Mode, suspended: bool, confirming_recovery: bool) -> bool {
    mode != Mode::Off && !suspended && !confirming_recovery
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrightnessRange {
    minimum: u32,
    maximum: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyRecovery {
    target: String,
    brightness: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Recovery {
    version: u32,
    target: String,
    brightness: u32,
    range: Option<BrightnessRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    legacy_source: Option<LegacyRecovery>,
}

impl Recovery {
    fn confirmation_target(&self) -> Option<String> {
        (self.version == 1 && !native::is_concrete_monitor_id(&self.target))
            .then(|| self.target.clone())
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != 2 || !native::is_concrete_monitor_id(&self.target) {
            return Err("亮度恢复记录版本或设备身份无效，已保留原文件".into());
        }
        let Some(range) = &self.range else {
            return Err("亮度恢复记录缺少原量程，已保留原文件".into());
        };
        if range.minimum > range.maximum
            || self.brightness < range.minimum
            || self.brightness > range.maximum
        {
            return Err("亮度恢复记录量程无效，已保留原文件".into());
        }
        if let Some(legacy) = &self.legacy_source {
            if legacy.brightness != self.brightness
                || !native::monitor_model_id(&self.target)
                    .is_some_and(|model| model.eq_ignore_ascii_case(&legacy.target))
            {
                return Err("恢复记录的旧型号绑定无效，已保留原文件".into());
            }
        }
        Ok(())
    }

    fn validate_range(&self, current: &native::Brightness) -> Result<(), String> {
        if current.minimum > current.maximum {
            return Err("显示器返回了无效的亮度量程，未恢复亮度".into());
        }
        if let Some(range) = &self.range {
            if range.minimum != current.minimum || range.maximum != current.maximum {
                return Err("显示器亮度量程与原记录不同，已保留恢复记录，未写入亮度".into());
            }
        }
        if !(current.minimum..=current.maximum).contains(&self.brightness) {
            return Err("原亮度不在当前显示器量程内，已保留恢复记录，未写入亮度".into());
        }
        Ok(())
    }
}

fn recovery_paths() -> [PathBuf; 3] {
    [
        config::data_dir().join("brightness-recovery-v2.json"),
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
    let text = text.trim_start_matches('\u{feff}').trim();
    if text.starts_with('{') {
        let record: Recovery = serde_json::from_str(text)
            .map_err(|_| "亮度恢复记录损坏，已保留原文件，请检查数据目录".to_string())?;
        record.validate()?;
        return Ok(record);
    }
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    let target = lines.next().unwrap_or_default().trim();
    let brightness = lines
        .next()
        .unwrap_or_default()
        .trim()
        .parse::<u32>()
        .map_err(|_| "亮度恢复记录损坏，已保留原文件，请检查数据目录".to_string())?;
    if target.is_empty() || lines.any(|line| !line.trim().is_empty()) {
        return Err("亮度恢复记录没有目标显示器，已保留原文件".into());
    }
    Ok(Recovery {
        version: 1,
        target: target.into(),
        brightness,
        range: None,
        legacy_source: None,
    })
}

fn save_recovery(record: &Recovery) -> Result<(), String> {
    save_recovery_at(&recovery_paths()[0], record)
}

fn save_recovery_at(path: &std::path::Path, record: &Recovery) -> Result<(), String> {
    record.validate()?;
    match fs::read_to_string(path) {
        Ok(text) if parse_recovery(&text).as_ref() == Ok(record) => return Ok(()),
        Ok(_) => return Err("存在不同的未完成恢复记录，已保留原文件，拒绝覆盖".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("读取已有恢复记录失败：{error}")),
    }
    let bytes = serde_json::to_vec_pretty(record).map_err(|error| error.to_string())?;
    config::atomic_write(path, &bytes)
}

fn backup_legacy_recovery_at(paths: &[PathBuf], record: &Recovery) -> Result<Vec<PathBuf>, String> {
    let mut backups = Vec::new();
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%f");
    for path in paths {
        match fs::read(path) {
            Ok(bytes) => {
                let matches = std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|text| parse_recovery(text).ok())
                    .is_some_and(|candidate| candidate == *record);
                if matches {
                    let backup = path.with_extension(format!("txt.confirmed-{stamp}.bak"));
                    if backup.exists() {
                        return Err("旧恢复记录备份已存在，请稍后重试；原记录未删除".into());
                    }
                    config::atomic_write(&backup, &bytes)?;
                    if fs::read(&backup).map_err(|error| error.to_string())? != bytes {
                        return Err("旧恢复记录备份核对失败，原记录未删除".into());
                    }
                    backups.push(backup);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("备份旧恢复记录失败：{error}")),
        }
    }
    if backups.is_empty() {
        return Err("旧恢复记录原件已变化或不存在，请重新启动并核对；未执行恢复".into());
    }
    Ok(backups)
}

fn may_clear_recovery(candidate: &Recovery, restored: &Recovery) -> bool {
    if candidate == restored {
        return true;
    }
    candidate.version == 1
        && restored.legacy_source.as_ref().is_some_and(|legacy| {
            candidate.target == legacy.target && candidate.brightness == legacy.brightness
        })
}

fn clear_recovery(record: &Recovery) -> Result<(), String> {
    clear_recovery_at(&recovery_paths(), record)
}

fn clear_recovery_at(paths: &[PathBuf], record: &Recovery) -> Result<(), String> {
    // Keep the authoritative v2 record until any matching legacy original has
    // been cleared. A failed cleanup can then safely resume using exact identity.
    for path in paths.iter().rev() {
        match fs::read_to_string(&path) {
            Ok(text) => {
                // Never remove a record for another monitor or an externally replaced value.
                if parse_recovery(&text)
                    .as_ref()
                    .is_ok_and(|candidate| may_clear_recovery(candidate, record))
                {
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
                    config::log_error(&format!("DDC 操作失败：{error}"));
                }
                let _ = feedback.send(Feedback {
                    generation: request.generation,
                    intent: request.intent,
                    outcome,
                    recovery_requires_confirmation: original
                        .as_ref()
                        .and_then(Recovery::confirmation_target),
                    confirmation_reply: None,
                });
            }
            WorkerCommand::ConfirmRecovery {
                target,
                generation: expected,
                reply,
            } => {
                let outcome = if cancelled(Some((&generation, expected))) {
                    Err("确认恢复请求已被较新的操作取消，恢复记录未删除".into())
                } else {
                    confirm_legacy_recovery(&target, &mut original, (&generation, expected))
                };
                let _ = feedback.send(Feedback {
                    generation: expected,
                    intent: BrightnessIntent::Restored,
                    outcome,
                    recovery_requires_confirmation: original
                        .as_ref()
                        .and_then(Recovery::confirmation_target),
                    confirmation_reply: Some(reply),
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
            config::log_error(&format!("DDC 线程结束时恢复失败，保留恢复记录：{error}"));
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
            version: 2,
            target: handle.target_id().into(),
            brightness: brightness.current,
            range: Some(BrightnessRange {
                minimum: brightness.minimum,
                maximum: brightness.maximum,
            }),
            legacy_source: None,
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
        TimingPolicy::PreserveSteps,
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
    if record.confirmation_target().is_some() {
        return Err("旧亮度恢复记录只有型号，无法确认原物理屏幕，已保留记录且未写入亮度".into());
    }
    let handle = BrightnessHandle::open(&record.target)?;
    let current = handle.read()?;
    record.validate_range(&current)?;
    let target = record.brightness;
    let outcome = fade_brightness(
        &handle,
        current.current,
        target,
        duration_ms,
        frames,
        1.0,
        TimingPolicy::SkipExpired,
        cancel,
    );
    if !complete_recovery(original, &record, outcome, clear_recovery)? {
        return Ok(false);
    }
    config::log(&format!(
        "已恢复 {} 原亮度 {target}，并读回验证。",
        record.target
    ));
    Ok(true)
}

fn complete_recovery(
    original: &mut Option<Recovery>,
    record: &Recovery,
    verified: Result<bool, String>,
    clear: impl FnOnce(&Recovery) -> Result<(), String>,
) -> Result<bool, String> {
    if !verified? {
        return Ok(false);
    }
    clear(record)?;
    *original = None;
    Ok(true)
}

fn bind_legacy_recovery(
    record: &Recovery,
    target: &str,
    brightness: &native::Brightness,
) -> Result<Recovery, String> {
    if record.confirmation_target().is_none() {
        return Err("当前恢复记录已有具体设备身份，无需重新绑定".into());
    }
    if !native::monitor_model_id(target)
        .is_some_and(|model| model.eq_ignore_ascii_case(&record.target))
    {
        return Err("所选屏幕型号与旧恢复记录不一致，未绑定且未写入亮度".into());
    }
    record.validate_range(brightness)?;
    Ok(Recovery {
        version: 2,
        target: target.into(),
        brightness: record.brightness,
        range: Some(BrightnessRange {
            minimum: brightness.minimum,
            maximum: brightness.maximum,
        }),
        legacy_source: Some(LegacyRecovery {
            target: record.target.clone(),
            brightness: record.brightness,
        }),
    })
}

fn confirm_legacy_recovery(
    target: &str,
    original: &mut Option<Recovery>,
    cancel: (&AtomicU64, u64),
) -> Result<bool, String> {
    if original.is_none() {
        *original = load_recovery()?;
    }
    let record = original
        .as_ref()
        .ok_or_else(|| "没有待确认的旧亮度恢复记录".to_string())?;
    // No hardware call is made before establishing that this is a legacy
    // record requiring the explicit user confirmation represented by this API.
    if record.confirmation_target().is_none() {
        return Err("当前恢复记录已有具体设备身份，无需重新绑定".into());
    }
    let handle = BrightnessHandle::open(target)?;
    if !handle.model_id().eq_ignore_ascii_case(&record.target) {
        return Err("所选屏幕型号与旧恢复记录不一致，未绑定且未写入亮度".into());
    }
    let brightness = handle.read()?;
    record.validate_range(&brightness)?;
    if cancelled(Some(cancel)) {
        return Err("确认恢复请求已取消，旧恢复记录保持不变".into());
    }
    let rebound = bind_legacy_recovery(record, handle.target_id(), &brightness)?;
    // The old text file remains intact until the newly bound operation has
    // finished and read back successfully. Failed writes do not alter `original`.
    backup_legacy_recovery_at(&recovery_paths()[1..], record)?;
    save_recovery(&rebound)?;
    *original = Some(rebound);
    restore_original(original, 0, 1, Some(cancel))
}

fn cancelled(cancel: Option<(&AtomicU64, u64)>) -> bool {
    cancel.is_some_and(|(generation, expected)| generation.load(Ordering::Acquire) != expected)
}

#[allow(clippy::too_many_arguments)] // Timing is selected by intent, not brightness direction.
fn fade_brightness(
    handle: &BrightnessHandle,
    start: u32,
    target: u32,
    duration_ms: u32,
    frames: u32,
    exponent: f64,
    timing: TimingPolicy,
    cancel: Option<(&AtomicU64, u64)>,
) -> Result<bool, String> {
    let values = brightness_frames(start, target, duration_ms, frames, exponent);
    let frame_count = values.len();
    let report = run_transition(
        &RealClock::new(),
        &values,
        start,
        target,
        Duration::from_millis(duration_ms as u64),
        timing,
        || cancelled(cancel),
        |value| handle.set(value),
        || handle.read().map(|brightness| brightness.current),
    )?;
    if !report.completed {
        return Ok(false);
    }
    config::log(&format!(
        "亮度 {start} → {target}，计划 {frame_count} 帧，实际写入 {} 次，跳过 {} 个样本，耗时 {} ms，已读回验证。",
        report.writes,
        report.skipped,
        report.elapsed.as_millis()
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

    const PANEL_A: &str = r"\\?\DISPLAY#DELA0E7#INSTANCE_A#{GUID}";
    const PANEL_B: &str = r"\\?\DISPLAY#DELA0E7#INSTANCE_B#{GUID}";

    #[test]
    fn status_publishes_before_logging_without_holding_the_snapshot_mutex() {
        let shared = Mutex::new(Snapshot {
            settings: AppSettings::default(),
            monitors: Vec::new(),
            status: "before".into(),
            brightness_dimmed: false,
            overlay_opacity: 0.0,
            recovery_requires_confirmation: None,
        });
        let mut logged = false;
        publish_status(&shared, "after", |message| {
            let snapshot = shared
                .try_lock()
                .expect("logging must not hold the state mutex");
            assert_eq!(snapshot.status, message);
            logged = true;
        });
        assert!(logged);
        publish_status(&shared, "after", |_| {
            panic!("unchanged status must not log again")
        });
    }

    #[test]
    fn display_notifications_coalesce_without_postponing_the_first_refresh() {
        let now = Instant::now();
        let fallback = now + MONITOR_INTERVAL;
        let first = display_refresh_deadline(fallback, now);
        assert_eq!(first, now + Duration::from_millis(250));
        assert_eq!(
            display_refresh_deadline(first, now + Duration::from_millis(100)),
            first
        );
        let sooner = now + Duration::from_millis(20);
        assert_eq!(display_refresh_deadline(sooner, now), sooner);
        assert_eq!(MONITOR_INTERVAL, Duration::from_secs(30));
    }

    #[test]
    fn stable_overlay_relayouts_after_a_monitor_change_without_restarting_animation() {
        let now = Instant::now();
        let mut animator = OverlayAnimator::default();
        animator.set_target(1.0, Duration::ZERO, now);
        assert_eq!(animator.tick(now), Some(1.0));
        assert!(animator.next_deadline().is_none());
        let original = Rect {
            left: -1440,
            top: 0,
            right: 0,
            bottom: 2560,
        };
        let resized = Rect {
            left: -1080,
            top: 0,
            right: 0,
            bottom: 1920,
        };
        assert_eq!(
            overlay_frame_for_target(None, 1.0, Some(&original), &resized),
            Some(1.0)
        );
        assert_eq!(
            overlay_frame_for_target(None, 1.0, Some(&original), &original),
            None
        );
        assert_eq!(
            overlay_frame_for_target(Some(0.5), 1.0, Some(&original), &resized),
            Some(0.5)
        );
        assert_eq!(overlay_frame_for_target(None, 0.0, None, &resized), None);
        assert!(animator.next_deadline().is_none());
    }

    #[test]
    fn controller_waits_for_nearest_deadline_without_idle_animation_wakes() {
        let now = Instant::now();
        let cursor = now + Duration::from_millis(75);
        let monitor = now + Duration::from_secs(3);
        let frame = now + Duration::from_millis(16);
        let retry = now + Duration::from_millis(5);
        assert_eq!(
            controller_wait(now, cursor, monitor, None, None),
            Duration::from_millis(75)
        );
        assert_eq!(
            controller_wait(now, cursor, monitor, Some(frame), None),
            Duration::from_millis(16)
        );
        assert_eq!(
            controller_wait(now, cursor, monitor, Some(frame), Some(retry)),
            Duration::from_millis(5)
        );
        assert_eq!(
            controller_wait(now, cursor, now + Duration::from_millis(3), None, None),
            Duration::from_millis(3)
        );
        assert_eq!(
            controller_wait(now, cursor, monitor, Some(now), None),
            Duration::ZERO
        );
    }

    #[test]
    fn cursor_policy_pauses_during_explicit_recovery_confirmation() {
        for mode in [Mode::Daily, Mode::Movie, Mode::Custom] {
            assert!(cursor_sampling_allowed(mode, false, false));
            assert!(!cursor_sampling_allowed(mode, false, true));
            assert!(!cursor_sampling_allowed(mode, true, false));
        }
        assert!(!cursor_sampling_allowed(Mode::Off, false, false));
    }

    fn recovery_v2() -> Recovery {
        Recovery {
            version: 2,
            target: PANEL_A.into(),
            brightness: 75,
            range: Some(BrightnessRange {
                minimum: 0,
                maximum: 100,
            }),
            legacy_source: None,
        }
    }

    struct RecoveryTestDirectory(PathBuf);

    impl RecoveryTestDirectory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "fishcreen-recovery-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for RecoveryTestDirectory {
        fn drop(&mut self) {
            // Only the unique directory created by this test is ever removed.
            let _ = fs::remove_dir_all(&self.0);
        }
    }

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
                version: 1,
                target: "DELA0E7".into(),
                brightness: 100,
                range: None,
                legacy_source: None,
            }
        );
        assert!(parse_recovery("DELA0E7\ninvalid").is_err());
        assert!(parse_recovery("\n100").is_err());
    }

    #[test]
    fn legacy_model_recovery_requires_explicit_binding_but_interface_does_not() {
        let legacy = parse_recovery("DELA0E7\n75").unwrap();
        assert_eq!(legacy.confirmation_target(), Some("DELA0E7".into()));
        let concrete = parse_recovery(&format!("{PANEL_A}\n75")).unwrap();
        assert_eq!(concrete.confirmation_target(), None);
        assert_eq!(recovery_v2().confirmation_target(), None);
    }

    #[test]
    fn explicit_binding_checks_model_and_preserves_legacy_provenance() {
        let legacy = parse_recovery("DELA0E7\n75").unwrap();
        let brightness = native::Brightness {
            minimum: 0,
            current: 0,
            maximum: 100,
        };
        let rebound = bind_legacy_recovery(&legacy, PANEL_A, &brightness).unwrap();
        assert_eq!(rebound.target, PANEL_A);
        assert_eq!(rebound.brightness, 75);
        assert!(may_clear_recovery(&legacy, &rebound));
        assert!(!may_clear_recovery(
            &parse_recovery("DELA0E7\n76").unwrap(),
            &rebound
        ));
        assert!(bind_legacy_recovery(&legacy, "DELA0E7", &brightness).is_err());
        assert!(
            bind_legacy_recovery(&legacy, r"\\?\DISPLAY#OTHER#INSTANCE#{GUID}", &brightness)
                .is_err()
        );
        assert!(bind_legacy_recovery(&rebound, PANEL_B, &brightness).is_err());
    }

    #[test]
    fn versioned_recovery_roundtrips_and_rejects_invalid_identity_or_range() {
        let record = recovery_v2();
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(parse_recovery(&json), Ok(record.clone()));
        let mut invalid = record.clone();
        invalid.version = 3;
        assert!(parse_recovery(&serde_json::to_string(&invalid).unwrap()).is_err());
        invalid = record.clone();
        invalid.target = "DELA0E7".into();
        assert!(invalid.validate().is_err());
        invalid = record.clone();
        invalid.range = None;
        assert!(invalid.validate().is_err());
        invalid = record;
        invalid.range = Some(BrightnessRange {
            minimum: 100,
            maximum: 0,
        });
        assert!(invalid.validate().is_err());
        assert!(parse_recovery("{\"version\":2}").is_err());
        assert!(parse_recovery("DELA0E7\n75\nextra").is_err());
    }

    #[test]
    fn restore_rejects_changed_range_instead_of_clamping_to_another_scale() {
        let record = recovery_v2();
        assert!(record
            .validate_range(&native::Brightness {
                minimum: 0,
                current: 0,
                maximum: 100
            })
            .is_ok());
        assert!(record
            .validate_range(&native::Brightness {
                minimum: 0,
                current: 0,
                maximum: 255
            })
            .is_err());
        assert!(record
            .validate_range(&native::Brightness {
                minimum: 80,
                current: 80,
                maximum: 100
            })
            .is_err());
        assert!(record
            .validate_range(&native::Brightness {
                minimum: 100,
                current: 100,
                maximum: 0
            })
            .is_err());
    }

    #[test]
    fn failed_or_cancelled_restore_never_clears_original_record() {
        let record = recovery_v2();
        let mut original = Some(record.clone());
        for outcome in [Err("DDC failed".into()), Ok(false)] {
            let mut cleared = false;
            let result = complete_recovery(&mut original, &record, outcome, |_| {
                cleared = true;
                Ok(())
            });
            assert!(!cleared);
            assert_eq!(original, Some(record.clone()));
            assert!(result.is_err() || result == Ok(false));
        }
        assert!(complete_recovery(&mut original, &record, Ok(true), |_| Err(
            "cleanup failed".into()
        ))
        .is_err());
        assert_eq!(original, Some(record.clone()));
        assert_eq!(
            complete_recovery(&mut original, &record, Ok(true), |_| Ok(())),
            Ok(true)
        );
        assert!(original.is_none());
    }

    #[test]
    fn migration_keeps_legacy_file_until_verified_cleanup_and_preserves_replacements() {
        let directory = RecoveryTestDirectory::new();
        let new_path = directory.0.join("brightness-recovery-v2.json");
        let old_path = directory.0.join("brightness-recovery.txt");
        let legacy_text = "DELA0E7\r\n75\r\n";
        fs::write(&old_path, legacy_text).unwrap();
        let legacy = parse_recovery(legacy_text).unwrap();
        let rebound = bind_legacy_recovery(
            &legacy,
            PANEL_A,
            &native::Brightness {
                minimum: 0,
                current: 0,
                maximum: 100,
            },
        )
        .unwrap();
        save_recovery_at(&new_path, &rebound).unwrap();
        assert_eq!(fs::read_to_string(&old_path).unwrap(), legacy_text);
        assert_eq!(
            parse_recovery(&fs::read_to_string(&new_path).unwrap()),
            Ok(rebound.clone())
        );
        fs::write(&old_path, "DELA0E7\n76").unwrap();
        clear_recovery_at(&[new_path.clone(), old_path.clone()], &rebound).unwrap();
        assert!(old_path.exists());
        assert!(!new_path.exists());
        fs::write(&old_path, legacy_text).unwrap();
        save_recovery_at(&new_path, &rebound).unwrap();
        clear_recovery_at(&[new_path.clone(), old_path.clone()], &rebound).unwrap();
        assert!(!old_path.exists());
        assert!(!new_path.exists());
    }

    #[test]
    fn recovery_write_failure_does_not_overwrite_existing_evidence() {
        let directory = RecoveryTestDirectory::new();
        let path = directory.0.join("record.json");
        let record = recovery_v2();
        fs::write(&path, "unparseable original").unwrap();
        assert!(save_recovery_at(&path, &record).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "unparseable original");
        let blocked_parent = directory.0.join("not-a-directory");
        fs::write(&blocked_parent, "keep me").unwrap();
        assert!(save_recovery_at(&blocked_parent.join("record.json"), &record).is_err());
        assert_eq!(fs::read_to_string(&blocked_parent).unwrap(), "keep me");
    }

    #[test]
    fn legacy_confirmation_backup_survives_successful_cleanup() {
        let directory = RecoveryTestDirectory::new();
        let old_path = directory.0.join("brightness-recovery.txt");
        let new_path = directory.0.join("brightness-recovery-v2.json");
        let original_bytes = b"DELA0E7\r\n75\r\n";
        fs::write(&old_path, original_bytes).unwrap();
        let legacy = parse_recovery(std::str::from_utf8(original_bytes).unwrap()).unwrap();
        let backups = backup_legacy_recovery_at(std::slice::from_ref(&old_path), &legacy).unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), original_bytes);
        assert_eq!(fs::read(&old_path).unwrap(), original_bytes);
        let rebound = bind_legacy_recovery(
            &legacy,
            PANEL_A,
            &native::Brightness {
                minimum: 0,
                current: 0,
                maximum: 100,
            },
        )
        .unwrap();
        save_recovery_at(&new_path, &rebound).unwrap();
        clear_recovery_at(&[new_path, old_path.clone()], &rebound).unwrap();
        assert!(!old_path.exists());
        assert_eq!(fs::read(&backups[0]).unwrap(), original_bytes);
        assert!(backup_legacy_recovery_at(&[old_path], &legacy).is_err());
    }
}
