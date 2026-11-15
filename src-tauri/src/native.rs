//! Windows-only monitor access and a WebView-free, click-through black overlay.
//!
//! Physical monitor handles never escape the DDC worker that opened them. Window
//! messages and layered-window updates belong to a separate message-pump thread.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::c_void,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
    sync::{mpsc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};
use windows_sys::Win32::{
    Devices::Display::{
        DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
        GetPhysicalMonitorsFromHMONITOR, SetMonitorBrightness, PHYSICAL_MONITOR,
    },
    Foundation::{
        CloseHandle, GetLastError, SetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND,
        ERROR_SUCCESS, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    },
    Graphics::{
        Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS},
        Gdi::{
            CombineRgn, CreateRectRgn, DeleteObject, EnumDisplayDevicesW, EnumDisplayMonitors,
            GetMonitorInfoW, GetStockObject, SetWindowRgn, BLACK_BRUSH, DISPLAY_DEVICEW, HDC,
            HMONITOR, HRGN, MONITORINFO, MONITORINFOEXW, RGN_DIFF,
        },
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW,
            RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
            REG_SZ, RRF_RT_REG_SZ,
        },
        Threading::{CreateMutexW, GetCurrentThreadId},
    },
    UI::{
        HiDpi::{
            SetProcessDpiAwarenessContext, SetThreadDpiAwarenessContext,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetAncestor,
            GetClassNameW, GetCursorPos, GetDesktopWindow, GetForegroundWindow, GetMessageW,
            GetShellWindow, GetWindowLongPtrW, IsIconic, IsWindow, IsWindowVisible, KillTimer,
            PostQuitMessage, PostThreadMessageW, RegisterClassW, SetLayeredWindowAttributes,
            SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, CREATESTRUCTW,
            EDD_GET_DEVICE_INTERFACE_NAME, GA_ROOT, GWLP_USERDATA, HTTRANSPARENT, HWND_TOPMOST,
            LWA_ALPHA, MA_NOACTIVATE, MONITORINFOF_PRIMARY, MSG, PBT_APMRESUMEAUTOMATIC,
            PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WM_APP,
            WM_CLOSE, WM_DESTROY, WM_ENDSESSION, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCHITTEST,
            WM_POWERBROADCAST, WM_QUERYENDSESSION, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "fishcreen";
const OVERLAY_MESSAGE: u32 = WM_APP + 17;
const FOREGROUND_TIMER: usize = 1;
const FOREGROUND_POLL_MILLISECONDS: u32 = 33;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn from_wide(text: &[u16]) -> String {
    let length = text
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(text.len());
    String::from_utf16_lossy(&text[..length])
}

fn win_error(context: &str) -> String {
    format!("{context}：{}", std::io::Error::last_os_error())
}

fn registry_error(context: &str, code: u32) -> String {
    format!(
        "{context}：{}",
        std::io::Error::from_raw_os_error(code as i32)
    )
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub stable_id: String,
    pub name: String,
    pub gdi_name: String,
    pub bounds: Rect,
    pub primary: bool,
}

#[derive(Clone)]
struct ActiveMonitor {
    monitor: Monitor,
    handle: HMONITOR,
    interface_id: String,
    model_id: String,
}

/// Windows device-interface identity, not the model token shared by identical panels.
pub fn is_concrete_monitor_id(value: &str) -> bool {
    let mut parts = value.split('#');
    parts
        .next()
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(r"\\?\DISPLAY"))
        && parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_some_and(|part| !part.is_empty())
        && parts.next().is_some_and(|part| !part.is_empty())
}

pub fn monitor_model_id(value: &str) -> Option<&str> {
    is_concrete_monitor_id(value)
        .then(|| value.split('#').nth(1))
        .flatten()
}

unsafe extern "system" fn collect_monitor(
    handle: HMONITOR,
    _dc: HDC,
    _rectangle: *mut RECT,
    data: LPARAM,
) -> i32 {
    let monitors = unsafe { &mut *(data as *mut Vec<ActiveMonitor>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(handle, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) } == 0
    {
        return 1;
    }

    let mut device = DISPLAY_DEVICEW {
        cb: size_of::<DISPLAY_DEVICEW>() as u32,
        ..Default::default()
    };
    if unsafe {
        EnumDisplayDevicesW(
            info.szDevice.as_ptr(),
            0,
            &mut device,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
    } == 0
    {
        return 1;
    }

    // Active virtual desktops have no physical panel to dim. Keep them out of
    // the picker, and never substitute an arbitrary monitor for a missing ID.
    let mut physical_count = 0;
    if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(handle, &mut physical_count) } == 0
        || physical_count == 0
    {
        return 1;
    }

    let interface_id = from_wide(&device.DeviceID);
    if !is_concrete_monitor_id(&interface_id) {
        // A model or GDI label is not enough evidence to identify a physical panel.
        return 1;
    }
    let gdi_name = from_wide(&info.szDevice);
    let model_id = interface_id
        .split('#')
        .nth(1)
        .filter(|value| !value.is_empty())
        .unwrap_or(&gdi_name)
        .to_owned();
    let description = from_wide(&device.DeviceString);
    let name = match model_id.as_str() {
        "DELA0E7" => "Dell S2417DG".to_owned(),
        _ if description.is_empty() => model_id.clone(),
        _ => format!("{description} ({model_id})"),
    };
    let bounds = info.monitorInfo.rcMonitor;
    monitors.push(ActiveMonitor {
        monitor: Monitor {
            stable_id: interface_id.clone(),
            name,
            gdi_name,
            bounds: Rect {
                left: bounds.left,
                top: bounds.top,
                right: bounds.right,
                bottom: bounds.bottom,
            },
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        },
        handle,
        interface_id,
        model_id,
    });
    1
}

fn active_monitors() -> Result<Vec<ActiveMonitor>, String> {
    let mut monitors: Vec<ActiveMonitor> = Vec::new();
    if unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(collect_monitor),
            &mut monitors as *mut Vec<ActiveMonitor> as LPARAM,
        )
    } == 0
    {
        return Err(win_error("枚举显示器失败"));
    }

    distinguish_monitor_names(&mut monitors);
    Ok(monitors)
}

