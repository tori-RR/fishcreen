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
    Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, GetStockObject, BLACK_BRUSH,
        DISPLAY_DEVICEW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
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
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos,
            GetMessageW, GetWindowLongPtrW, PostQuitMessage, PostThreadMessageW, RegisterClassW,
            SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, ShowWindow,
            TranslateMessage, CREATESTRUCTW, EDD_GET_DEVICE_INTERFACE_NAME, GWLP_USERDATA,
            HTTRANSPARENT, HWND_TOPMOST, LWA_ALPHA, MA_NOACTIVATE, MONITORINFOF_PRIMARY, MSG,
            PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, SWP_NOACTIVATE,
            SWP_SHOWWINDOW, SW_HIDE, WM_APP, WM_CLOSE, WM_DESTROY, WM_ENDSESSION, WM_MOUSEACTIVATE,
            WM_NCCREATE, WM_NCHITTEST, WM_POWERBROADCAST, WM_QUERYENDSESSION, WNDCLASSW,
            WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "fishcreen";
const OVERLAY_MESSAGE: u32 = WM_APP + 17;

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

struct ActiveMonitor {
    monitor: Monitor,
    handle: HMONITOR,
    interface_id: String,
    model_id: String,
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
            stable_id: model_id.clone(),
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

    let mut counts = HashMap::new();
    for monitor in &monitors {
        *counts.entry(monitor.model_id.clone()).or_insert(0usize) += 1;
    }
    for monitor in &mut monitors {
        if counts.get(&monitor.model_id).copied().unwrap_or(0) > 1 {
            monitor.monitor.stable_id = monitor.interface_id.clone();
            monitor.monitor.name =
                format!("{} · {}", monitor.monitor.name, monitor.monitor.gdi_name);
        }
    }
    Ok(monitors)
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
}

impl BrightnessHandle {
    pub fn open(stable_id: &str) -> Result<Self, String> {
        if stable_id.trim().is_empty() {
            return Err("未选择生效屏幕".to_owned());
        }
        let matches: Vec<_> = active_monitors()?
            .into_iter()
            .filter(|item| {
                item.monitor.stable_id.eq_ignore_ascii_case(stable_id)
                    || item.interface_id.eq_ignore_ascii_case(stable_id)
                    || item.model_id.eq_ignore_ascii_case(stable_id)
            })
            .collect();
        if matches.len() > 1 {
            return Err("发现多台同型号显示器，请在设置中重新选择具体屏幕".to_owned());
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
        Ok(Self { physical })
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
    Cover(Rect, f64, mpsc::SyncSender<Result<(), String>>),
    Hide(mpsc::SyncSender<Result<(), String>>),
    Stop,
}

#[derive(Clone, Copy, Debug)]
pub enum SystemEvent {
    Suspend,
    Resume,
    SessionEnding,
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

fn cover_window(window: HWND, bounds: Rect, opacity: f64) -> Result<(), String> {
    if !opacity.is_finite() || bounds.right <= bounds.left || bounds.bottom <= bounds.top {
        return Err("黑幕范围或透明度无效".to_owned());
    }
    if opacity <= 0.0 {
        unsafe { ShowWindow(window, SW_HIDE) };
        return Ok(());
    }
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round().max(1.0) as u8;
    if unsafe { SetLayeredWindowAttributes(window, 0, alpha, LWA_ALPHA) } == 0 {
        return Err(win_error("更新黑幕透明度失败"));
    }
    if unsafe {
        SetWindowPos(
            window,
            HWND_TOPMOST,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    } == 0
    {
        return Err(win_error("更新黑幕位置失败"));
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
    'messages: loop {
        let result = unsafe { GetMessageW(&mut message, null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        if message.message == OVERLAY_MESSAGE {
            while let Ok(command) = receiver.try_recv() {
                match command {
                    OverlayCommand::Cover(bounds, opacity, response) => {
                        let _ = response.send(cover_window(window, bounds, opacity));
                    }
                    OverlayCommand::Hide(response) => {
                        unsafe { ShowWindow(window, SW_HIDE) };
                        let _ = response.send(Ok(()));
                    }
                    OverlayCommand::Stop => break 'messages,
                }
            }
        } else {
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    unsafe { DestroyWindow(window) };
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

    pub fn cover(&self, bounds: Rect, opacity: f64) -> Result<(), String> {
        let (response, result) = mpsc::sync_channel(1);
        self.sender
            .send(OverlayCommand::Cover(bounds, opacity, response))
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
    use super::Rect;

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
}
