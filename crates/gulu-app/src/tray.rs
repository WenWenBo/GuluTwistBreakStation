use crate::{Command, Event};
use async_channel::Sender;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
pub struct Tray {
    pub _icon: TrayIcon,
    pub status: MenuItem,
    pub floating: MenuItem,
}
impl Tray {
    pub fn new(tx: Sender<Event>) -> anyhow::Result<Self> {
        let menu = Menu::new();
        let status = MenuItem::with_id("status", "咕嘟扭扭 · 准备好了", false, None);
        menu.append(&MenuItem::with_id("open", "打开咕嘟扭扭", true, None))?;
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
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let c = match e.id.as_ref() {
                "floating" => Command::ToggleFloating,
                "open" => Command::Open,
                "pause15" => Command::PauseMinutes(15),
                "pause60" => Command::PauseMinutes(60),
                "nextday" => Command::NextDay,
                "pause" => Command::Pause,
                "resume" => Command::Resume,
                "quit" => Command::Quit,
                _ => return,
            };
            let _ = tx.try_send(Event::Command(c));
        }));
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
            .build()?;
        Ok(Self {
            _icon: icon,
            status,
            floating,
        })
    }
}
