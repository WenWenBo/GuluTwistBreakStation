use crate::{Command, Event, Snapshot, platform};
use async_channel::Sender;
use gpui_kit::{
    component::{ActiveTheme, Disableable, Selectable, button::*},
    prelude::FluentBuilder,
    *,
};
use gulu_core::*;
use gulu_storage::FloatingPosition;
use std::{cell::RefCell, rc::Rc};

pub const WIDTH: f32 = 200.;
pub const HEIGHT: f32 = 52.;
pub const MENU_WIDTH: f32 = 216.;
pub const MENU_HEIGHT: f32 = 296.;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayState {
    pub title: String,
    pub value: String,
    pub ticking: bool,
    pub numeric: bool,
    pub settings: bool,
}
impl DisplayState {
    pub fn from_engine(engine: &Engine, error: bool) -> Self {
        let text = |title: String, value: &str, settings| Self {
            title,
            value: value.into(),
            settings,
            ..Self::default()
        };
        if error {
            return text("小站需要你".into(), "需要处理", false);
        }
        if !engine.settings.reminders.iter().any(|r| r.enabled) {
            return text("去设置开启".into(), "提醒未开启", true);
        }
        if !matches!(engine.pause, Pause::None) {
            return text(
                match &engine.pause {
                    Pause::Until(t) => format!("{} 恢复", t.format("%H:%M")),
                    _ => "等你准备好".into(),
                },
                "已暂停",
                false,
            );
        }
        if engine.settings.work_window(engine.clock.wall).is_none() {
            let next = engine.settings.next_start(engine.clock.wall, false);
            let day = if next.date_naive() == engine.clock.wall.date_naive() {
                "今天"
            } else {
                use chrono::Datelike;
                ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
                    [next.weekday().num_days_from_monday() as usize]
            };
            return text(format!("{day} {}", next.format("%H:%M")), "下班啦", false);
        }
        match engine.upcoming_summary() {
            ReminderSummary::Upcoming {
                kinds,
                remaining_ms,
                snoozed,
            } => Self {
                title: if snoozed {
                    if kinds.len() > 1 {
                        format!("稍后 · {} 项", kinds.len())
                    } else {
                        format!("稍后 · {}", short(kinds[0]))
                    }
                } else {
                    names(&kinds)
                },
                value: crate::ui::duration(remaining_ms.div_ceil(1000)),
                ticking: engine.running(),
                numeric: true,
                settings: false,
            },
            ReminderSummary::Due { kinds, notified } => text(
                names(&kinds),
                if notified { "待处理" } else { "待提醒" },
                false,
            ),
            ReminderSummary::None if !engine.pending().is_empty() => text(
                format!("{} 项等你完成", engine.pending().len()),
                "全部待处理",
                false,
            ),
            ReminderSummary::None => text("照顾好自己".into(), "休息中", false),
        }
    }
}
fn short(kind: ReminderKind) -> &'static str {
    match kind {
        ReminderKind::Water => "喝水",
        ReminderKind::Stretch => "颈肩",
        ReminderKind::Walk => "放风",
        ReminderKind::Stand => "站立",
        ReminderKind::EyeCare => "护眼",
    }
}
fn names(kinds: &[ReminderKind]) -> String {
    if kinds.len() > 1 {
        format!("{} + {} 项", short(kinds[0]), kinds.len() - 1)
    } else {
        match kinds[0] {
            ReminderKind::Water => "咕嘟喝水",
            ReminderKind::Stretch => "扭扭颈肩",
            ReminderKind::Walk => "放风一下",
            ReminderKind::Stand => "站一站",
            ReminderKind::EyeCare => "保护眼睛",
        }
        .into()
    }
}

