use crate::{Command, Event, Snapshot, platform};
use async_channel::Sender;
use chrono::{NaiveTime, Timelike};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    component::{
        button::*,
        input::{Input, InputEvent, InputState},
        menu::{DropdownMenu, PopupMenuItem},
        switch::Switch,
        time_field::{TimeField, TimeFieldEvent, TimeFieldState},
        *,
    },
    *,
};
use gulu_core::*;
use std::{cell::RefCell, collections::HashSet, rc::Rc};

const INK: u32 = 0x243d35;
const MUTED: u32 = 0x607068;
const GREEN: u32 = 0x2f6b58;
const PAPER: u32 = 0xf7f7f2;
const REST: u32 = 0xeaf1e9;
const LINE: u32 = 0xdde5dc;
const DANGER: u32 = 0xa34237;
const CATEGORY_COLORS: [u32; ReminderKind::COUNT] =
    [0xe8f2f6, 0xefeaf6, 0xf8eedc, 0xe8f0df, 0xe4f1ef];
const NAV: [&str; 4] = ["我的小站", "提醒设置", "今日记录", "关于小站"];
const NAV_ICONS: [&str; 4] = ["home", "settings", "records", "about"];
pub(crate) const KIND_ICONS: [&str; ReminderKind::COUNT] =
    ["water", "stretch", "walk", "stand", "eye"];

