use crate::Event;
use async_channel::Sender;
use gpui_kit::Window;
use gulu_core::ClockSample;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, RemoteDesktop::*},
    UI::WindowsAndMessaging::*,
};
static LOCKED: AtomicBool = AtomicBool::new(false);
static SENDER: OnceLock<Sender<Event>> = OnceLock::new();
#[link(name = "kernel32")]
unsafe extern "system" {
    fn QueryUnbiasedInterruptTime(t: *mut u64) -> i32;
    fn QueryInterruptTime(t: *mut u64);
}
pub fn clock() -> ClockSample {
    let (mut awake, mut continuous) = (0, 0);
    unsafe {
        QueryUnbiasedInterruptTime(&mut awake);
        QueryInterruptTime(&mut continuous);
    }
    ClockSample {
        wall: chrono::Local::now().fixed_offset(),
        awake_ms: awake / 10_000,
        continuous_ms: continuous / 10_000,
        locked: LOCKED.load(Ordering::Relaxed),
    }
}
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if matches!(msg, WM_DISPLAYCHANGE | WM_SETTINGCHANGE) {
        if let Some(tx) = SENDER.get() {
            let _ = tx.try_send(Event::Displays);
        }
    }
    if msg == WM_WTSSESSION_CHANGE {
        if w as u32 == WTS_SESSION_LOCK {
            LOCKED.store(true, Ordering::Relaxed);
        }
        if w as u32 == WTS_SESSION_UNLOCK {
            LOCKED.store(false, Ordering::Relaxed);
        }
        if let Some(tx) = SENDER.get() {
            let _ = tx.try_send(Event::Tick);
        }
    }
    if msg == WM_POWERBROADCAST {
        if let Some(tx) = SENDER.get() {
            let _ = tx.try_send(Event::Tick);
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, w, l) }
}
pub fn init(sender: Sender<Event>) {
    let _ = SENDER.set(sender);
    std::thread::spawn(|| unsafe {
        let class: Vec<u16> = "GuluTwistSessionEvents\0".encode_utf16().collect();
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            if let Some(tx) = SENDER.get() {
                let _ = tx.try_send(Event::Error(
                    "无法监听 Windows 锁屏事件，提醒已暂停。".into(),
                ));
            }
            return;
        }
        if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) == 0 {
            if let Some(tx) = SENDER.get() {
                let _ = tx.try_send(Event::Error(
                    "无法注册 Windows 会话通知，提醒已暂停。".into(),
                ));
            }
        }
        let mut data = std::ptr::null_mut();
        let mut len = 0;
        if WTSQuerySessionInformationW(
            std::ptr::null_mut(),
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut data,
            &mut len,
        ) != 0
        {
            if len as usize >= std::mem::size_of::<WTSINFOEXW>() {
                let info = &*(data as *const WTSINFOEXW);
                if info.Level == 1 {
                    LOCKED.store(
                        info.Data.WTSInfoExLevel1.SessionFlags as u32 == WTS_SESSIONSTATE_LOCK,
                        Ordering::Relaxed,
                    );
                }
            }
            WTSFreeMemory(data as _);
        }
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        WTSUnRegisterSessionNotification(hwnd);
        DestroyWindow(hwnd);
    });
}
pub fn prepare_overlay(window: &Window) -> anyhow::Result<()> {
    if let RawWindowHandle::Win32(h) = HasWindowHandle::window_handle(window)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?
        .as_raw()
    {
        unsafe {
            if SetWindowPos(
                h.hwnd.get() as HWND,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            ) == 0
            {
                anyhow::bail!("无法置顶遮罩");
            }
        }
        Ok(())
    } else {
        anyhow::bail!("窗口类型不受支持")
    }
}
pub fn hide(window: &Window) {
    if let Ok(h) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(h) = h.as_raw() {
            unsafe {
                ShowWindow(h.hwnd.get() as HWND, SW_HIDE);
            }
        }
    }
}
pub fn notification_settings() {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", "ms-settings:notifications"])
        .spawn();
}
pub type FocusTarget = isize;
pub fn capture_focus() -> Option<FocusTarget> {
    let h = unsafe { GetForegroundWindow() };
    if h.is_null() { None } else { Some(h as isize) }
}
pub fn restore_focus(h: FocusTarget) {
    unsafe {
        SetForegroundWindow(h as HWND);
    }
}
pub fn contains_pointer(window: &Window) -> bool {
    if let Ok(h) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(h) = h.as_raw() {
            unsafe {
                let mut p: POINT = std::mem::zeroed();
                let mut r: RECT = std::mem::zeroed();
                GetCursorPos(&mut p);
                GetWindowRect(h.hwnd.get() as HWND, &mut r);
                return p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom;
            }
        }
    }
    false
}
pub fn startup_error(message: &str) {
    let title: Vec<u16> = "咕嘟扭扭暂时无法启动\0".encode_utf16().collect();
    let text: Vec<u16> = format!("提醒没有启动，请检查数据目录是否可写。\n{message}\0")
        .encode_utf16()
        .collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
pub fn is_visible(window: &Window) -> bool {
    if let Ok(h) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(h) = h.as_raw() {
            return unsafe {
                IsWindowVisible(h.hwnd.get() as HWND) != 0 && IsIconic(h.hwnd.get() as HWND) == 0
            };
        }
    }
    false
}