pub fn placed_bounds(
    work: Bounds<Pixels>,
    position: Option<&FloatingPosition>,
    size: Size<Pixels>,
) -> Bounds<Pixels> {
    let max_x = (f32::from(work.size.width) - f32::from(size.width)).max(0.);
    let max_y = (f32::from(work.size.height) - f32::from(size.height)).max(0.);
    let x = position.map_or((max_x - 24.).max(0.), |p| p.x);
    let y = position.map_or((max_y - 24.).max(0.), |p| p.y);
    Bounds::new(
        work.origin + point(px(x.clamp(0., max_x)), px(y.clamp(0., max_y))),
        size,
    )
}
pub fn current_position(w: &Window, cx: &App) -> Option<FloatingPosition> {
    let display = w.display(cx)?;
    let relative = w.bounds().origin - display.visible_bounds().origin;
    Some(FloatingPosition {
        display_uuid: display.uuid().ok().map(|id| id.to_string()),
        x: relative.x.into(),
        y: relative.y.into(),
    })
}
pub(crate) fn colors(w: &Window) -> (u32, u32, u32, u32) {
    if matches!(
        w.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    ) {
        (0x253b33, 0xbacabd, 0xb8ddbf, 0x456052)
    } else {
        (0xfcfcf6, 0x607068, 0x2f6b58, 0xdce5da)
    }
}
pub struct FloatingView {
    shared: Rc<RefCell<Snapshot>>,
    tx: Sender<Event>,
    press: Option<Point<Pixels>>,
    focus: FocusHandle,
}
impl FloatingView {
    pub fn new(
        shared: Rc<RefCell<Snapshot>>,
        tx: Sender<Event>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_window_bounds(w, |this, w, cx| {
            if let Some(p) = current_position(w, cx) {
                let _ = this.tx.try_send(Event::FloatingMoved(p));
            }
            platform::refresh_floating_shape(w);
        })
        .detach();
        cx.observe_window_appearance(w, |_, _, cx| cx.notify())
            .detach();
        let focus = cx.focus_handle();
        w.focus(&focus, cx);
        Self {
            shared,
            tx,
            press: None,
            focus,
        }
    }
    fn open(&self) {
        let command = if self.shared.borrow().floating.settings {
            Command::OpenSettings
        } else {
            Command::OpenHome
        };
        let _ = self.tx.try_send(Event::Command(command));
    }
}
impl Render for FloatingView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.shared.borrow();
        let state = s.floating.clone();
        let (paper, muted, green, line) = colors(w);
        let title = state.title.clone();
        drop(s);
        div()
            .id("floating-capsule")
            .role(Role::Group)
            .aria_label("咕嘟扭扭悬浮窗")
            .font_family(cx.theme().font_family.clone())
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .rounded(px(HEIGHT / 2.))
            .bg(rgb(paper))
            .border_1()
            .border_color(rgb(line))
            .px(px(8.))
            .py(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, _, _| {
                    this.press = (f32::from(e.position.x) < WIDTH - 42.).then_some(e.position);
                }),
            )
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, w, _| {
                if e.dragging()
                    && this.press.is_some_and(|p| {
                        (f32::from(e.position.x - p.x)).abs() > 4.
                            || (f32::from(e.position.y - p.y)).abs() > 4.
                    })
                {
                    this.press = None;
                    w.start_window_move();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, e: &MouseUpEvent, _, _| {
                    if this.press.take().is_some() && e.click_count == 2 {
                        this.open();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.press = None;
                }),
            )
            .child(
                div()
                    .id("floating-open")
                    .track_focus(&self.focus)
                    .role(Role::Button)
                    // Keep the action name stable between ticks; the visible text
                    // exposes the time without a focused label changing each second.
                    .aria_label(format!("{title}，打开小站"))
                    .on_click(cx.listener(|this, event, _, _| {
                        // Mouse double-click activation is resolved by the parent after
                        // distinguishing a native window drag. Keyboard and accessibility clicks have
                        // no mouse-down event and must also reach the main window.
                        if !matches!(event, ClickEvent::Mouse(_)) {
                            this.open();
                        }
                    }))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .flex_1()
                    .min_w_0()
                    .cursor_pointer()
                    .child(img("gulu/app.png").size(px(38.)).flex_shrink_0())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(15.))
                                    .text_color(rgb(muted))
                                    .child(state.title),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(if state.numeric { 20. } else { 17. }))
                                    .line_height(px(23.))
                                    .text_color(rgb(green))
                                    .when(state.numeric, |d| {
                                        d.font_family(if cfg!(target_os = "macos") {
                                            "Menlo"
                                        } else {
                                            "Consolas"
                                        })
                                    })
                                    .child(state.value),
                            ),
                    ),
            )
            .child(
                Button::new("floating-more")
                    .label("···")
                    .accessibility_label("悬浮窗更多操作")
                    .ghost()
                    .text_color(rgb(green))
                    .w(px(28.))
                    .h(px(28.))
                    .p_0()
                    .on_click(cx.listener(|this, _, _, _| {
                        let _ = this.tx.try_send(Event::Command(Command::FloatingMenu));
                    })),
            )
    }
}