fn distinguish_monitor_names(monitors: &mut [ActiveMonitor]) {
    let mut counts = HashMap::new();
    for monitor in monitors.iter() {
        *counts.entry(monitor.model_id.clone()).or_insert(0usize) += 1;
    }
    for monitor in monitors.iter_mut() {
        if counts.get(&monitor.model_id).copied().unwrap_or(0) > 1 {
            monitor.monitor.name =
                format!("{} · {}", monitor.monitor.name, monitor.monitor.gdi_name);
        }
    }
}

fn matches_concrete_monitor(monitor: &ActiveMonitor, target: &str) -> bool {
    is_concrete_monitor_id(target) && monitor.interface_id.eq_ignore_ascii_case(target)
}

pub fn enumerate_monitors() -> Result<Vec<Monitor>, String> {
    active_monitors().map(|items| items.into_iter().map(|item| item.monitor).collect())
}

pub fn cursor_position() -> Result<(i32, i32), String> {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return Err(win_error("读取鼠标位置失败"));
    }
    Ok((point.x, point.y))
}

pub fn set_dpi_awareness() {
    // An existing application manifest may already have established this mode.
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Brightness {
    pub minimum: u32,
    pub current: u32,
    pub maximum: u32,
}

pub struct BrightnessHandle {
    physical: Vec<PHYSICAL_MONITOR>,
    target_id: String,
    model_id: String,
}

impl BrightnessHandle {
    pub fn open(stable_id: &str) -> Result<Self, String> {
        if !is_concrete_monitor_id(stable_id) {
            return Err(
                "旧版屏幕选择只有型号，请在设置中重新选择具体屏幕；未执行硬件操作".to_owned(),
            );
        }
        let matches: Vec<_> = active_monitors()?
            .into_iter()
            .filter(|item| matches_concrete_monitor(item, stable_id))
            .collect();
        if matches.len() > 1 {
            return Err("显示器设备身份不唯一，已拒绝硬件操作，请检查显示拓扑".to_owned());
        }
        let monitor = matches
            .into_iter()
            .next()
            .ok_or_else(|| "未找到所选显示器，请检查连接和生效屏幕设置".to_owned())?;

        let mut count = 0;
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(monitor.handle, &mut count) } == 0 {
            return Err(win_error("枚举物理显示器失败"));
        }
        if count == 0 {
            return Err("所选屏幕没有可控制的物理显示器".to_owned());
        }
        if count > 1 {
            return Err("所选逻辑屏幕包含多个物理显示器，请改用扩展显示模式后重试".to_owned());
        }
        let mut physical = vec![PHYSICAL_MONITOR::default(); count as usize];
        if unsafe { GetPhysicalMonitorsFromHMONITOR(monitor.handle, count, physical.as_mut_ptr()) }
            == 0
        {
            return Err(win_error("打开物理显示器失败"));
        }
        Ok(Self {
            physical,
            target_id: monitor.interface_id,
            model_id: monitor.model_id,
        })
    }

    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn read(&self) -> Result<Brightness, String> {
        let mut value = Brightness {
            minimum: 0,
            current: 0,
            maximum: 0,
        };
        let handle = self.physical[0].hPhysicalMonitor;
        if unsafe {
            GetMonitorBrightness(
                handle,
                &mut value.minimum,
                &mut value.current,
                &mut value.maximum,
            )
        } == 0
        {
            return Err(win_error("读取硬件亮度失败，请检查显示器的 DDC/CI 设置"));
        }
        Ok(value)
    }

    pub fn set(&self, value: u32) -> Result<(), String> {
        let handle = self.physical[0].hPhysicalMonitor;
        if unsafe { SetMonitorBrightness(handle, value) } == 0 {
            return Err(win_error("设置硬件亮度失败"));
        }
        Ok(())
    }
}

