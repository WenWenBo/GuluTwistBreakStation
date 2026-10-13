use crate::Event;
use async_channel::Sender;
use gpui_kit::Window;
use gulu_core::ClockSample;
use objc2::{AnyThread, MainThreadMarker, msg_send, runtime::AnyObject};
use objc2_app_kit::{
    NSApplication, NSImage, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSData, NSDistributedNotificationCenter, NSNotificationCenter, NSString};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
};
static LOCKED: AtomicBool = AtomicBool::new(false);
#[repr(C)]
struct Timebase {
    numer: u32,
    denom: u32,
}
unsafe extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_continuous_time() -> u64;
    fn mach_timebase_info(info: *mut Timebase) -> i32;
}
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGSessionCopyCurrentDictionary() -> *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(
        alloc: *const c_void,
        s: *const i8,
        encoding: u32,
    ) -> *const c_void;
    fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
    fn CFBooleanGetValue(value: *const c_void) -> bool;
    fn CFRelease(value: *const c_void);
}
pub fn clock() -> ClockSample {
    let (awake_ms, continuous_ms) = unsafe {
        let mut t = Timebase { numer: 0, denom: 0 };
        mach_timebase_info(&mut t);
        let ms = |v: u64| ((v as u128 * t.numer as u128) / (t.denom as u128 * 1_000_000)) as u64;
        (ms(mach_absolute_time()), ms(mach_continuous_time()))
    };
    ClockSample {
        wall: chrono::Local::now().fixed_offset(),
        awake_ms,
        continuous_ms,
        locked: LOCKED.load(Ordering::Relaxed),
    }
}
pub fn init(sender: Sender<Event>) {
    // Dock can retain an older LaunchServices icon after replacing a bundle.
    // Set this process's icon from the same embedded artwork as the About page.
    let mtm = MainThreadMarker::new().expect("macOS initialization must run on the main thread");
    let bytes = NSData::with_bytes(include_bytes!("../../../../assets/icons/app.png"));
    if let Some(icon) = NSImage::initWithData(NSImage::alloc(), &bytes) {
        let app = NSApplication::sharedApplication(mtm);
        // A non-nil image is supplied; AppKit retains it for the running app.
        unsafe { app.setApplicationIconImage(Some(&icon)) };
        if std::env::var_os("GULU_DIAGNOSTICS").is_some() {
            eprintln!("dock_icon=embedded-small-gulu size={:?}", icon.size());
        }
    } else {
        eprintln!("无法解码应用图标，使用系统包图标");
    }
    // Native session query also handles starting the app while the session is locked.
    unsafe {
        let dict = CGSessionCopyCurrentDictionary();
        if !dict.is_null() {
            let key = CFStringCreateWithCString(
                std::ptr::null(),
                c"CGSSessionScreenIsLocked".as_ptr(),
                0x08000100,
            );
            let value = CFDictionaryGetValue(dict, key);
            if !value.is_null() {
                LOCKED.store(CFBooleanGetValue(value), Ordering::Relaxed);
            }
            CFRelease(key);
            CFRelease(dict);
        }
        let center = NSDistributedNotificationCenter::defaultCenter();
        let base: &NSNotificationCenter = &center;
        for (name, locked) in [
            ("com.apple.screenIsLocked", true),
            ("com.apple.screenIsUnlocked", false),
        ] {
            let tx = sender.clone();
            let block = block2::RcBlock::new(
                move |_: std::ptr::NonNull<objc2_foundation::NSNotification>| {
                    LOCKED.store(locked, Ordering::Relaxed);
                    let _ = tx.try_send(Event::Tick);
                },
            );
            let token = base.addObserverForName_object_queue_usingBlock(
                Some(&NSString::from_str(name)),
                None,
                None,
                &block,
            );
            // NotificationCenter retains the block. Observers live for the process lifetime.
            std::mem::forget(token);
        }
    }
}
fn ns_window(window: &Window) -> anyhow::Result<&NSWindow> {
    let handle = HasWindowHandle::window_handle(window).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    if let RawWindowHandle::AppKit(h) = handle.as_raw() {
        unsafe {
            let view = &*(h.ns_view.as_ptr() as *const AnyObject);
            let win: *const NSWindow = msg_send![view, window];
            return win.as_ref().ok_or_else(|| anyhow::anyhow!("窗口尚未创建"));
        }
    }
    anyhow::bail!("窗口类型不受支持")
}
pub fn prepare_overlay(window: &Window) -> anyhow::Result<()> {
    let win = ns_window(window)?;
    // GPUI's titlebar=None still creates a titled full-content window. macOS
    // constrains that window below the menu bar, pushing its bottom offscreen.
    // A true borderless overlay respects the complete screen frame.
    win.setStyleMask(NSWindowStyleMask::Borderless);
    win.setLevel(25); // Above normal windows; never capture the secure desktop.
    win.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );
    win.setIgnoresMouseEvents(false);
    // GPUI may constrain initial geometry to the visible work area; cover the full screen.
    if let Some(screen) = win.screen() {
        win.setFrame_display(screen.frame(), true);
    }
    Ok(())
}
pub fn hide(window: &Window) {
    if let Ok(win) = ns_window(window) {
        win.orderOut(None);
    }
}
pub fn notification_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.Notifications-Settings.extension")
        .spawn();
}
pub type FocusTarget = objc2::rc::Retained<objc2_app_kit::NSRunningApplication>;
pub fn capture_focus() -> Option<FocusTarget> {
    objc2_app_kit::NSWorkspace::sharedWorkspace().frontmostApplication()
}
#[allow(deprecated)]
pub fn restore_focus(app: FocusTarget) {
    app.activateWithOptions(
        objc2_app_kit::NSApplicationActivationOptions::ActivateIgnoringOtherApps,
    );
}
pub fn contains_pointer(window: &Window) -> bool {
    ns_window(window).ok().is_some_and(|w| {
        let p = objc2_app_kit::NSEvent::mouseLocation();
        let f = w.frame();
        p.x >= f.origin.x
            && p.y >= f.origin.y
            && p.x < f.origin.x + f.size.width
            && p.y < f.origin.y + f.size.height
    })
}
pub fn startup_error(message: &str) {
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        let alert = objc2_app_kit::NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str("咕嘟扭扭暂时无法启动"));
        alert.setInformativeText(&NSString::from_str(&format!(
            "提醒没有启动，请检查数据目录是否可写。\n{message}"
        )));
        alert.runModal();
    }
}
pub fn is_visible(window: &Window) -> bool {
    ns_window(window).is_ok_and(|w| w.isVisible() && !w.isMiniaturized())
}