pub struct MenuView {
    shared: Rc<RefCell<Snapshot>>,
    tx: Sender<Event>,
    focus: FocusHandle,
    selected: usize,
}
impl MenuView {
    pub fn new(
        shared: Rc<RefCell<Snapshot>>,
        tx: Sender<Event>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_window_activation(w, |this, w, _| {
            if !w.is_window_active() {
                let _ = this.tx.try_send(Event::Command(Command::CloseFloatingMenu));
            }
        })
        .detach();
        let focus = cx.focus_handle();
        w.focus(&focus, cx);
        Self {
            shared,
            tx,
            focus,
            selected: 0,
        }
    }
    fn items(&self) -> Vec<(&'static str, Command)> {
        let paused = !matches!(self.shared.borrow().pause, Pause::None);
        vec![
            ("打开小站", Command::OpenHome),
            ("提醒设置", Command::OpenSettings),
            ("暂停 15 分钟", Command::PauseMinutes(15)),
            ("暂停 1 小时", Command::PauseMinutes(60)),
            ("暂停到下一工作日", Command::NextDay),
            (
                if paused {
                    "恢复提醒"
                } else {
                    "手动暂停"
                },
                if paused {
                    Command::Resume
                } else {
                    Command::Pause
                },
            ),
            ("重置悬浮窗位置", Command::ResetFloating),
            ("隐藏悬浮窗", Command::SetFloating(false)),
        ]
    }
    fn execute(&self, command: Command) {
        let _ = self.tx.try_send(Event::Command(Command::CloseFloatingMenu));
        let _ = self.tx.try_send(Event::Command(command));
    }
}
impl Render for MenuView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (paper, _, green, line) = colors(w);
        let saving = self.shared.borrow().ui_saving;
        div()
            .id("floating-menu")
            .track_focus(&self.focus)
            .font_family(cx.theme().font_family.clone())
            .size_full()
            .p(px(8.))
            .bg(rgb(paper))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(line))
            .flex()
            .flex_col()
            .gap(px(2.))
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let items = this.items();
                match e.keystroke.key.as_str() {
                    "escape" => {
                        let _ = this.tx.try_send(Event::Command(Command::CloseFloatingMenu));
                    }
                    "up" => this.selected = (this.selected + items.len() - 1) % items.len(),
                    "down" | "tab" => this.selected = (this.selected + 1) % items.len(),
                    "enter" | "space" => {
                        if this.selected < 6 || !this.shared.borrow().ui_saving {
                            this.execute(items[this.selected].1.clone());
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .children(
                self.items()
                    .into_iter()
                    .enumerate()
                    .map(|(i, (label, command))| {
                        Button::new(("floating-menu-item", i))
                            .label(label)
                            .accessibility_label(label)
                            .ghost()
                            .w_full()
                            .h(px(32.))
                            .text_color(rgb(green))
                            .selected(i == self.selected)
                            .when(i == self.selected, |button| button.bg(rgb(line)))
                            .disabled(i >= 6 && saving)
                            .on_click(
                                cx.listener(move |this, _, _, _| this.execute(command.clone())),
                            )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn display_uses_engine_time_and_real_pause_and_work_states() {
        let clock = ClockSample {
            wall: chrono::DateTime::parse_from_rfc3339("2026-10-13T10:00:00+08:00").unwrap(),
            awake_ms: 0,
            continuous_ms: 0,
            locked: false,
        };
        let mut settings = Settings {
            onboarded: true,
            ..Default::default()
        };
        for (i, r) in settings.reminders.iter_mut().enumerate() {
            r.enabled = i == 0;
            r.interval_minutes = 240;
        }
        let mut engine = Engine::new(settings, clock, Pause::None).unwrap();
        let state = DisplayState::from_engine(&engine, false);
        assert_eq!(state.value, "240:00");
        assert!(state.ticking);
        engine.reminders[0].remaining_ms = 1;
        assert_eq!(DisplayState::from_engine(&engine, false).value, "00:01");
        engine.set_pause(Pause::Manual, clock);
        let state = DisplayState::from_engine(&engine, false);
        assert_eq!(state.value, "已暂停");
        assert!(!state.ticking);
        engine.set_pause(Pause::None, clock);
        let mut later = clock;
        later.wall = chrono::DateTime::parse_from_rfc3339("2026-10-13T20:00:00+08:00").unwrap();
        engine.tick(later);
        let state = DisplayState::from_engine(&engine, false);
        assert_eq!(state.value, "下班啦");
        assert!(!state.ticking);
        engine.settings.reminders[0].enabled = false;
        assert!(DisplayState::from_engine(&engine, false).settings);
        assert_eq!(DisplayState::from_engine(&engine, true).value, "需要处理");
    }
    #[::core::prelude::v1::test]
    fn placement_recovers_offscreen_coordinates_on_negative_monitor() {
        let work = Bounds::new(point(px(-1920.), px(-200.)), size(px(1920.), px(1080.)));
        let pos = FloatingPosition {
            display_uuid: None,
            x: 4000.,
            y: -100.,
        };
        let placed = placed_bounds(work, Some(&pos), size(px(WIDTH), px(HEIGHT)));
        assert_eq!(placed.origin, point(px(-200.), px(-200.)));
        let small = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(40.)));
        assert_eq!(
            placed_bounds(small, None, size(px(WIDTH), px(HEIGHT))).origin,
            small.origin
        );
    }
}