impl Drop for BrightnessHandle {
    fn drop(&mut self) {
        unsafe {
            DestroyPhysicalMonitors(self.physical.len() as u32, self.physical.as_ptr());
        }
    }
}

enum OverlayCommand {
    Cover(Rect, f64, bool, mpsc::SyncSender<Result<(), String>>),
    Hide(mpsc::SyncSender<Result<(), String>>),
    Stop,
}

#[derive(Clone, Copy, Debug)]
pub enum SystemEvent {
    Suspend,
    Resume,
    SessionEnding,
    OverlayInvalidated,
}

pub struct Overlay {
    sender: mpsc::Sender<OverlayCommand>,
    thread_id: u32,
    thread: Mutex<Option<JoinHandle<()>>>,
    system_events: Mutex<Option<mpsc::Receiver<SystemEvent>>>,
}

fn publish_system_event(window: HWND, event: SystemEvent) {
    let sender =
        unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const mpsc::Sender<SystemEvent>;
    if !sender.is_null() {
        // The Box backing this pointer stays alive until DestroyWindow returns.
        let _ = unsafe { &*sender }.send(event);
    }
}

unsafe extern "system" fn overlay_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCREATE => {
            let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
            unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize) };
            1
        }
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_POWERBROADCAST => {
            match wparam as u32 {
                PBT_APMSUSPEND => publish_system_event(window, SystemEvent::Suspend),
                PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => {
                    publish_system_event(window, SystemEvent::Resume)
                }
                _ => {}
            }
            1
        }
        WM_QUERYENDSESSION => {
            publish_system_event(window, SystemEvent::SessionEnding);
            1
        }
        WM_ENDSESSION => {
            publish_system_event(
                window,
                if wparam != 0 {
                    SystemEvent::SessionEnding
                } else {
                    SystemEvent::Resume
                },
            );
            0
        }
        WM_CLOSE => {
            unsafe { DestroyWindow(window) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

// A region remains owned here until SetWindowRgn succeeds; after that Windows
// owns and eventually frees it. Temporary cutout regions always stay ours.
struct OwnedRegion(HRGN);

impl OwnedRegion {
    fn rectangle(left: i32, top: i32, right: i32, bottom: i32) -> Result<Self, String> {
        let region = unsafe { CreateRectRgn(left, top, right, bottom) };
        if region.is_null() {
            Err(win_error("创建黑幕区域失败"))
        } else {
            Ok(Self(region))
        }
    }

    fn transfer_to_window(mut self, window: HWND) -> Result<(), String> {
        if unsafe { SetWindowRgn(window, self.0, 1) } == 0 {
            return Err(win_error("更新黑幕覆盖区域失败"));
        }
        self.0 = null_mut();
        Ok(())
    }
}

impl Drop for OwnedRegion {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { DeleteObject(self.0) };
        }
    }
}

fn overlay_dimensions(bounds: &Rect) -> Option<(i32, i32)> {
    let width = bounds.right.checked_sub(bounds.left)?;
    let height = bounds.bottom.checked_sub(bounds.top)?;
    (width > 0 && height > 0).then_some((width, height))
}

fn intersection_in_overlay(bounds: &Rect, active: &Rect) -> Option<Rect> {
    overlay_dimensions(bounds)?;
    if active.right <= active.left || active.bottom <= active.top {
        return None;
    }
    let left = bounds.left.max(active.left);
    let top = bounds.top.max(active.top);
    let right = bounds.right.min(active.right);
    let bottom = bounds.bottom.min(active.bottom);
    if left >= right || top >= bottom {
        return None;
    }
    Some(Rect {
        left: left.checked_sub(bounds.left)?,
        top: top.checked_sub(bounds.top)?,
        right: right.checked_sub(bounds.left)?,
        bottom: bottom.checked_sub(bounds.top)?,
    })
}