/// Use the executable's embedded multi-resolution icon for both window sizes.
pub fn set_window_icon(window: &Window) {
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::Win32(h) = handle.as_raw()
    {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            for (kind, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
                let size = GetSystemMetrics(metric);
                let icon = LoadImageW(
                    instance,
                    1usize as *const u16,
                    IMAGE_ICON,
                    size,
                    size,
                    LR_SHARED,
                );
                if !icon.is_null() {
                    SendMessageW(
                        h.hwnd.get() as HWND,
                        WM_SETICON,
                        kind as usize,
                        icon as isize,
                    );
                }
            }
        }
    }
}

fn floating_hwnd(window: &Window) -> anyhow::Result<HWND> {
    if let RawWindowHandle::Win32(h) = HasWindowHandle::window_handle(window)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?
        .as_raw()
    {
        Ok(h.hwnd.get() as HWND)
    } else {
        anyhow::bail!("窗口类型不受支持")
    }
}
pub fn prepare_floating(window: &Window, _: bool) -> anyhow::Result<()> {
    let hwnd = floating_hwnd(window)?;
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(
            hwnd,
            GWL_STYLE,
            ((style & !(WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX)) | WS_POPUP)
                as isize,
        );
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ((ex & !WS_EX_APPWINDOW) | WS_EX_TOOLWINDOW) as isize,
        );
        if SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        ) == 0
        {
            anyhow::bail!("无法创建悬浮窗口样式");
        }
    }
    refresh_floating_shape(window);
    Ok(())
}
pub fn show_floating(window: &Window) {
    if let Ok(hwnd) = floating_hwnd(window) {
        unsafe {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
}
pub fn activate_floating_menu(window: &mut Window) {
    if let Ok(hwnd) = floating_hwnd(window) {
        // GPUI activation reapplies the hidden window's original framed placement.
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(hwnd);
        }
    }
}
pub fn start_floating_move(window: &Window) -> anyhow::Result<()> {
    queue_floating_move(floating_hwnd(window)?)
}
fn queue_floating_move(hwnd: HWND) -> anyhow::Result<()> {
    unsafe {
        windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
        // Post instead of Send: the native move loop must start after GPUI releases App.
        if PostMessageW(hwnd, WM_SYSCOMMAND, (SC_MOVE | HTCAPTION) as WPARAM, 0) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}
pub fn position_floating(
    window: &mut Window,
    bounds: gpui_kit::Bounds<gpui_kit::Pixels>,
    display: &dyn gpui_kit::PlatformDisplay,
    cx: &mut gpui_kit::App,
) -> anyhow::Result<()> {
    use windows_sys::Win32::Graphics::Gdi::*;
    let hwnd = floating_hwnd(window)?;
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    unsafe {
        if GetMonitorInfoW(u64::from(display.id()) as HMONITOR, &mut info) == 0 {
            anyhow::bail!("无法取得屏幕工作区");
        }
        let work = display.visible_bounds();
        let scale = (info.rcWork.right - info.rcWork.left) as f32 / f32::from(work.size.width);
        let x =
            info.rcWork.left + (f32::from(bounds.origin.x - work.origin.x) * scale).round() as i32;
        let y =
            info.rcWork.top + (f32::from(bounds.origin.y - work.origin.y) * scale).round() as i32;
        if SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            (f32::from(bounds.size.width) * scale).round() as i32,
            (f32::from(bounds.size.height) * scale).round() as i32,
            SWP_NOACTIVATE,
        ) == 0
        {
            anyhow::bail!("无法移动悬浮窗");
        }
    }
    // WM_SIZE cannot borrow App here; notify observers after the view update ends.
    window.defer(cx, |window, cx| window.bounds_changed(cx));
    refresh_floating_shape(window);
    Ok(())
}
pub fn refresh_floating_shape(window: &Window) {
    use windows_sys::Win32::Graphics::Gdi::*;
    if let Ok(hwnd) = floating_hwnd(window) {
        unsafe {
            let mut r: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut r) == 0 {
                return;
            }
            let diameter = if f32::from(window.bounds().size.height) < 100. {
                r.bottom - r.top
            } else {
                (24. * window.scale_factor()) as i32
            };
            let region = CreateRoundRectRgn(
                0,
                0,
                r.right - r.left + 1,
                r.bottom - r.top + 1,
                diameter,
                diameter,
            );
            if !region.is_null() && SetWindowRgn(hwnd, region, 1) == 0 {
                DeleteObject(region);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetCapture, SetCapture};

    #[test]
    fn floating_move_releases_capture_and_queues_native_move() {
        unsafe {
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                200,
                52,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null());
            SetCapture(hwnd);
            let captured = GetCapture() == hwnd;
            let result = queue_floating_move(hwnd);
            let released = GetCapture().is_null();
            let mut message: MSG = std::mem::zeroed();
            let queued = PeekMessageW(&mut message, hwnd, WM_SYSCOMMAND, WM_SYSCOMMAND, PM_REMOVE);
            DestroyWindow(hwnd);

            assert!(captured);
            result.unwrap();
            assert!(released);
            assert_ne!(queued, 0);
            assert_eq!(message.wParam, (SC_MOVE | HTCAPTION) as WPARAM);
        }
    }
}
