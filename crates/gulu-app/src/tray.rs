use crate::{Command, Event, resident};
use async_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
pub struct Tray {
    icon: TrayIcon,
    pub status: MenuItem,
    pub floating: MenuItem,
}
impl Tray {
    pub fn new() -> anyhow::Result<Self> {
        static INJECTED: AtomicBool = AtomicBool::new(false);
        let fail_once =
            resident::diagnostic_failure("tray-once") && !INJECTED.swap(true, Ordering::Relaxed);
        anyhow::ensure!(
            !resident::diagnostic_failure("tray") && !fail_once,
            "托盘创建失败（诊断注入）"
        );
        let menu = Menu::new();
        let status = MenuItem::with_id("status", "咕嘟扭扭 · 准备好了", false, None);
        menu.append(&MenuItem::with_id("open", "打开咕嘟扭扭", true, None))?;
        menu.append(&MenuItem::with_id("settings", "提醒设置", true, None))?;
        menu.append(&status)?;
        let floating = MenuItem::with_id("floating", "显示悬浮窗", true, None);
        menu.append(&floating)?;
        menu.append(&PredefinedMenuItem::separator())?;
        for (id, title) in [
            ("pause15", "暂停 15 分钟"),
            ("pause60", "暂停 1 小时"),
            ("nextday", "暂停到下一工作日"),
            ("pause", "手动暂停"),
            ("resume", "恢复提醒"),
            ("quit", "退出咕嘟扭扭"),
        ] {
            menu.append(&MenuItem::with_id(id, title, true, None))?;
        }
        #[cfg(target_os = "macos")]
        let (rgba, size) = (
            include_bytes!("../../../assets/icons/tray-macos.rgba").to_vec(),
            36,
        );
        #[cfg(not(target_os = "macos"))]
        let (rgba, size) = (
            include_bytes!("../../../assets/icons/tray-windows.rgba").to_vec(),
            32,
        );
        let icon = Icon::from_rgba(rgba, size, size)?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("咕嘟扭扭放风站")
            .with_icon(icon)
            .with_icon_as_template(cfg!(target_os = "macos"))
            .with_menu_on_left_click(!cfg!(target_os = "windows"))
            .build()?;
        Ok(Self {
            icon,
            status,
            floating,
        })
    }

    pub fn check_available(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !resident::diagnostic_failure("tray-registration"),
            "托盘注册失败（诊断注入）"
        );
        // On Windows NIM_ADD may fail even when TrayIconBuilder returns Ok.
        // NIM_MODIFY confirms registration, including icons in the overflow area.
        #[cfg(target_os = "windows")]
        self.icon.set_tooltip(Some("咕嘟扭扭放风站"))?;
        #[cfg(target_os = "macos")]
        let _ = &self.icon; // Own the status item for the resident lifetime.
        Ok(())
    }
}

/// The native libraries install their global callbacks once, before creating any icons.
pub fn init(tx: Sender<Event>) {
    MenuEvent::set_event_handler(Some({
        let tx = tx.clone();
        move |e: MenuEvent| {
            let c = match e.id.as_ref() {
                "floating" => Command::ToggleFloating,
                "open" => Command::Open,
                "settings" => Command::OpenSettings,
                "pause15" => Command::PauseMinutes(15),
                "pause60" => Command::PauseMinutes(60),
                "nextday" => Command::NextDay,
                "pause" => Command::Pause,
                "resume" => Command::Resume,
                "quit" => Command::Quit,
                _ => return,
            };
            let _ = tx.try_send(Event::Command(c));
        }
    }));
    #[cfg(target_os = "windows")]
    tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
        if matches!(
            event,
            tray_icon::TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            }
        ) {
            let _ = tx.try_send(Event::Command(Command::Open));
        }
    }));
}