fn shell_window_class(name: &str) -> bool {
    matches!(
        name,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}

fn foreground_cutout(overlay: HWND, bounds: &Rect) -> Option<Rect> {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() || foreground == overlay {
        return None;
    }
    let window = unsafe { GetAncestor(foreground, GA_ROOT) };
    if window.is_null()
        || window == overlay
        || window == unsafe { GetDesktopWindow() }
        || window == unsafe { GetShellWindow() }
        || unsafe { IsWindow(window) } == 0
        || unsafe { IsWindowVisible(window) } == 0
        || unsafe { IsIconic(window) } != 0
    {
        return None;
    }
    let mut class_name = [0u16; 256];
    let length = unsafe { GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32) };
    if length <= 0 || shell_window_class(&from_wide(&class_name)) {
        return None;
    }
    let mut cloaked = 0u32;
    if unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED as u32,
            (&mut cloaked as *mut u32).cast::<c_void>(),
            size_of::<u32>() as u32,
        )
    } < 0
        || cloaked != 0
    {
        return None;
    }
    let mut frame = RECT::default();
    if unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut frame as *mut RECT).cast::<c_void>(),
            size_of::<RECT>() as u32,
        )
    } < 0
        || unsafe { GetForegroundWindow() } != foreground
        || unsafe { IsWindow(window) } == 0
    {
        return None;
    }
    intersection_in_overlay(
        bounds,
        &Rect {
            left: frame.left,
            top: frame.top,
            right: frame.right,
            bottom: frame.bottom,
        },
    )
}

fn cutout_region(bounds: &Rect, cutout: &Rect) -> Result<OwnedRegion, String> {
    let (width, height) = overlay_dimensions(bounds).ok_or_else(|| "黑幕范围无效".to_owned())?;
    let cover = OwnedRegion::rectangle(0, 0, width, height)?;
    let hole = OwnedRegion::rectangle(cutout.left, cutout.top, cutout.right, cutout.bottom)?;
    // NULLREGION (1) is valid: a maximized foreground window can fill the screen.
    if unsafe { CombineRgn(cover.0, cover.0, hole.0, RGN_DIFF) } == 0 {
        return Err(win_error("计算黑幕覆盖区域失败"));
    }
    Ok(cover)
}

#[derive(Default)]
struct OverlayCoverage {
    bounds: Option<Rect>,
    show_active_window: bool,
    last_cutout: Option<Rect>,
    region_current: bool,
    tracking: bool,
    last_refresh_error: Option<String>,
    last_alpha: Option<u8>,
    positioned_bounds: Option<Rect>,
}

impl OverlayCoverage {
    fn clear_render_cache(&mut self) {
        self.last_alpha = None;
        self.positioned_bounds = None;
    }

    fn needs_alpha_update(&self, alpha: u8) -> bool {
        self.last_alpha != Some(alpha)
    }

    fn needs_position_update(&self, bounds: &Rect) -> bool {
        self.positioned_bounds.as_ref() != Some(bounds)
    }

    fn stop_tracking(&mut self, window: HWND) {
        if self.tracking {
            unsafe { KillTimer(window, FOREGROUND_TIMER) };
            self.tracking = false;
        }
    }

    fn hide(&mut self, window: HWND) {
        self.stop_tracking(window);
        self.clear_render_cache();
        self.bounds = None;
        self.show_active_window = false;
        self.region_current = false;
        self.last_refresh_error = None;
        unsafe { ShowWindow(window, SW_HIDE) };
    }

    fn clear_region(&mut self, window: HWND) -> Result<(), String> {
        self.region_current = false;
        if unsafe { SetWindowRgn(window, null_mut(), 1) } == 0 {
            return Err(win_error("恢复完整黑幕覆盖失败"));
        }
        self.last_cutout = None;
        self.region_current = true;
        Ok(())
    }

    fn refresh_region(&mut self, window: HWND) -> Result<(), String> {
        let Some(bounds) = self.bounds.as_ref() else {
            return Ok(());
        };
        let cutout = self
            .show_active_window
            .then(|| foreground_cutout(window, bounds))
            .flatten();
        if self.region_current && self.last_cutout == cutout {
            return Ok(());
        }
        if let Some(hole) = &cutout {
            let applied =
                cutout_region(bounds, hole).and_then(|region| region.transfer_to_window(window));
            if let Err(error) = applied {
                // Do not leave a stale opening on a previous foreground window.
                let _ = self.clear_region(window);
                return Err(error);
            }
            self.last_cutout = cutout;
            self.region_current = true;
            Ok(())
        } else {
            self.clear_region(window)
        }
    }

