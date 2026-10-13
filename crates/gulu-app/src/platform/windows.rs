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