pub fn duration(s: u64) -> String {
    format!("{:02}:{:02}", s / 60, s % 60)
}
pub(crate) fn icon(name: &str) -> Icon {
    let bytes: &[u8] = match name {
        "list" => include_bytes!("../../../assets/ui/list.svg"),
        "eye" => include_bytes!("../../../assets/ui/eye.svg"),
        "water" => include_bytes!("../../../assets/ui/water.svg"),
        "stretch" => include_bytes!("../../../assets/ui/stretch.svg"),
        "walk" => include_bytes!("../../../assets/ui/walk.svg"),
        "stand" => include_bytes!("../../../assets/ui/stand.svg"),
        "settings" => include_bytes!("../../../assets/ui/settings.svg"),
        "records" => include_bytes!("../../../assets/ui/records.svg"),
        "about" => include_bytes!("../../../assets/ui/about.svg"),
        "pause" => include_bytes!("../../../assets/ui/pause.svg"),
        "chevron" => include_bytes!("../../../assets/ui/chevron.svg"),
        _ => include_bytes!("../../../assets/ui/home.svg"),
    };
    Icon::default()
        .data(bytes)
        .text_color(rgb(GREEN))
        .size(px(20.))
}
fn mascot(size: f32) -> impl IntoElement {
    img("gulu/mascot.png").size(px(size)).flex_shrink_0()
}
fn button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    command: Command,
    tx: &Sender<Event>,
) -> Button {
    let label = label.into();
    let accessible = if let Command::Manual(kind, _) = &command {
        format!("{}，我已完成", kind.title())
    } else {
        label.to_string()
    };
    let tx = tx.clone();
    Button::new(id.into())
        .label(label)
        .accessibility_label(accessible)
        .h(px(36.))
        .on_click(move |_, _, _| {
            let _ = tx.try_send(Event::Command(command.clone()));
        })
}
fn menu_item(label: &'static str, command: Command, tx: &Sender<Event>) -> PopupMenuItem {
    let tx = tx.clone();
    PopupMenuItem::new(label).on_click(move |_, _, _| {
        let _ = tx.try_send(Event::Command(command.clone()));
    })
}
fn caption(text: impl Into<SharedString>) -> Stateful<Div> {
    let text: SharedString = text.into();
    div()
        .id(text.clone())
        .role(Role::Label)
        .aria_label(text.clone())
        .text_size(px(13.))
        .line_height(px(20.))
        .text_color(rgb(MUTED))
        .child(text)
}
fn heading(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(17.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}
fn card() -> Div {
    div()
        .v_flex()
        .gap_3()
        .p_4()
        .bg(rgb(0xffffff))
        .rounded(px(16.))
        .border_1()
        .border_color(rgb(LINE))
}
fn error_text(text: impl Into<SharedString>) -> Stateful<Div> {
    caption(text).text_color(rgb(DANGER))
}
fn badge(text: impl Into<SharedString>) -> Div {
    div()
        .px_3()
        .py_1()
        .rounded_full()
        .bg(rgb(REST))
        .text_color(rgb(GREEN))
        .text_size(px(12.))
        .child(text.into())
}
fn numeric(text: String, size: f32) -> Div {
    // A dedicated monospaced font and explicit metrics avoid inherited CJK/light
    // glyph metrics; the extra horizontal/vertical space protects ink overhang.
    div()
        .flex_shrink_0()
        .px_2()
        .text_size(px(size))
        .line_height(px(size * 1.4))
        .font_family(if cfg!(target_os = "macos") {
            "Menlo"
        } else {
            "Consolas"
        })
        .font_weight(FontWeight::NORMAL)
        .child(text)
}
fn section(title: &'static str, detail: &'static str) -> Div {
    div()
        .v_flex()
        .gap_1()
        .child(heading(title))
        .child(caption(detail))
}
fn digits_only(value: &str) -> bool {
    value.bytes().all(|b| b.is_ascii_digit())
}
fn numeric_input(input: &Entity<InputState>, cx: &Context<MainView>) -> Input {
    let view = cx.entity().downgrade();
    Input::new(input).on_paste(move |clipboard, _, cx| {
        // Check the original clipboard before the single-line input removes newlines.
        let rejected = clipboard.text().is_some_and(|text| !digits_only(&text));
        if rejected {
            let _ = view.update(cx, |this, cx| {
                this.error = "仅支持数字，请直接填写整数。".into();
                cx.notify();
            });
        }
        rejected
    })
}
fn field(text: String, w: &mut Window, cx: &mut Context<MainView>) -> Entity<InputState> {
    let view = cx.entity().downgrade();
    let input = cx.new(|cx| {
        InputState::new(w, cx)
            .default_value(text)
            .validate(move |value, cx| {
                let valid = digits_only(value);
                if !valid {
                    let _ = view.update(cx, |this, cx| {
                        this.error = "仅支持数字，请直接填写整数。".into();
                        cx.notify();
                    });
                }
                valid
            })
    });
    cx.subscribe_in(&input, w, |this, input, event: &InputEvent, w, cx| {
        if matches!(event, InputEvent::Blur) {
            let value = input.read(cx).value().to_string();
            if let Ok(number) = value.parse::<u32>() {
                let normalized = number.to_string();
                if value != normalized {
                    input.update(cx, |state, cx| state.set_value(normalized, w, cx));
                }
            }
        }
        if matches!(event, InputEvent::Change) && this.error.starts_with("仅支持数字") {
            this.error.clear();
        }
        cx.notify();
    })
    .detach();
    input
}

struct PeriodFields {
    start: Entity<TimeFieldState>,
    end: Entity<TimeFieldState>,
    end_of_day: bool,
}
impl PeriodFields {
    fn new(period: WorkPeriod, w: &mut Window, cx: &mut Context<MainView>) -> Self {
        let mut time_field = |minute: u16| {
            let state = cx.new(|cx| {
                let mut state = TimeFieldState::new(w, cx);
                state.set_time(minute_time(minute), w, cx);
                state
            });
            cx.subscribe(&state, |_, _, _: &TimeFieldEvent, cx| cx.notify())
                .detach();
            let focus = state.read(cx).focus_handle(cx);
            cx.on_blur(&focus, w, |_, _, cx| cx.notify()).detach();
            state
        };
        Self {
            start: time_field(period.start),
            end: time_field(if period.end == 1440 { 1439 } else { period.end }),
            end_of_day: period.end == 1440,
        }
    }
    fn value(&self, cx: &App) -> WorkPeriod {
        WorkPeriod {
            start: time_minute(self.start.read(cx).time()),
            end: if self.end_of_day {
                1440
            } else {
                time_minute(self.end.read(cx).time())
            },
        }
    }
    fn editing(&self, w: &Window, cx: &App) -> bool {
        self.start.read(cx).focus_handle(cx).is_focused(w)
            || self.end.read(cx).focus_handle(cx).is_focused(w)
    }
}
fn minute_time(minute: u16) -> NaiveTime {
    NaiveTime::from_hms_opt(u32::from(minute / 60), u32::from(minute % 60), 0).unwrap()
}
fn time_minute(time: NaiveTime) -> u16 {
    (time.hour() * 60 + time.minute()) as u16
}

pub struct MainView {
    shared: Rc<RefCell<Snapshot>>,
    tx: Sender<Event>,
    page: usize,
    form: Settings,
    intervals: [Entity<InputState>; ReminderKind::COUNT],
    minutes: [Entity<InputState>; ReminderKind::COUNT],
    seconds: [Entity<InputState>; ReminderKind::COUNT],
    periods: Vec<PeriodFields>,
    error: String,
    confirm_clear: bool,
    pending_page: Option<usize>,
    save_destination: Option<usize>,
}
impl MainView {
    pub fn new(
        shared: Rc<RefCell<Snapshot>>,
        tx: Sender<Event>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_window_activation(w, |this, _, _| {
            let _ = this.tx.try_send(Event::Tick);
        })
        .detach();
        cx.observe_window_visibility(w, |this, _, _, _| {
            let _ = this.tx.try_send(Event::Tick);
        })
        .detach();
        let settings = shared.borrow().settings.clone();
        let intervals = std::array::from_fn(|i| {
            field(settings.reminders[i].interval_minutes.to_string(), w, cx)
        });
        let minutes = std::array::from_fn(|i| {
            field(
                (settings.reminders[i].break_seconds / 60).to_string(),
                w,
                cx,
            )
        });
        let seconds = std::array::from_fn(|i| {
            field(
                (settings.reminders[i].break_seconds % 60).to_string(),
                w,
                cx,
            )
        });
        let periods = settings
            .periods
            .iter()
            .map(|p| PeriodFields::new(*p, w, cx))
            .collect();
        Self {
            shared,
            tx,
            page: if settings.onboarded { 0 } else { 1 },
            form: settings,
            intervals,
            minutes,
            seconds,
            periods,
            error: String::new(),
            confirm_clear: false,
            pending_page: None,
            save_destination: None,
        }
    }
    pub fn shows_countdown(&self) -> bool {
        self.page == 0
    }
    pub fn saved(&mut self, cx: &mut Context<Self>) {
        self.page = self.save_destination.take().unwrap_or(0);
        self.pending_page = None;
        self.error.clear();
        cx.notify();
    }
    fn reset_form(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.form = self.shared.borrow().settings.clone();
        for i in 0..ReminderKind::COUNT {
            self.intervals[i].update(cx, |s, cx| {
                s.set_value(self.form.reminders[i].interval_minutes.to_string(), w, cx)
            });
            self.minutes[i].update(cx, |s, cx| {
                s.set_value(
                    (self.form.reminders[i].break_seconds / 60).to_string(),
                    w,
                    cx,
                )
            });
            self.seconds[i].update(cx, |s, cx| {
                s.set_value(
                    (self.form.reminders[i].break_seconds % 60).to_string(),
                    w,
                    cx,
                )
            });
        }
        self.periods = self
            .form
            .periods
            .iter()
            .map(|p| PeriodFields::new(*p, w, cx))
            .collect();
        self.error.clear();
        self.pending_page = None;
        self.save_destination = None;
    }
    fn reminder_values(&self, i: usize, cx: &App) -> Result<(u32, u32), String> {
        if self.form.reminders[i].mode == ReminderMode::Gentle {
            let seconds = self.form.reminders[i].break_seconds;
            return parse_reminder(
                self.intervals[i].read(cx).value().as_ref(),
                &(seconds / 60).to_string(),
                &(seconds % 60).to_string(),
            );
        }
        parse_reminder(
            self.intervals[i].read(cx).value().as_ref(),
            self.minutes[i].read(cx).value().as_ref(),
            self.seconds[i].read(cx).value().as_ref(),
        )
    }
    fn read_settings(&self, cx: &App) -> Result<Settings, String> {
        let mut s = self.form.clone();
        for (i, r) in s.reminders.iter_mut().enumerate() {
            let (interval, duration) = self
                .reminder_values(i, cx)
                .map_err(|e| format!("{}：{e}", ReminderKind::ALL[i].title()))?;
            r.interval_minutes = interval;
            r.break_seconds = duration;
        }
        s.periods = self.periods.iter().map(|p| p.value(cx)).collect();
        s.normalized().map_err(|e| e.to_string())
    }
    fn dirty(&self, cx: &App) -> bool {
        self.read_settings(cx)
            .map_or(true, |s| s != self.shared.borrow().settings)
    }
    pub fn navigate(&mut self, page: usize, w: &mut Window, cx: &mut Context<Self>) {
        if self.shared.borrow().saving {
            return;
        }
        if self.page == 1 && page != 1 && self.dirty(cx) {
            self.pending_page = Some(page);
        } else {
            if page == 1 && self.page != 1 {
                self.reset_form(w, cx);
            }
            self.page = page;
        }
        let _ = self.tx.try_send(Event::Tick);
        cx.notify();
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        match self.read_settings(cx) {
            Ok(mut s) => {
                s.onboarded = true;
                self.error.clear();
                self.save_destination = self.pending_page.take();
                let _ = self.tx.try_send(Event::Command(Command::Save(s)));
            }
            Err(e) => {
                self.error = e;
                self.pending_page = None;
                self.save_destination = None;
            }
        }
        cx.notify();
    }
    fn home(&self, narrow: bool) -> Div {
        let s = self.shared.borrow();
        let complete = s.records.iter().filter(|r| r.outcome.completed()).count();
        let sessions = s
            .records
            .iter()
            .filter(|r| r.outcome == Outcome::Timed)
            .filter_map(|r| r.session_id)
            .collect::<HashSet<_>>()
            .len();
        let mut content = div().v_flex().gap_4();
        if s.pending.is_empty() {
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .p_4()
                    .rounded(px(16.))
                    .bg(rgb(REST))
                    .child(
                        div()
                            .v_flex()
                            .gap_2()
                            .child(heading("工作很重要，你也是。"))
                            .child(caption(format!(
                                "今日休息 {sessions} 场 · 完成 {complete} 项小事"
                            ))),
                    )
                    .child(mascot(72.)),
            );
        } else {
            let mut pending = card()
                .border_color(rgb(0xd9bf8b))
                .child(heading("留一点时间给自己"));
            for item in &s.pending {
                let id = item.id;
                let tx = self.tx.clone();
                pending = pending.child(
                    div()
                        .id(id.to_string())
                        .v_flex()
                        .gap_2()
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .items_center()
                                .child(icon(KIND_ICONS[item.kind.index()]))
                                .child(heading(item.kind.title())),
                        )
                        .child(caption(item.kind.hint()))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    button(
                                        format!("done-{id}"),
                                        "已完成",
                                        Command::Respond(id, Response::Complete),
                                        &self.tx,
                                    )
                                    .primary()
                                    .accessibility_label(format!("{}，已完成", item.kind.title())),
                                )
                                .child(
                                    Button::new(format!("later-{id}"))
                                        .label("稍后")
                                        .icon(icon("chevron"))
                                        .accessibility_label(format!(
                                            "{}，稍后提醒",
                                            item.kind.title()
                                        ))
                                        .dropdown_menu(move |menu, _, _| {
                                            menu.item(menu_item(
                                                "5 分钟后",
                                                Command::Respond(id, Response::Snooze(5)),
                                                &tx,
                                            ))
                                            .item(menu_item(
                                                "10 分钟后",
                                                Command::Respond(id, Response::Snooze(10)),
                                                &tx,
                                            ))
                                            .item(
                                                menu_item(
                                                    "15 分钟后",
                                                    Command::Respond(id, Response::Snooze(15)),
                                                    &tx,
                                                ),
                                            )
                                        }),
                                )
                                .child(
                                    button(
                                        format!("skip-{id}"),
                                        "跳过",
                                        Command::Respond(id, Response::Skip),
                                        &self.tx,
                                    )
                                    .ghost()
                                    .accessibility_label(format!("{}，跳过", item.kind.title())),
                                ),
                        ),
                );
            }
            content = content.child(pending);
        }
        let mut cards = div().grid().grid_cols(if narrow { 1 } else { 2 }).gap_3();
        for kind in ReminderKind::ALL {
            let i = kind.index();
            let remaining = &s.remaining[i];
            let clock = remaining
                .strip_prefix("还需 ")
                .and_then(|r| r.strip_suffix(" 有效时间"));
            let enabled = s.settings.reminders[i].enabled;
            cards = cards.child(
                card()
                    .id(("reminder-card", i))
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size_8()
                                    .rounded_lg()
                                    .bg(rgb(CATEGORY_COLORS[i]))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon(KIND_ICONS[i])),
                            )
                            .child(heading(kind.title()))
                            .child(badge(
                                if s.settings.reminders[i].mode == ReminderMode::Strong {
                                    "全屏"
                                } else {
                                    "轻提醒"
                                },
                            )),
                    )
                    .child(
                        div()
                            .id("remaining-time")
                            .role(Role::Label)
                            .aria_label(remaining.clone())
                            .text_color(rgb(if enabled { GREEN } else { MUTED }))
                            .child(if let Some(t) = clock {
                                numeric(t.to_string(), 28.)
                            } else {
                                div().py_2().text_lg().child(remaining.clone())
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(caption(if clock.is_some() {
                                "剩余有效时间"
                            } else if !enabled {
                                "可在设置中开启"
                            } else {
                                "照顾一下自己吧"
                            }))
                            .child(
                                button(
                                    format!("manual-{i}"),
                                    "我已完成",
                                    Command::Manual(kind, s.rounds[i]),
                                    &self.tx,
                                )
                                .small()
                                .outline()
                                .disabled(s.break_active || !s.settings.onboarded),
                            ),
                    ),
            );
        }
        content
            .child(cards)
            .child(caption("锁屏、睡眠或休息时，提醒间隔会暂停。"))
    }
    fn settings(&self, _narrow: bool, w: &Window, cx: &mut Context<Self>) -> Div {
        let s = self.shared.borrow();
        let mut content = div().v_flex().gap_5();
        if !s.settings.onboarded {
            content = content.child(
                card()
                    .bg(rgb(REST))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(mascot(68.))
                            .child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .child(heading("给自己建一座小小休息站"))
                                    .child(caption("咕嘟一口，扭扭两下，放风三分钟。")),
                            ),
                    )
                    .child(caption(
                        "护眼默认全屏 20 秒，其余用轻提醒。每项都可调整，确认开始后才计时。",
                    )),
            );
        }
        content = content.child(
            card().child(section("后台常驻", if cfg!(target_os = "macos") {
                "小站常驻右上角菜单栏，不显示在程序坞。关闭窗口后继续提醒，从菜单栏图标或再次打开应用即可返回。"
            } else {
                "关闭窗口后，小站在系统托盘继续提醒。图标可能位于右下角 ∧ 展开区，左键打开小站，右键打开菜单。"
            }))
            .child(caption("确认开始后窗口会收起；需要停止所有提醒时，请从图标菜单选择退出。")),
        );
        let ui_tx = self.tx.clone();
        content = content.child(
            card()
                .child(section(
                    "桌面悬浮窗",
                    "关闭后，到期仍会显示独立提醒条。需要停止提醒时，请使用暂停。",
                ))
                .child(
                    Switch::new("floating-enabled")
                        .label("显示桌面悬浮窗")
                        .checked(s.ui.floating_enabled)
                        .disabled(s.ui_saving || s.recovery)
                        .on_click(move |checked, _, _| {
                            let _ = ui_tx.try_send(Event::Command(Command::SetFloating(*checked)));
                        }),
                )
                .child(
                    button(
                        "floating-reset",
                        "重置悬浮窗位置",
                        Command::ResetFloating,
                        &self.tx,
                    )
                    .ghost()
                    .disabled(s.ui_saving || s.recovery),
                )
                .when(s.ui_saving, |d| d.child(caption("正在保存悬浮窗设置…")))
                .when(!s.ui_error.is_empty(), |d| {
                    d.child(error_text(s.ui_error.clone()))
                }),
        );
        let mut reminders = card().child(section(
            "五件小事",
            "每项独立选择提醒方式；轻提醒勾选完成，全屏休息按时结束。",
        ));
        for kind in ReminderKind::ALL {
            let i = kind.index();
            let config = &self.form.reminders[i];
            let disabled = !config.enabled || s.saving;
            let mut row = div()
                .id(("reminder-form", i))
                .v_flex()
                .gap_3()
                .py_3()
                .border_t_1()
                .border_color(rgb(LINE))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_wrap()
                        .gap_3()
                        .child(icon(KIND_ICONS[i]))
                        .child(
                            Switch::new(("enabled", i))
                                .checked(config.enabled)
                                .label(kind.title())
                                .disabled(s.saving)
                                .on_click(cx.listener(move |this, checked, _, cx| {
                                    this.form.reminders[i].enabled = *checked;
                                    cx.notify();
                                })),
                        )
                        .child(div().flex_1())
                        .children(
                            [
                                (ReminderMode::Gentle, "轻提醒"),
                                (ReminderMode::Strong, "全屏休息"),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (mode, label))| {
                                Button::new(("item-mode", i * 2 + index))
                                    .label(label)
                                    .small()
                                    .outline()
                                    .accessibility_label(format!("{}，{}", kind.title(), label))
                                    .selected(config.mode == mode)
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.form.reminders[i].mode = mode;
                                        cx.notify();
                                    }))
                            }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_end()
                        .gap_3()
                        .child(
                            div()
                                .v_flex()
                                .gap_1()
                                .w(px(94.))
                                .child(caption("间隔 · 分钟"))
                                .child(
                                    numeric_input(&self.intervals[i], cx)
                                        .aria_label(format!("{}间隔分钟", kind.title()))
                                        .disabled(disabled),
                                ),
                        )
                        .when(config.mode == ReminderMode::Strong, |d| {
                            d.child(
                                div()
                                    .v_flex()
                                    .gap_1()
                                    .child(caption("全屏休息 · 分 / 秒"))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(caption("休息").flex_shrink_0())
                                            .child(
                                                div().w(px(62.)).child(
                                                    numeric_input(&self.minutes[i], cx)
                                                        .aria_label(format!(
                                                            "{}休息分钟",
                                                            kind.title()
                                                        ))
                                                        .disabled(disabled),
                                                ),
                                            )
                                            .child(caption("分"))
                                            .child(
                                                div().w(px(62.)).child(
                                                    numeric_input(&self.seconds[i], cx)
                                                        .aria_label(format!(
                                                            "{}休息秒",
                                                            kind.title()
                                                        ))
                                                        .disabled(disabled),
                                                ),
                                            )
                                            .child(caption("秒")),
                                    ),
                            )
                        })
                        .when(config.mode == ReminderMode::Gentle, |d| {
                            d.child(caption("到期显示提醒条，勾选后开始下一轮。"))
                        }),
                );
            let editing = [&self.intervals[i], &self.minutes[i], &self.seconds[i]]
                .iter()
                .any(|input| input.read(cx).focus_handle(cx).is_focused(w));
            if let Err(e) = self.reminder_values(i, cx)
                && !editing
            {
                row = row.child(error_text(e));
            }
            reminders = reminders.child(row);
        }
        content = content.child(reminders.child(caption(
            "间隔 5–240 分钟；休息 5 秒–30 分钟，秒数填写 0–59。",
        )));
        content = content.child(
            card()
                .child(section("全屏休息", "退出设置在保存后对下一场休息生效。"))
                .child(
                    checkbox::Checkbox::new("disable-fullscreen-exit")
                        .label("隐藏退出全屏按钮")
                        .checked(self.form.disable_fullscreen_exit)
                        .disabled(s.saving)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.form.disable_fullscreen_exit = *checked;
                            cx.notify();
                        })),
                )
                .child(caption(
                    "勾选后，全屏休息中不显示“退出”按钮，倒计时结束后自动关闭。",
                )),
        );
        let mut work = card().child(section("工作时间", "工作时间以外，小站安静待命。"));
        let mut days = div().flex().flex_wrap().gap_1();
        for (i, name) in ["一", "二", "三", "四", "五", "六", "日"]
            .into_iter()
            .enumerate()
        {
            days = days.child(
                Button::new(("day", i))
                    .label(name)
                    .accessibility_label(format!("周{name}"))
                    .selected(self.form.weekdays[i])
                    .w(px(36.))
                    .h(px(36.))
                    .disabled(s.saving)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.form.weekdays[i] = !this.form.weekdays[i];
                        cx.notify();
                    })),
            );
        }
        work = work.child(days);
        if !self.form.weekdays.iter().any(|v| *v) {
            work = work.child(error_text("请选择至少一个提醒日。"));
        }
        for (i, fields) in self.periods.iter().enumerate() {
            let value = fields.value(cx);
            let invalid = value.start >= value.end && !fields.editing(w, cx);
            let mut period = div()
                .id(("work-period", i))
                .v_flex()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(caption(format!("时段 {}", i + 1)))
                        .child(
                            div()
                                .id(("start-time", i))
                                .role(Role::Group)
                                .aria_label(format!("时段 {} 开始时间", i + 1))
                                .child(
                                    TimeField::new(&fields.start)
                                        .disabled(s.saving)
                                        .invalid(invalid),
                                ),
                        )
                        .child(caption("至"))
                        .child(if fields.end_of_day {
                            div()
                                .id(("end-of-day-value", i))
                                .child(caption("24:00（当天结束）"))
                        } else {
                            div()
                                .id(("end-time", i))
                                .role(Role::Group)
                                .aria_label(format!("时段 {} 结束时间", i + 1))
                                .child(
                                    TimeField::new(&fields.end)
                                        .disabled(s.saving)
                                        .invalid(invalid),
                                )
                        })
                        .child(
                            Button::new(("remove", i))
                                .ghost()
                                .label("移除")
                                .accessibility_label(format!("移除时段 {}", i + 1))
                                .disabled(self.periods.len() == 1 || s.saving)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.periods.remove(i);
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    checkbox::Checkbox::new(("end-of-day", i))
                        .label("至当天结束")
                        .checked(fields.end_of_day)
                        .disabled(s.saving)
                        .on_click(cx.listener(move |this, checked, _, cx| {
                            this.periods[i].end_of_day = *checked;
                            cx.notify();
                        })),
                );
            if invalid {
                period = period.child(error_text("结束须晚于开始；跨午夜请拆成两段。"));
            }
            work = work.child(period);
        }
        if !self.periods.iter().any(|p| p.editing(w, cx))
            && let Err(e) = self.read_settings(cx)
            && e.contains("重叠")
        {
            work = work.child(error_text(e));
        }
        work = work
            .child(
                Button::new("add-period")
                    .label("＋ 添加时段")
                    .disabled(self.periods.len() >= 4 || s.saving)
                    .on_click(cx.listener(|this, _, w, cx| {
                        this.periods.push(PeriodFields::new(
                            WorkPeriod {
                                start: 19 * 60,
                                end: 20 * 60,
                            },
                            w,
                            cx,
                        ));
                        cx.notify();
                    })),
            )
            .child(caption("24 小时制 HH:MM，最多 4 段，相邻时段会自动合并。"));
        content = content.child(work);
        content.child(
            card()
                .child(section(
                    "体验与通知",
                    "全屏休息默认可退出，可在上方隐藏退出按钮；提醒条不可用时使用系统通知。",
                ))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(button(
                            "preview",
                            "体验 5 秒休息",
                            Command::Preview,
                            &self.tx,
                        ))
                        .child(button("notice", "测试通知", Command::TestNotice, &self.tx))
                        .child(
                            button(
                                "notice-settings",
                                "系统通知设置",
                                Command::NotificationSettings,
                                &self.tx,
                            )
                            .ghost(),
                        ),
                )
                .child(caption(
                    "通知权限状态未知；请通过测试通知确认。体验不计入记录。",
                )),
        )
    }
    fn settings_footer(&self, cx: &mut Context<Self>) -> Div {
        let s = self.shared.borrow();
        let dirty = self.dirty(cx);
        let mut footer = div()
            .flex_shrink_0()
            .v_flex()
            .gap_2()
            .px_5()
            .py_3()
            .bg(rgb(0xffffff))
            .border_t_1()
            .border_color(rgb(LINE));
        if !self.error.is_empty() {
            footer = footer.child(error_text(self.error.clone()));
        }
        if self.pending_page.is_some() {
            return footer
                .child(heading("还有未保存的修改"))
                .child(caption("保存后离开，或放弃本次修改。"))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("keep-editing")
                                .label("继续编辑")
                                .disabled(s.saving)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.pending_page = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("discard")
                                .label("放弃修改")
                                .disabled(s.saving)
                                .on_click(cx.listener(|this, _, w, cx| {
                                    let page = this.pending_page.take().unwrap_or(0);
                                    this.reset_form(w, cx);
                                    this.page = page;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("save-leave")
                                .primary()
                                .label("保存并离开")
                                .disabled(s.saving || s.recovery)
                                .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                        ),
                );
        }
        footer.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(caption(if s.saving {
                    "正在保存，请稍候…"
                } else if dirty {
                    "有未保存的修改"
                } else if !s.settings.onboarded {
                    "确认后开始提醒"
                } else {
                    "设置已保存"
                }))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("cancel")
                                .label("取消修改")
                                .disabled(s.saving)
                                .on_click(cx.listener(|this, _, w, cx| {
                                    this.reset_form(w, cx);
                                    if this.shared.borrow().settings.onboarded {
                                        this.page = 0;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("save")
                                .primary()
                                .label(if s.saving {
                                    "正在保存…"
                                } else if s.settings.onboarded {
                                    "保存设置"
                                } else {
                                    "确认，开始照顾自己"
                                })
                                .disabled(s.saving || s.recovery)
                                .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                        ),
                ),
        )
    }
    fn records(&self, cx: &mut Context<Self>) -> Div {
        let s = self.shared.borrow();
        let completed = s.records.iter().filter(|r| r.outcome.completed()).count();
        let sessions = s
            .records
            .iter()
            .filter(|r| r.outcome == Outcome::Timed)
            .filter_map(|r| r.session_id)
            .collect::<HashSet<_>>()
            .len();
        let mut content = div().v_flex().gap_4().child(caption(format!(
            "今日休息 {sessions} 场 · 事项完成 {completed} 次"
        )));
        if s.records.is_empty() {
            content = content.child(
                card()
                    .items_center()
                    .py_8()
                    .child(mascot(92.))
                    .child(heading("今天的小站刚开门"))
                    .child(caption("每一次停下来，都算照顾自己。")),
            );
        } else {
            let mut rows = card().gap_0().p_0();
            for r in &s.records {
                rows = rows.child(
                    div()
                        .id(r.id.to_string())
                        .v_flex()
                        .gap_1()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .w(px(68.))
                                        .child(caption(r.ended_at.format("%H:%M:%S").to_string())),
                                )
                                .child(icon(KIND_ICONS[r.kind.index()]))
                                .child(div().flex_1().child(r.kind.title()))
                                .child(badge(r.outcome.label())),
                        )
                        .when(!r.reason.is_empty(), |d| d.child(caption(r.reason.clone()))),
                );
            }
            content = content.child(rows);
        }
        content = content.child(caption(
            "计时结束计为完成，不检测身体动作。记录仅保存在本机，保留 30 天。",
        ));
        if self.confirm_clear {
            content = content.child(
                card()
                    .border_color(rgb(0xd8b0a5))
                    .child(heading("清空全部历史记录？"))
                    .child(caption("此操作不能撤销，提醒设置会保留。"))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(Button::new("keep-records").label("保留记录").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.confirm_clear = false;
                                    cx.notify();
                                }),
                            ))
                            .child(
                                Button::new("confirm-clear")
                                    .danger()
                                    .label("确定清空")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let _ = this.tx.try_send(Event::Command(Command::Clear));
                                        this.confirm_clear = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        } else {
            content = content.child(
                Button::new("clear-records")
                    .ghost()
                    .label("清空全部记录…")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirm_clear = true;
                        cx.notify();
                    })),
            );
        }
        content
    }
    fn about(&self) -> Div {
        card()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(img("gulu/app.png").size(px(88.)))
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(heading("咕嘟扭扭放风站"))
                            .child(caption("GuluTwist Break Station"))
                            .child(caption("0.1.0 · 个人预览版")),
                    ),
            )
            .child(
                div()
                    .text_lg()
                    .text_color(rgb(GREEN))
                    .child("咕嘟一口，扭扭两下，放风三分钟。"),
            )
            .child(caption(
                "离线使用，免登录。没有云同步、行为追踪或健康评分。",
            ))
            .child(caption(
                "关闭窗口后，小站继续在菜单栏 / 系统托盘值班。退出应用才会停止提醒。",
            ))
            .child(caption(
                "轻轻活动到舒适的位置；如厕按个人需要。全屏休息默认可退出，也可在设置中隐藏退出按钮。",
            ))
            .child(button("quit", "退出咕嘟扭扭", Command::Quit, &self.tx).outline())
    }
}
impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = window.viewport_size().width < px(760.);
        let mut nav = div().v_flex().gap_2().flex_1();
        for (i, name) in NAV.iter().enumerate() {
            if i == 3 {
                nav = nav.child(div().flex_1());
            }
            nav = nav.child(
                Button::new(("nav", i))
                    .ghost()
                    .selected(self.page == i)
                    .icon(icon(NAV_ICONS[i]))
                    .accessibility_label(*name)
                    .tooltip(*name)
                    .w_full()
                    .h(px(42.))
                    .when(!narrow, |b| b.label(*name))
                    .on_click(cx.listener(move |this, _, w, cx| this.navigate(i, w, cx))),
            );
        }
        let content = match self.page {
            0 => self.home(narrow),
            1 => self.settings(narrow, window, cx),
            2 => self.records(cx),
            _ => self.about(),
        };
        let footer = if self.page == 1 {
            Some(self.settings_footer(cx))
        } else {
            None
        };
        let s = self.shared.borrow();
        let tx = self.tx.clone();
        let mut body = div()
            .id(("content-scroll", self.page))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(if narrow { px(16.) } else { px(24.) })
            .v_flex()
            .gap_3();
        if !s.message.is_empty() {
            body = body.child(
                div()
                    .id("station-message")
                    .role(Role::Label)
                    .aria_label(s.message.clone())
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(REST))
                    .text_sm()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().child(s.message.clone()))
                    .child(
                        Button::new("dismiss-message")
                            .ghost()
                            .small()
                            .label("×")
                            .accessibility_label("关闭提示")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.shared.borrow_mut().message.clear();
                                cx.notify();
                            })),
                    ),
            );
        }
        if !s.tray_error.is_empty() {
            body = body.child(
                card()
                    .border_color(rgb(DANGER))
                    .child(heading("请保留小站窗口"))
                    .child(error_text(s.tray_error.clone()))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(button(
                                "retry-tray",
                                "重试常驻图标",
                                Command::RetryTray,
                                &self.tx,
                            ))
                            .child(button(
                                "quit-without-tray",
                                "退出应用",
                                Command::Quit,
                                &self.tx,
                            )),
                    ),
            );
        }
        if !s.storage_error.is_empty() {
            body = body.child(
                card()
                    .border_color(rgb(DANGER))
                    .child(error_text(s.storage_error.clone())),
            );
        }
        if !s.ui_error.is_empty() && self.page != 1 {
            body = body.child(
                card()
                    .border_color(rgb(DANGER))
                    .child(error_text(s.ui_error.clone())),
            );
        }
        if s.recovery {
            body = body.child(
                card()
                    .child(heading("数据需要恢复"))
                    .child(caption("自动提醒尚未启动，原文件会另存备份。"))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                button(
                                    "recover-backup",
                                    "恢复上一份设置",
                                    Command::Recover(true),
                                    &self.tx,
                                )
                                .disabled(!s.backup),
                            )
                            .child(button(
                                "recover-default",
                                "备份并使用默认设置",
                                Command::Recover(false),
                                &self.tx,
                            )),
                    ),
            );
        }
        body = body.child(content);
        let paused = !matches!(s.pause, Pause::None);
        let header = div()
            .flex_shrink_0()
            .v_flex()
            .gap_2()
            .px(if narrow { px(16.) } else { px(24.) })
            .pt_5()
            .pb_4()
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(24.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(NAV[self.page]),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("mode-settings")
                                    .ghost()
                                    .small()
                                    .label("按事项提醒")
                                    .tooltip("调整提醒方式")
                                    .on_click(
                                        cx.listener(|this, _, w, cx| this.navigate(1, w, cx)),
                                    ),
                            )
                            .when(paused, |d| {
                                d.child(
                                    button("resume", "恢复提醒", Command::Resume, &self.tx)
                                        .primary(),
                                )
                            })
                            .when(!paused, |d| {
                                d.child(
                                    Button::new("pause-menu")
                                        .label("暂停提醒")
                                        .icon(icon("pause"))
                                        .disabled(!s.settings.onboarded)
                                        .dropdown_menu(move |menu, _, _| {
                                            menu.item(menu_item(
                                                "暂停 15 分钟",
                                                Command::PauseMinutes(15),
                                                &tx,
                                            ))
                                            .item(menu_item(
                                                "暂停 1 小时",
                                                Command::PauseMinutes(60),
                                                &tx,
                                            ))
                                            .item(menu_item(
                                                "下一工作日再提醒",
                                                Command::NextDay,
                                                &tx,
                                            ))
                                            .item(menu_item("手动暂停", Command::Pause, &tx))
                                        }),
                                )
                            }),
                    ),
            )
            .child(caption(s.status.clone()));
        div()
            .size_full()
            .flex()
            .bg(rgb(PAPER))
            .text_color(rgb(INK))
            .text_size(px(14.))
            .line_height(px(22.))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(if narrow { 64. } else { 148. }))
                    .v_flex()
                    .items_center()
                    .gap_5()
                    .px_2()
                    .py_5()
                    .bg(rgb(0xf0f3eb))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .v_flex()
                            .items_center()
                            .gap_1()
                            .child(mascot(if narrow { 40. } else { 64. }))
                            .when(!narrow, |d| {
                                d.child(div().font_weight(FontWeight::SEMIBOLD).child("咕嘟扭扭"))
                                    .child(caption("桌面休息小基地"))
                            }),
                    )
                    .child(nav.w_full()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .v_flex()
                    .child(header)
                    .child(body)
                    .children(footer),
            )
    }
}