    fn tick(&mut self, window: HWND) {
        match self.refresh_region(window) {
            Ok(()) => self.last_refresh_error = None,
            Err(error) => {
                if !self.region_current {
                    // If even restoring a full rectangle failed, hide rather than
                    // leave a stale opening belonging to a previous window.
                    self.hide(window);
                    publish_system_event(window, SystemEvent::OverlayInvalidated);
                }
                if self.last_refresh_error.as_ref() != Some(&error) {
                    crate::config::log(&error);
                    self.last_refresh_error = Some(error);
                }
            }
        }
    }
}

fn cover_window(
    window: HWND,
    bounds: Rect,
    opacity: f64,
    show_active_window: bool,
    coverage: &mut OverlayCoverage,
) -> Result<(), String> {
    let dimensions = overlay_dimensions(&bounds);
    if !opacity.is_finite() || dimensions.is_none() {
        coverage.hide(window);
        return Err("黑幕范围或透明度无效".to_owned());
    }
    if opacity <= 0.0 {
        coverage.hide(window);
        return Ok(());
    }
    let (width, height) = dimensions.unwrap();
    if coverage.bounds.as_ref() != Some(&bounds) {
        // A new monitor has a different local region coordinate space.
        unsafe { ShowWindow(window, SW_HIDE) };
        coverage.region_current = false;
    }
    coverage.bounds = Some(bounds.clone());
    coverage.show_active_window = show_active_window;
    if !show_active_window {
        coverage.stop_tracking(window);
    }
    if let Err(error) = coverage.refresh_region(window) {
        coverage.hide(window);
        return Err(error);
    }
    if show_active_window && !coverage.tracking {
        if unsafe { SetTimer(window, FOREGROUND_TIMER, FOREGROUND_POLL_MILLISECONDS, None) } == 0 {
            let error = win_error("启动活跃窗口跟踪失败");
            let _ = coverage.clear_region(window);
            coverage.hide(window);
            return Err(error);
        }
        coverage.tracking = true;
    }
    let alpha = crate::overlay_animation::opacity_alpha(opacity);
    if coverage.needs_alpha_update(alpha) {
        if unsafe { SetLayeredWindowAttributes(window, 0, alpha, LWA_ALPHA) } == 0 {
            let error = win_error("更新黑幕透明度失败");
            coverage.hide(window);
            return Err(error);
        }
        coverage.last_alpha = Some(alpha);
    }
    if coverage.needs_position_update(&bounds) {
        if unsafe {
            SetWindowPos(
                window,
                HWND_TOPMOST,
                bounds.left,
                bounds.top,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        } == 0
        {
            let error = win_error("更新黑幕位置失败");
            coverage.hide(window);
            return Err(error);
        }
        coverage.positioned_bounds = Some(bounds);
    }
    Ok(())
}

fn overlay_thread(
    receiver: mpsc::Receiver<OverlayCommand>,
    ready: mpsc::SyncSender<Result<u32, String>>,
    system_events: mpsc::Sender<SystemEvent>,
) {
    unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let class_name = wide("FishcreenRustOverlayWindow");
    let title = wide("fishcreen native overlay");
    let module = unsafe { GetModuleHandleW(null()) };
    let class = WNDCLASSW {
        lpfnWndProc: Some(overlay_proc),
        hInstance: module,
        hbrBackground: unsafe { GetStockObject(BLACK_BRUSH) },
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        // Re-creating an overlay after a settings restart can reuse its class.
        const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
        if unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS {
            let _ = ready.send(Err(win_error("注册黑幕窗口失败")));
            return;
        }
    }
    let mut event_sender = Box::new(system_events);
    let window = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            module,
            (&mut *event_sender as *mut mpsc::Sender<SystemEvent>).cast::<c_void>(),
        )
    };
    if window.is_null() {
        let _ = ready.send(Err(win_error("创建原生黑幕失败")));
        return;
    }
    if ready.send(Ok(unsafe { GetCurrentThreadId() })).is_err() {
        unsafe { DestroyWindow(window) };
        return;
    }

    let mut message = MSG::default();
    let mut coverage = OverlayCoverage::default();
    'messages: loop {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        if message.message == OVERLAY_MESSAGE {
            while let Ok(command) = receiver.try_recv() {
                match command {
                    OverlayCommand::Cover(bounds, opacity, show_active_window, response) => {
                        let _ = response.send(cover_window(
                            window,
                            bounds,
                            opacity,
                            show_active_window,
                            &mut coverage,
                        ));
                    }
                    OverlayCommand::Hide(response) => {
                        coverage.hide(window);
                        let _ = response.send(Ok(()));
                    }
                    OverlayCommand::Stop => break 'messages,
                }
            }
        } else if message.hwnd == window
            && message.message == WM_TIMER
            && message.wParam == FOREGROUND_TIMER
        {
            coverage.tick(window);
        } else {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    coverage.stop_tracking(window);
    if unsafe { IsWindow(window) } != 0 {
        unsafe { DestroyWindow(window) };
    }
}

impl Overlay {
    pub fn new() -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let (events_sender, events_receiver) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("fishcreen-overlay".to_owned())
            .spawn(move || overlay_thread(receiver, ready_sender, events_sender))
            .map_err(|error| format!("启动原生黑幕线程失败：{error}"))?;
        match ready_receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(thread_id)) => Ok(Self {
                sender,
                thread_id,
                thread: Mutex::new(Some(thread)),
                system_events: Mutex::new(Some(events_receiver)),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(error) => Err(format!("等待原生黑幕初始化失败：{error}")),
        }
    }

    pub fn take_system_events(&mut self) -> Option<mpsc::Receiver<SystemEvent>> {
        self.system_events.lock().ok()?.take()
    }

    fn wake(&self) -> Result<(), String> {
        if unsafe { PostThreadMessageW(self.thread_id, OVERLAY_MESSAGE, 0, 0) } == 0 {
            return Err(win_error("唤醒黑幕线程失败"));
        }
        Ok(())
    }

    pub fn cover(
        &self,
        bounds: Rect,
        opacity: f64,
        show_active_window: bool,
    ) -> Result<(), String> {
        let (response, result) = mpsc::sync_channel(1);
        self.sender
            .send(OverlayCommand::Cover(
                bounds,
                opacity,
                show_active_window,
                response,
            ))
            .map_err(|_| "黑幕线程已经关闭".to_owned())?;
        self.wake()?;
        result
            .recv_timeout(Duration::from_secs(2))
            .map_err(|error| format!("等待黑幕更新失败：{error}"))?
    }

    pub fn hide(&self) -> Result<(), String> {
        let (response, result) = mpsc::sync_channel(1);
        self.sender
            .send(OverlayCommand::Hide(response))
            .map_err(|_| "黑幕线程已经关闭".to_owned())?;
        self.wake()?;
        result
            .recv_timeout(Duration::from_secs(2))
            .map_err(|error| format!("等待黑幕隐藏失败：{error}"))?
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        let _ = self.sender.send(OverlayCommand::Stop);
        let _ = self.wake();
        if let Ok(mut thread) = self.thread.lock() {
            if let Some(thread) = thread.take() {
                let _ = thread.join();
            }
        }
    }
}

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

pub fn startup_enabled() -> Result<bool, String> {
    let subkey = wide(RUN_KEY);
    let name = wide(RUN_VALUE);
    let mut bytes = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut bytes,
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(false);
    }
    if status != ERROR_SUCCESS {
        return Err(registry_error("读取开机启动项失败", status));
    }
    let mut text = vec![0u16; (bytes as usize / size_of::<u16>()) + 1];
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            text.as_mut_ptr().cast::<c_void>(),
            &mut bytes,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(registry_error("读取开机启动项失败", status));
    }
    let configured = from_wide(&text);
    let configured = configured.trim().trim_matches('"');
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    Ok(PathBuf::from(configured)
        .to_string_lossy()
        .eq_ignore_ascii_case(&current.to_string_lossy()))
}

pub fn set_startup_enabled(enabled: bool) -> Result<(), String> {
    let subkey = wide(RUN_KEY);
    let name = wide(RUN_VALUE);
    let mut key = null_mut();
    let status = if enabled {
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                null(),
                &mut key,
                null_mut(),
            )
        }
    } else {
        unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut key,
            )
        }
    };
    if !enabled && status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    if status != ERROR_SUCCESS {
        return Err(registry_error("打开开机启动配置失败", status));
    }
    let key = RegistryKey(key);
    let status = if enabled {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let command = wide(&format!("\"{}\"", executable.display()));
        unsafe {
            RegSetValueExW(
                key.0,
                name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast::<u8>(),
                (command.len() * size_of::<u16>()) as u32,
            )
        }
    } else {
        unsafe { RegDeleteValueW(key.0, name.as_ptr()) }
    };
    if status == ERROR_SUCCESS || (!enabled && status == ERROR_FILE_NOT_FOUND) {
        Ok(())
    } else {
        Err(registry_error("修改开机启动项失败", status))
    }
}

pub struct SingleInstanceGuard(HANDLE);