pub struct Overlay {
    title: String,
    hints: String,
    rest: crate::RestPresentation,
    tx: Sender<Event>,
    focus: FocusHandle,
}
impl Overlay {
    pub fn new(
        title: String,
        hints: String,
        rest: crate::RestPresentation,
        tx: Sender<Event>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(w, cx);
        Self {
            title,
            hints,
            rest,
            tx,
            focus,
        }
    }
}
impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let seconds = self
            .rest
            .deadline
            .saturating_sub(platform::clock().continuous_ms)
            .div_ceil(1000);
        let compact = window.viewport_size().height < px(650.);
        let preview = self.rest.preview;
        let mut tags = div().flex().flex_wrap().justify_center().gap_2();
        if !preview {
            for kind in ReminderKind::ALL {
                if self.title.contains(kind.title()) {
                    tags = tags.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1()
                            .bg(rgb(0xf7faf4))
                            .rounded_full()
                            .child(icon(KIND_ICONS[kind.index()]))
                            .child(kind.title()),
                    );
                }
            }
        }
        div()
            .id("rest-overlay")
            .role(Role::Group)
            .aria_label(format!(
                "{}。{}",
                self.title,
                if self.rest.disable_exit {
                    "退出按钮已隐藏，休息结束后自动关闭。"
                } else {
                    "休息中，可点击底部的“退出”结束。"
                }
            ))
            .w(window.viewport_size().width)
            .h(window.viewport_size().height)
            .overflow_hidden()
            .v_flex()
            .items_center()
            .p_6()
            .bg(rgb(REST))
            .text_color(rgb(INK))
            .track_focus(&self.focus)
            .child(
                div()
                    .id("rest-content")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .min_h_full()
                            .v_flex()
                            .items_center()
                            .justify_center()
                            .gap(if compact { px(8.) } else { px(16.) })
                            .py_4()
                            .child(mascot(if compact { 64. } else { 96. }))
                            .child(
                                div()
                                    .text_size(if compact { px(26.) } else { px(36.) })
                                    .line_height(px(48.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(if preview {
                                        "体验一下休息"
                                    } else {
                                        "一起放个风吧"
                                    }),
                            )
                            .child(tags)
                            .child(
                                div()
                                    .id("rest-countdown")
                                    .role(Role::Label)
                                    .aria_label(format!("剩余 {}", duration(seconds)))
                                    .px_4()
                                    .py_2()
                                    .child(numeric(
                                        duration(seconds),
                                        if compact { 56. } else { 80. },
                                    )),
                            )
                            .child(
                                div()
                                    .max_w(px(600.))
                                    .v_flex()
                                    .items_center()
                                    .gap_2()
                                    .children(self.hints.lines().enumerate().map(|(i, s)| {
                                        div()
                                            .id(("hint", i))
                                            .text_center()
                                            .child(caption(s.to_string()))
                                    })),
                            ),
                    ),
            )
            .when(!self.rest.disable_exit, |overlay| {
                overlay.child(
                    div()
                        .flex_shrink_0()
                        .v_flex()
                        .items_center()
                        .pt_3()
                        .pb_2()
                        .child(
                            Button::new("emergency")
                                .label("退出")
                                .accessibility_label("退出全屏休息")
                                .ghost()
                                .small()
                                .text_color(rgb(MUTED))
                                .underline()
                                .on_click(cx.listener(|this, _, _, _| {
                                    let _ = this
                                        .tx
                                        .try_send(Event::Command(Command::Interrupt(this.rest.id)));
                                })),
                        ),
                )
            })
    }
}
fn parse_reminder(interval: &str, minutes: &str, seconds: &str) -> Result<(u32, u32), String> {
    let interval = interval
        .trim()
        .parse::<u32>()
        .map_err(|_| "间隔请输入整数（分钟）")?;
    if !(5..=240).contains(&interval) {
        return Err("间隔须为 5–240 分钟".into());
    }
    let minutes = minutes
        .trim()
        .parse::<u32>()
        .map_err(|_| "休息分钟请输入非负整数")?;
    let seconds = seconds
        .trim()
        .parse::<u32>()
        .map_err(|_| "休息秒数请输入 0–59 的整数")?;
    if seconds > 59 {
        return Err("秒数须为 0–59，请将整分钟填入左侧".into());
    }
    let total = minutes
        .checked_mul(60)
        .and_then(|m| m.checked_add(seconds))
        .ok_or("休息时长过大")?;
    if !(5..=1800).contains(&total) {
        return Err("休息时长须为 5 秒–30 分钟".into());
    }
    Ok((interval, total))
}
#[cfg(test)]
mod tests {
    use super::{digits_only, minute_time, parse_reminder, time_minute};
    #[test]
    fn numeric_edits_allow_empty_but_reject_entire_mixed_pastes() {
        for valid in ["", "0", "005", "240"] {
            assert!(digits_only(valid));
        }
        for invalid in ["1.5", "-5", "20分钟", "1\n5", "1\r\n5", "２０", " 5", "+5"] {
            assert!(!digits_only(invalid));
        }
    }
    #[test]
    fn split_duration_preserves_boundaries_and_values() {
        for seconds in [5, 30, 59, 60, 61, 120, 180, 1799, 1800] {
            assert_eq!(
                parse_reminder(
                    "60",
                    &(seconds / 60).to_string(),
                    &(seconds % 60).to_string()
                ),
                Ok((60, seconds))
            );
        }
    }
    #[test]
    fn invalid_duration_never_rolls_over_or_overflows() {
        for (minutes, seconds) in [
            ("0", "4"),
            ("0", "60"),
            ("30", "1"),
            ("-1", "0"),
            ("1.5", "0"),
            ("", "5"),
            ("4294967295", "0"),
        ] {
            assert!(parse_reminder("60", minutes, seconds).is_err());
        }
        for interval in ["4", "241", "-1", "abc"] {
            assert!(parse_reminder(interval, "1", "0").is_err());
        }
    }
    #[test]
    fn work_time_roundtrips_every_minute() {
        for minute in 0..1440 {
            assert_eq!(time_minute(minute_time(minute)), minute);
        }
    }
}