impl SingleInstanceGuard {
    pub fn acquire() -> Result<Option<Self>, String> {
        let name = wide("Local\\fishcreen-DELA0E7");
        unsafe { SetLastError(ERROR_SUCCESS) };
        let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        let status = unsafe { GetLastError() };
        if handle.is_null() {
            return Err(registry_error("创建单实例锁失败", status));
        }
        if status == ERROR_ALREADY_EXISTS {
            unsafe { CloseHandle(handle) };
            return Ok(None);
        }
        Ok(Some(Self(handle)))
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANEL_A: &str = r"\\?\DISPLAY#DELA0E7#INSTANCE_A#{GUID}";
    const PANEL_B: &str = r"\\?\DISPLAY#DELA0E7#INSTANCE_B#{GUID}";

    #[test]
    fn overlay_render_cache_deduplicates_alpha_and_position_independently() {
        let bounds = Rect {
            left: -1440,
            top: 0,
            right: 0,
            bottom: 2560,
        };
        let mut coverage = OverlayCoverage::default();
        assert!(coverage.needs_alpha_update(128));
        assert!(coverage.needs_position_update(&bounds));
        coverage.last_alpha = Some(128);
        coverage.positioned_bounds = Some(bounds.clone());
        assert!(!coverage.needs_alpha_update(128));
        assert!(!coverage.needs_position_update(&bounds));
        assert!(coverage.needs_alpha_update(129));
        let moved = Rect {
            left: 0,
            top: 0,
            right: 1440,
            bottom: 2560,
        };
        assert!(coverage.needs_position_update(&moved));
        assert!(!coverage.needs_alpha_update(128));
        // Hiding invalidates both caches: showing the same frame must show and
        // position the native window again, not silently suppress the command.
        coverage.clear_render_cache();
        assert!(coverage.needs_alpha_update(128));
        assert!(coverage.needs_position_update(&bounds));
    }

    fn panel(identity: &str) -> ActiveMonitor {
        ActiveMonitor {
            monitor: Monitor {
                stable_id: identity.into(),
                name: "Dell S2417DG".into(),
                gdi_name: "DISPLAY1".into(),
                bounds: Rect {
                    left: 0,
                    top: 0,
                    right: 1440,
                    bottom: 2560,
                },
                primary: false,
            },
            handle: null_mut(),
            interface_id: identity.into(),
            model_id: "DELA0E7".into(),
        }
    }

    #[test]
    fn concrete_identity_survives_identical_panel_hotplug() {
        let mut single = vec![panel(PANEL_A)];
        distinguish_monitor_names(&mut single);
        let saved = single[0].monitor.stable_id.clone();
        let mut duplicate = vec![panel(PANEL_A), panel(PANEL_B)];
        distinguish_monitor_names(&mut duplicate);
        assert_eq!(duplicate[0].monitor.stable_id, saved);
        assert_ne!(
            duplicate[0].monitor.stable_id,
            duplicate[1].monitor.stable_id
        );
        duplicate.remove(1);
        distinguish_monitor_names(&mut duplicate);
        assert_eq!(duplicate[0].monitor.stable_id, saved);
        assert!(matches_concrete_monitor(&duplicate[0], &saved));
    }

    #[test]
    fn monitor_model_is_never_hardware_identity() {
        let original = panel(PANEL_A);
        let replacement = panel(PANEL_B);
        assert!(!matches_concrete_monitor(&original, "DELA0E7"));
        assert!(!matches_concrete_monitor(&replacement, PANEL_A));
        assert!(matches_concrete_monitor(
            &original,
            &PANEL_A.to_ascii_lowercase()
        ));
        assert!(!is_concrete_monitor_id("DISPLAY1"));
        assert!(!is_concrete_monitor_id(r"\\?\DISPLAY#DELA0E7##{GUID}"));
        assert_eq!(monitor_model_id(PANEL_A), Some("DELA0E7"));
        assert_eq!(monitor_model_id("DELA0E7"), None);
    }
    use windows_sys::Win32::Graphics::Gdi::{GetWindowRgn, PtInRegion};

    #[test]
    fn portrait_negative_coordinates_have_exclusive_right_and_bottom_edges() {
        let bounds = Rect {
            left: -1440,
            top: 0,
            right: 0,
            bottom: 2560,
        };
        assert!(bounds.contains(-1440, 0));
        assert!(bounds.contains(-1, 2559));
        assert!(!bounds.contains(0, 200));
        assert!(!bounds.contains(-10, 2560));
        assert!(!bounds.contains(-1441, 100));
    }

    #[test]
    fn foreground_intersection_uses_overlay_local_coordinates_and_clips_cross_screen() {
        let screen = Rect {
            left: -1440,
            top: -200,
            right: 0,
            bottom: 2360,
        };
        let foreground = Rect {
            left: -300,
            top: -400,
            right: 500,
            bottom: 800,
        };
        assert_eq!(
            intersection_in_overlay(&screen, &foreground),
            Some(Rect {
                left: 1140,
                top: 0,
                right: 1440,
                bottom: 1000,
            })
        );
        assert_eq!(
            intersection_in_overlay(&screen, &screen),
            Some(Rect {
                left: 0,
                top: 0,
                right: 1440,
                bottom: 2560,
            })
        );
    }

    #[test]
    fn nonintersecting_and_invalid_foreground_bounds_keep_full_coverage() {
        let screen = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        for foreground in [
            Rect {
                left: 1920,
                top: 0,
                right: 2560,
                bottom: 1080,
            },
            Rect {
                left: 10,
                top: 10,
                right: 10,
                bottom: 300,
            },
            Rect {
                left: 20,
                top: 300,
                right: 10,
                bottom: 10,
            },
        ] {
            assert_eq!(intersection_in_overlay(&screen, &foreground), None);
        }
        assert_eq!(
            overlay_dimensions(&Rect {
                left: i32::MIN,
                top: 0,
                right: i32::MAX,
                bottom: 10,
            }),
            None
        );
    }

    #[test]
    fn desktop_and_taskbar_classes_are_not_treated_as_active_applications() {
        for class in [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
        ] {
            assert!(shell_window_class(class));
        }
        assert!(!shell_window_class("Chrome_WidgetWin_1"));
        assert!(!shell_window_class("ApplicationFrameWindow"));
    }

    struct HiddenWindow(HWND);

    impl HiddenWindow {
        fn new() -> Self {
            let class_name = wide("FishcreenOverlayRegionUnitTest");
            let module = unsafe { GetModuleHandleW(null()) };
            let class = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: module,
                lpszClassName: class_name.as_ptr(),
                ..Default::default()
            };
            unsafe { RegisterClassW(&class) };
            let window = unsafe {
                CreateWindowExW(
                    WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                    class_name.as_ptr(),
                    wide("fishcreen hidden region test").as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    640,
                    480,
                    null_mut(),
                    null_mut(),
                    module,
                    null(),
                )
            };
            assert!(!window.is_null());
            Self(window)
        }
    }

    impl Drop for HiddenWindow {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    fn hidden_native_region_preserves_cutout_full_restore_and_timer_cleanup() {
        // This test never shows, activates, moves or focuses a window.
        let foreground_before = unsafe { GetForegroundWindow() };
        let window = HiddenWindow::new();
        let bounds = Rect {
            left: -640,
            top: 0,
            right: 0,
            bottom: 480,
        };
        let hole = Rect {
            left: 100,
            top: 100,
            right: 500,
            bottom: 400,
        };
        cutout_region(&bounds, &hole)
            .unwrap()
            .transfer_to_window(window.0)
            .unwrap();
        let readback = OwnedRegion::rectangle(0, 0, 0, 0).unwrap();
        assert_ne!(unsafe { GetWindowRgn(window.0, readback.0) }, 0);
        assert_ne!(unsafe { PtInRegion(readback.0, 10, 10) }, 0);
        assert_eq!(unsafe { PtInRegion(readback.0, 300, 200) }, 0);
        assert_ne!(unsafe { PtInRegion(readback.0, 600, 450) }, 0);

        // A foreground window filling the entire monitor leaves an empty region.
        cutout_region(
            &bounds,
            &Rect {
                left: 0,
                top: 0,
                right: 640,
                bottom: 480,
            },
        )
        .unwrap()
        .transfer_to_window(window.0)
        .unwrap();
        assert_eq!(unsafe { GetWindowRgn(window.0, readback.0) }, 1);
        assert_eq!(unsafe { PtInRegion(readback.0, 300, 200) }, 0);

        let mut coverage = OverlayCoverage {
            bounds: Some(bounds),
            last_cutout: Some(hole),
            region_current: true,
            ..Default::default()
        };
        coverage.refresh_region(window.0).unwrap();
        assert_eq!(unsafe { GetWindowRgn(window.0, readback.0) }, 0);
        assert!(coverage.last_cutout.is_none());

        assert_ne!(
            unsafe {
                SetTimer(
                    window.0,
                    FOREGROUND_TIMER,
                    FOREGROUND_POLL_MILLISECONDS,
                    None,
                )
            },
            0
        );
        coverage.tracking = true;
        coverage.hide(window.0);
        assert!(!coverage.tracking);
        assert!(coverage.bounds.is_none());
        assert_eq!(unsafe { KillTimer(window.0, FOREGROUND_TIMER) }, 0);
        assert_eq!(unsafe { IsWindowVisible(window.0) }, 0);
        assert_eq!(unsafe { GetForegroundWindow() }, foreground_before);
    }
}
