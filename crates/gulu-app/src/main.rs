#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod artwork;
mod floating;
mod floating_controller;
mod persistence;
mod platform;
mod preferences;
mod tray;
mod ui;
use async_channel::Sender;
use chrono::{Duration as ChronoDuration, Local};
use fs2::FileExt;
use gpui_kit::*;
use gulu_core::*;
use persistence::Job;
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::mpsc, time::Duration};
use uuid::Uuid;
gpui_kit::actions!(station, [QuitApplication, ShowStation, CloseStationWindow]);
#[derive(Clone, Debug)]
pub enum Command {
    OpenHome,
    OpenSettings,
    SetFloating(bool),
    ToggleFloating,
    ResetFloating,
    FloatingMenu,
    CloseFloatingMenu,
    Open,
    CloseWindow,
    Hide,
    PauseMinutes(i64),
    NextDay,
    Pause,
    Resume,
    Interrupt,
    Quit,
    Manual(ReminderKind),
    Respond(Uuid, Response),
    Save(Settings),
    Preview,
    TestNotice,
    NotificationSettings,
    Clear,
    Recover(bool),
}
pub enum Event {
    UiSaved(u64, Result<(), String>),
    FloatingMoved(gulu_storage::FloatingPosition),
    Tick,
    Displays,
    Command(Command),
    Saved(Result<Settings, String>),
    Cleared(Result<(), String>),
    Recovered(Result<Settings, String>),
    Persisted,
    Records(chrono::NaiveDate, Result<Vec<ActivityRecord>, String>),
    StorageError(String),
    Error(String),
    Flushed,
    Notice(String),
}
pub struct Snapshot {
    pub ui: gulu_storage::UiPreferences,
    pub ui_saving: bool,
    pub ui_error: String,
    pub floating: floating::DisplayState,
    pub settings: Settings,
    pub pause: Pause,
    pub status: String,
    pub remaining: [String; 4],
    pub pending: Vec<Instance>,
    pub records: Vec<ActivityRecord>,
    pub message: String,
    pub saving: bool,
    pub recovery: bool,
    pub backup: bool,
    pub break_active: bool,
    pub storage_error: String,
}
struct Controller {
    preferences: preferences::Preferences,
    floating_window: Option<(AnyWindowHandle, Entity<floating::FloatingView>)>,
    floating_menu: Option<AnyWindowHandle>,
    floating_visible: bool,
    floating_failed: bool,
    position_due: Option<std::time::Instant>,
    engine: Engine,
    snapshot: Rc<RefCell<Snapshot>>,
    tx: Sender<Event>,
    jobs: mpsc::Sender<Job>,
    main: Option<(AnyWindowHandle, Entity<ui::MainView>)>,
    overlays: Vec<(AnyWindowHandle, Entity<ui::Overlay>)>,
    preview: Option<u64>,
    tray: Option<tray::Tray>,
    quitting: bool,
    close_explained: bool,
    _lock: std::fs::File,
    activation: PathBuf,
    last_day: chrono::NaiveDate,
    _watcher: notify::RecommendedWatcher,
    main_visible: bool,
    tray_status: String,
    previous_focus: Option<platform::FocusTarget>,
    notice_tags: std::collections::HashMap<Uuid, String>,
}
impl Controller {
    fn open_main(&mut self, cx: &mut App) {
        self.main_visible = true;
        if let Some((win, _)) = &self.main
            && win.update(cx, |_, w, _| w.activate_window()).is_ok()
        {
            cx.activate(true);
            return;
        }
        let shared = self.snapshot.clone();
        let tx = self.tx.clone();
        let close_tx = tx.clone();
        let result = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(860.), px(700.)),
                    cx,
                ))),
                window_min_size: Some(size(px(640.), px(480.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("咕嘟扭扭放风站".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            cx,
            move |w, cx| {
                #[cfg(target_os = "windows")]
                platform::set_window_icon(w);
                w.on_window_should_close(cx, move |_, _| {
                    let _ = close_tx.try_send(Event::Command(Command::Hide));
                    false
                });
                cx.new(|cx| ui::MainView::new(shared, tx, w, cx))
            },
        );
        match result {
            Ok(pair) => {
                self.main = Some(pair);
                cx.activate(true);
            }
            Err(e) => eprintln!("打开小站失败：{e}"),
        }
    }
    fn close_overlays(&mut self, cx: &mut App) {
        for (w, _) in self.overlays.drain(..) {
            let _ = w.update(cx, |_, w, _| w.remove_window());
        }
        if let Some(focus) = self.previous_focus.take() {
            platform::restore_focus(focus);
        }
    }
    fn open_overlays(
        &mut self,
        title: String,
        hints: String,
        deadline: u64,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        self.hide_floating(cx);
        self.close_overlays(cx);
        self.previous_focus = platform::capture_focus();
        let displays = cx.displays();
        if displays.is_empty() {
            anyhow::bail!("没有可用显示器");
        }
        let mut pointer_window = None;
        for display in displays {
            let tx = self.tx.clone();
            let t = title.clone();
            let h = hints.clone();
            let close_tx = tx.clone();
            let (handle, view) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(display.bounds())),
                    display_id: Some(display.id()),
                    titlebar: None,
                    kind: WindowKind::Normal,
                    is_resizable: false,
                    is_minimizable: false,
                    is_movable: false,
                    focus: false,
                    ..Default::default()
                },
                cx,
                move |w, cx| {
                    w.set_window_title("咕嘟扭扭 · 休息中");
                    w.on_window_should_close(cx, move |_, _| {
                        let _ = close_tx.try_send(Event::Command(Command::Interrupt));
                        false
                    });
                    cx.new(|cx| ui::Overlay::new(t, h, deadline, tx, w, cx))
                },
            )?;
            self.overlays.push((handle, view));
            handle.update(cx, |_, w, _| {
                platform::prepare_overlay(w)?;
                if platform::contains_pointer(w) {
                    pointer_window = Some(handle);
                }
                Ok::<(), anyhow::Error>(())
            })??;
        }
        cx.activate(true);
        if let Some(handle) = pointer_window.or_else(|| self.overlays.first().map(|p| p.0)) {
            let _ = handle.update(cx, |_, w, _| w.activate_window());
        }
        if std::env::var_os("GULU_DIAGNOSTICS").is_some() {
            eprintln!(
                "overlay_count={} deadline_ms={deadline}",
                self.overlays.len()
            );
        }
        Ok(())
    }
    fn apply(&mut self, effects: Vec<Effect>, cx: &mut App) {
        let persist = !effects.is_empty();
        let mut extra = Vec::new();
        if std::env::var_os("GULU_DIAGNOSTICS").is_some() && !effects.is_empty() {
            eprintln!("{} effects={effects:?}", Local::now().to_rfc3339());
        }
        for effect in &effects {
            match effect {
                Effect::CloseBreak => self.close_overlays(cx),
                Effect::OpenBreak(s) => {
                    self.preview = None;
                    let title = s
                        .items
                        .iter()
                        .map(|i| i.kind.title())
                        .collect::<Vec<_>>()
                        .join(" · ");
                    let hints = s
                        .items
                        .iter()
                        .map(|i| i.kind.hint())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Err(e) = self.open_overlays(title, hints, s.deadline_ms, cx) {
                        self.close_overlays(cx);
                        self.snapshot.borrow_mut().message =
                            format!("遮罩未能完整启动，本次改为轻提醒：{e}");
                        extra = self.engine.overlay_failed(s.id, platform::clock());
                    }
                }
                Effect::Notify(items) => {
                    let ids = items
                        .iter()
                        .map(|i| i.id.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    for item in items {
                        self.notice_tags.insert(item.id, ids.clone());
                    }
                    cx.show_system_notification(SystemNotification {
                        tag: ids.into(),
                        title: "咕嘟扭扭 · 该照顾自己啦".into(),
                        body: items
                            .iter()
                            .map(|i| i.kind.title())
                            .collect::<Vec<_>>()
                            .join(" · ")
                            .into(),
                        actions: vec![],
                    });
                }
                Effect::Record(record) => {
                    self.snapshot.borrow_mut().records.insert(0, record.clone());
                    if let Some(tag) = self.notice_tags.remove(&record.instance_id) {
                        cx.dismiss_system_notification(&tag);
                        self.notice_tags.retain(|_, v| *v != tag);
                    }
                }
            }
        }
        if persist {
            let _ = self.jobs.send(Job::Commit(
                effects,
                self.engine.session.clone(),
                self.engine.pause.clone(),
            ));
        }
        if !extra.is_empty() {
            self.apply(extra, cx);
        }
    }
    fn event(&mut self, event: Event, cx: &mut App) {
        let now = platform::clock();
        let event = match event {
            Event::Command(Command::CloseWindow) => Event::Command(
                if self
                    .floating_menu
                    .is_some_and(|w| Some(w) == cx.active_window())
                {
                    Command::CloseFloatingMenu
                } else if self
                    .floating_window
                    .as_ref()
                    .is_some_and(|(w, _)| Some(*w) == cx.active_window())
                {
                    Command::SetFloating(false)
                } else if self.preview.is_some() || self.engine.session.is_some() {
                    Command::Interrupt
                } else {
                    Command::Hide
                },
            ),
            event => event,
        };
        let old_clock = self.engine.clock;
        let awake_elapsed = now.awake_ms.saturating_sub(old_clock.awake_ms);
        let continuous_elapsed = now.continuous_ms.saturating_sub(old_clock.continuous_ms);
        let clock_transition = old_clock.locked != now.locked
            || continuous_elapsed > awake_elapsed.saturating_add(2000);
        let diagnostics = std::env::var_os("GULU_DIAGNOSTICS").is_some();
        if diagnostics {
            if let Event::Command(command) = &event {
                eprintln!("{} command={command:?}", now.wall.to_rfc3339());
            }
            if clock_transition {
                eprintln!(
                    "{} clock_transition locked={}->{} awake_delta_ms={awake_elapsed} continuous_delta_ms={continuous_elapsed} remaining_before_ms={:?}",
                    now.wall.to_rfc3339(),
                    old_clock.locked,
                    now.locked,
                    self.engine.reminders.each_ref().map(|r| r.remaining_ms)
                );
            }
        }
        if matches!(
            &event,
            Event::Command(
                Command::Interrupt
                    | Command::Quit
                    | Command::Pause
                    | Command::Resume
                    | Command::PauseMinutes(_)
                    | Command::NextDay
            )
        ) && self.preview.is_some()
        {
            let effects = self.engine.set_preview(false, now);
            self.apply(effects, cx);
        }
        match event {
            Event::FloatingMoved(position) => {
                if self.preferences.set_position(Some(position)) {
                    self.position_due =
                        Some(std::time::Instant::now() + Duration::from_millis(300));
                }
            }
            Event::UiSaved(revision, result) => {
                self.preferences.finish_save(revision, result.is_ok());
                self.snapshot.borrow_mut().ui_error = result
                    .err()
                    .map(|e| format!("悬浮窗设置未保存：{e}。开关已恢复，当前位置仅本次有效。"))
                    .unwrap_or_default();
                self.save_ui_if_ready(false);
                if !self.snapshot.borrow().ui_error.is_empty() {
                    self.open_main(cx);
                }
            }
            Event::Command(c) => match c {
                Command::OpenHome | Command::OpenSettings => {
                    self.open_main(cx);
                    let page = if matches!(c, Command::OpenSettings) {
                        1
                    } else {
                        0
                    };
                    if let Some((handle, view)) = &self.main {
                        let view = view.clone();
                        let _ = handle.update(cx, |_, w, cx| {
                            view.update(cx, |v, cx| v.navigate(page, w, cx))
                        });
                    }
                }
                Command::SetFloating(enabled) => self.set_floating(enabled, cx),
                Command::ToggleFloating => {
                    self.set_floating(!self.preferences.current.floating_enabled, cx)
                }
                Command::ResetFloating => {
                    if !self.preferences.saving() {
                        self.preferences.set_position(None);
                        self.position_due = None;
                        self.reposition_floating(cx);
                        self.save_ui_if_ready(true);
                    }
                }
                Command::FloatingMenu => self.open_floating_menu(cx),
                Command::CloseFloatingMenu => self.close_floating_menu(cx),
                Command::CloseWindow => unreachable!("close resolved before dispatch"),
                Command::Open => self.open_main(cx),
                Command::Hide => {
                    if !self.close_explained {
                        self.close_explained = true;
                        self.snapshot.borrow_mut().message="小站会继续在菜单栏 / 系统托盘运行。需要结束时请选择「退出咕嘟扭扭」。再次关闭即可隐藏。".into();
                    } else if self.tray.is_some() {
                        self.main_visible = false;
                        if let Some((w, _)) = &self.main {
                            let _ = w.update(cx, |_, w, _| platform::hide(w));
                        }
                    }
                }
                Command::PauseMinutes(m) => {
                    let effects = self
                        .engine
                        .set_pause(Pause::Until(now.wall + ChronoDuration::minutes(m)), now);
                    self.preview = None;
                    self.close_overlays(cx);
                    self.apply(effects, cx);
                    self.persist_pause();
                }
                Command::NextDay => {
                    let effects = self.engine.set_pause(
                        Pause::Until({
                            use chrono::TimeZone;
                            let next = self.engine.settings.next_start(now.wall, true);
                            Local
                                .from_local_datetime(&next.naive_local())
                                .earliest()
                                .map(|t| t.fixed_offset())
                                .unwrap_or(next)
                        }),
                        now,
                    );
                    self.preview = None;
                    self.close_overlays(cx);
                    self.apply(effects, cx);
                    self.persist_pause();
                }
                Command::Pause | Command::Resume => {
                    let pause = if matches!(c, Command::Pause) {
                        Pause::Manual
                    } else {
                        Pause::None
                    };
                    let effects = self.engine.set_pause(pause, now);
                    self.preview = None;
                    self.close_overlays(cx);
                    self.apply(effects, cx);
                    self.persist_pause();
                }
                Command::Interrupt => {
                    let was_preview = self.preview.take().is_some();
                    let was_break = self.engine.session.is_some();
                    self.close_overlays(cx);
                    let effects = self.engine.interrupt(now);
                    self.apply(effects, cx);
                    if was_preview {
                        self.snapshot.borrow_mut().message = "体验已结束，没有新增记录。".into();
                    } else if was_break {
                        self.snapshot.borrow_mut().message =
                            "已结束本次休息，下次提醒时间已更新。".into();
                    }
                }
                Command::Quit => {
                    self.quitting = true;
                    self.hide_floating(cx);
                    self.save_ui_if_ready(true);
                    self.preview = None;
                    self.close_overlays(cx);
                    let effects = self.engine.shutdown(now);
                    self.apply(effects, cx);
                    self.persist_pause();
                    let _ = self.jobs.send(Job::Flush);
                    let tx = self.tx.clone();
                    cx.spawn(async move |cx| {
                        cx.background_executor().timer(Duration::from_secs(2)).await;
                        let _ = tx.try_send(Event::Flushed);
                    })
                    .detach();
                }
                Command::Manual(kind) => {
                    let effects = self.engine.manual_complete(kind, now);
                    self.apply(effects, cx);
                }
                Command::Respond(id, r) => {
                    if !self.engine.pending().iter().any(|i| i.id == id) {
                        self.snapshot.borrow_mut().message = "这条提醒已结束。".into();
                    }
                    let effects = self.engine.respond(id, r, now);
                    self.apply(effects, cx);
                }
                Command::Save(settings) => {
                    if !self.snapshot.borrow().saving {
                        self.snapshot.borrow_mut().saving = true;
                        let _ = self.jobs.send(Job::Save(settings));
                    }
                }
                Command::Preview => {
                    if self.engine.session.is_none() && self.preview.is_none() {
                        let deadline = now.continuous_ms + 5000;
                        let effects = self.engine.set_preview(true, now);
                        self.apply(effects, cx);
                        self.preview = Some(deadline);
                        if let Err(e) = self.open_overlays(
                            "体验一下休息".into(),
                            "5 秒体验，不计入记录。".into(),
                            deadline,
                            cx,
                        ) {
                            self.close_overlays(cx);
                            self.preview = None;
                            let effects = self.engine.set_preview(false, now);
                            self.apply(effects, cx);
                            self.snapshot.borrow_mut().message = format!("测试遮罩未能启动：{e}");
                        }
                    }
                }
                Command::TestNotice => {
                    cx.show_system_notification(SystemNotification {
                        tag: "test".into(),
                        title: "咕嘟扭扭 · 测试提醒".into(),
                        body: "看见这条消息，说明这次系统通知可以显示。".into(),
                        actions: vec![],
                    });
                    self.snapshot.borrow_mut().message="已发出测试通知。是否显示取决于系统授权和专注模式；未显示时请打开通知设置。".into();
                }
                Command::NotificationSettings => platform::notification_settings(),
                Command::Clear => {
                    let _ = self.jobs.send(Job::Clear);
                }
                Command::Recover(backup) => {
                    let _ = self.jobs.send(Job::Recover(backup));
                }
            },
            Event::Saved(result) => {
                self.snapshot.borrow_mut().saving = false;
                match result {
                    Ok(settings) => match self.engine.replace_settings(settings, now) {
                        Ok(e) => {
                            self.apply(e, cx);
                            self.snapshot.borrow_mut().message =
                                "设置已保存。新的规则已生效。".into();
                            if let Some((_, v)) = &self.main {
                                v.update(cx, |v, cx| v.saved(cx));
                            }
                        }
                        Err(e) => self.snapshot.borrow_mut().message = e.to_string(),
                    },
                    Err(e) => {
                        self.snapshot.borrow_mut().message =
                            format!("未保存，当前设置没有改变：{e}")
                    }
                }
            }
            Event::Recovered(result) => match result {
                Ok(settings) => {
                    self.engine = Engine::new(settings, now, Pause::None).unwrap();
                    self.preferences = preferences::Preferences::new(Default::default());
                    self.position_due = None;
                    let mut s = self.snapshot.borrow_mut();
                    s.recovery = false;
                    s.ui_error.clear();
                    s.records.clear();
                    s.message = "已恢复。原数据已另存到数据目录。".into();
                }
                Err(e) => self.snapshot.borrow_mut().message = format!("恢复失败：{e}"),
            },
            Event::Cleared(result) => match result {
                Ok(()) => {
                    self.snapshot.borrow_mut().records.clear();
                    self.snapshot.borrow_mut().message = "所有记录已清空，设置已保留。".into();
                }
                Err(e) => self.snapshot.borrow_mut().message = format!("记录没有清空：{e}"),
            },
            Event::Records(day, result) => {
                if day == now.wall.date_naive() {
                    match result {
                        Ok(records) => {
                            let mut s = self.snapshot.borrow_mut();
                            let ids = s
                                .records
                                .iter()
                                .map(|r| r.id)
                                .collect::<std::collections::HashSet<_>>();
                            s.records
                                .extend(records.into_iter().filter(|r| !ids.contains(&r.id)));
                            s.records.sort_by_key(|r| std::cmp::Reverse(r.ended_at));
                        }
                        Err(e) => self.snapshot.borrow_mut().storage_error = e,
                    }
                }
            }
            Event::StorageError(e) => self.snapshot.borrow_mut().storage_error = e,
            Event::Persisted => self.snapshot.borrow_mut().storage_error.clear(),
            Event::Error(e) => {
                let effects = self.engine.set_pause(Pause::Manual, now);
                self.apply(effects, cx);
                self.snapshot.borrow_mut().message = e;
            }
            Event::Flushed => {
                if self.quitting {
                    cx.quit();
                    return;
                }
            }
            Event::Notice(ids) => {
                self.open_main(cx);
                if let Some((_, v)) = &self.main {
                    v.update(cx, |v, cx| v.saved(cx));
                }
                if !self
                    .engine
                    .pending()
                    .iter()
                    .any(|i| ids.contains(&i.id.to_string()))
                {
                    self.snapshot.borrow_mut().message = "这条提醒已结束。".into();
                }
            }
            Event::Displays => {
                self.close_floating_menu(cx);
                self.reposition_floating(cx);
                if let Some(s) = self.engine.session.clone() {
                    if let Err(e) = self.open_overlays(
                        s.items
                            .iter()
                            .map(|i| i.kind.title())
                            .collect::<Vec<_>>()
                            .join(" · "),
                        "给自己一点休息时间。".into(),
                        s.deadline_ms,
                        cx,
                    ) {
                        self.close_overlays(cx);
                        self.snapshot.borrow_mut().message =
                            format!("显示器变化，本次改为轻提醒：{e}");
                        let effects = self.engine.overlay_failed(s.id, now);
                        self.apply(effects, cx);
                    }
                } else if self.preview.is_some() {
                    let effects = self.engine.set_preview(false, now);
                    self.apply(effects, cx);
                    self.preview = None;
                    self.close_overlays(cx);
                }
            }
            Event::Tick => {}
        }
        if self.preview.is_some_and(|d| now.continuous_ms >= d) {
            self.preview = None;
            let effects = self.engine.set_preview(false, now);
            self.apply(effects, cx);
            self.close_overlays(cx);
        }
        if !self.quitting && !self.snapshot.borrow().recovery {
            let effects = self.engine.tick(now);
            self.apply(effects, cx);
        }
        if self.activation.exists() {
            let _ = std::fs::remove_file(&self.activation);
            self.open_main(cx);
        }
        self.refresh(cx);
        if diagnostics && clock_transition {
            eprintln!(
                "{} clock_reconciled remaining_after_ms={:?} session_deadline_ms={:?} overlays={}",
                now.wall.to_rfc3339(),
                self.engine.reminders.each_ref().map(|r| r.remaining_ms),
                self.engine.session.as_ref().map(|s| s.deadline_ms),
                self.overlays.len()
            );
        }
    }
    fn persist_pause(&self) {
        let _ = self.jobs.send(Job::Commit(
            Vec::new(),
            self.engine.session.clone(),
            self.engine.pause.clone(),
        ));
    }
    fn refresh(&mut self, cx: &mut App) {
        if self
            .position_due
            .is_some_and(|due| due <= std::time::Instant::now())
        {
            self.reposition_floating(cx);
        }
        self.save_ui_if_ready(false);
        if let Some((window, _)) = &self.main {
            self.main_visible = window
                .update(cx, |_, w, _| platform::is_visible(w))
                .unwrap_or(false);
        }
        let now = self.engine.clock;
        let mut s = self.snapshot.borrow_mut();
        s.ui = self.preferences.current.clone();
        s.ui_saving = self.preferences.saving();
        s.floating = floating::DisplayState::from_engine(
            &self.engine,
            !s.storage_error.is_empty() || !s.ui_error.is_empty(),
        );
        s.settings = self.engine.settings.clone();
        s.pause = self.engine.pause.clone();
        s.pending = self.engine.pending();
        s.break_active = self.engine.session.is_some();
        s.records
            .retain(|r| r.local_date == now.wall.date_naive().to_string());
        if self.last_day != now.wall.date_naive() {
            self.last_day = now.wall.date_naive();
            let _ = self.jobs.send(Job::ReadDay(self.last_day));
            let _ = self.jobs.send(Job::Commit(
                Vec::new(),
                self.engine.session.clone(),
                self.engine.pause.clone(),
            ));
        }
        s.status = if s.recovery {
            "数据需要恢复".into()
        } else if !s.settings.onboarded {
            "欢迎来到你的小站".into()
        } else if !s.settings.reminders.iter().any(|r| r.enabled) {
            "所有提醒已关闭".into()
        } else if let Some(rest) = &self.engine.session {
            format!(
                "正在休息 · {}",
                ui::duration(rest.remaining_seconds(now.continuous_ms))
            )
        } else if now.locked {
            "电脑已锁屏，间隔暂停".into()
        } else if !matches!(s.pause, Pause::None) {
            match &s.pause {
                Pause::Until(t) => format!("已暂停至 {}", t.format("%m-%d %H:%M")),
                _ => "已暂停，等待手动恢复".into(),
            }
        } else if self.engine.settings.work_window(now.wall).is_none() {
            format!(
                "现在休息中 · 下次开始 {}",
                s.settings.next_start(now.wall, false).format("%m-%d %H:%M")
            )
        } else {
            "小站值班中 · 记得照顾自己".into()
        };
        for kind in ReminderKind::ALL {
            let i = kind.index();
            s.remaining[i] = if !s.settings.reminders[i].enabled {
                "已关闭".into()
            } else {
                match &self.engine.reminders[i].phase {
                    ReminderPhase::Waiting => format!(
                        "还需 {} 有效时间",
                        ui::duration(self.engine.reminders[i].remaining_ms.div_ceil(1000))
                    ),
                    ReminderPhase::Pending { notified, .. } => if *notified {
                        "已发出提醒"
                    } else {
                        "待提醒"
                    }
                    .into(),
                    ReminderPhase::Snoozed { remaining_ms, .. } => {
                        format!("稍后 · {}", ui::duration(remaining_ms.div_ceil(1000)))
                    }
                    ReminderPhase::Breaking => "正在休息".into(),
                }
            };
        }
        if let Some(tray) = &self.tray
            && self.tray_status != s.status
        {
            tray.status.set_text(&s.status);
            self.tray_status = s.status.clone();
        }
        drop(s);
        self.sync_floating(cx);
        if self.main_visible
            && let Some((_, v)) = &self.main
        {
            v.update(cx, |_, cx| cx.notify());
        }
        for (_, v) in &self.overlays {
            v.update(cx, |_, cx| cx.notify());
        }
    }
    fn wait_ms(&self, cx: &App) -> u64 {
        if !self.preferences.saving()
            && let Some(due) = self.position_due
        {
            return self
                .engine
                .next_wake_ms()
                .min(
                    due.saturating_duration_since(std::time::Instant::now())
                        .as_millis()
                        .max(1) as u64,
                )
                .min(1000);
        }
        if self.quitting {
            1000
        } else if !self.overlays.is_empty()
            || (self.floating_visible && self.snapshot.borrow().floating.ticking)
            || (self.main_visible
                && self.engine.running()
                && self
                    .main
                    .as_ref()
                    .is_some_and(|(_, v)| v.read(cx).shows_countdown()))
        {
            self.engine.next_wake_ms().min(1000)
        } else {
            self.engine.next_wake_ms().min(30_000)
        }
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        platform::startup_error(&format!("{error:#}"));
    }
}
fn run() -> anyhow::Result<()> {
    let data = std::env::var_os("GULU_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            directories::ProjectDirs::from("app", "GuluTwist", "BreakStation")
                .expect("data directory")
                .data_local_dir()
                .to_path_buf()
        });
    std::fs::create_dir_all(&data)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(data.join("instance.lock"))?;
    let activation = data.join("activate");
    match lock.try_lock_exclusive() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            std::fs::write(activation, b"open")?;
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    }
    let (tx, rx) = async_channel::unbounded();
    let activation_tx = tx.clone();
    let activation_watch = activation.clone();
    use notify::Watcher;
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
            && event.paths.iter().any(|p| p == &activation_watch)
            && (event.kind.is_create() || event.kind.is_modify())
        {
            let _ = activation_tx.try_send(Event::Command(Command::Open));
        }
    })?;
    watcher.watch(&data, notify::RecursiveMode::NonRecursive)?;
    let (jobs, loaded) = persistence::start(data.join("station.sqlite3"), tx.clone());
    let engine = Engine::new(
        loaded.settings.clone(),
        platform::clock(),
        loaded.pause.clone(),
    )?;
    let snapshot = Rc::new(RefCell::new(Snapshot {
        ui: loaded.ui.clone(),
        ui_saving: false,
        ui_error: loaded.ui_warning,
        floating: Default::default(),
        settings: loaded.settings,
        pause: loaded.pause,
        status: String::new(),
        remaining: Default::default(),
        pending: vec![],
        records: loaded.records,
        message: loaded.error.clone().unwrap_or_default(),
        recovery: loaded.error.is_some(),
        backup: loaded.backup,
        saving: false,
        break_active: false,
        storage_error: String::new(),
    }));
    let app = application()
        .with_assets(artwork::Artwork)
        .with_quit_mode(QuitMode::Explicit);
    let reopen = tx.clone();
    app.on_reopen(move |_| {
        let _ = reopen.try_send(Event::Command(Command::Open));
    });
    app.run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Light, None, cx);
        gpui_kit::component::Theme::update(cx, |theme| {
            theme.colors.primary = rgb(0x2f6b58).into();
            theme.colors.primary_hover = rgb(0x245544).into();
            theme.radius = px(8.);
            theme.radius_lg = px(16.);
            theme.colors.button_primary = rgb(0x2f6b58).into();
            theme.colors.button_primary_hover = rgb(0x245544).into();
            theme.colors.button_primary_active = rgb(0x1d4638).into();
            theme.colors.button_primary_foreground = rgb(0xffffff).into();
            theme.colors.primary_foreground = rgb(0xffffff).into();
            theme.colors.foreground = rgb(0x243d35).into();
            theme.colors.muted_foreground = rgb(0x607068).into();
            theme.colors.accent = rgb(0xeaf1e9).into();
            theme.colors.accent_foreground = rgb(0x243d35).into();
            theme.colors.border = rgb(0xdde5dc).into();
            theme.colors.input = rgb(0xc4d2c5).into();
            theme.colors.ring = rgb(0x2f6b58).into();
            theme.colors.button_danger = rgb(0xa34237).into();
            theme.colors.button_danger_hover = rgb(0x8e392f).into();
            theme.colors.button_danger_active = rgb(0x792f27).into();
            theme.colors.button_danger_foreground = rgb(0xffffff).into();
            theme.colors.secondary_active = rgb(0xdce8dc).into();
            theme.colors.secondary_hover = rgb(0xe5eee3).into();
            theme.colors.button_active = rgb(0xdce8dc).into();
            theme.colors.button_hover = rgb(0xeff4ec).into();
            theme.colors.button_foreground = rgb(0x243d35).into();
        });
        let quit = tx.clone();
        cx.on_action(move |_: &QuitApplication, _| {
            let _ = quit.try_send(Event::Command(Command::Quit));
        });
        let show = tx.clone();
        cx.on_action(move |_: &ShowStation, _| {
            let _ = show.try_send(Event::Command(Command::Open));
        });
        let close = tx.clone();
        cx.on_action(move |_: &CloseStationWindow, _| {
            let _ = close.try_send(Event::Command(Command::CloseWindow));
        });
        if cfg!(target_os = "macos") {
            cx.bind_keys([KeyBinding::new("cmd-w", CloseStationWindow, None)]);
        }
        cx.bind_keys([KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-q"
            } else {
                "ctrl-q"
            },
            QuitApplication,
            None,
        )]);
        cx.set_menus([Menu::new("咕嘟扭扭").items([
            MenuItem::action("打开小站", ShowStation),
            MenuItem::action("关闭窗口", CloseStationWindow),
            MenuItem::separator(),
            MenuItem::action("退出咕嘟扭扭", QuitApplication),
        ])]);
        cx.set_app_identity("app.gulutwist.breakstation", "咕嘟扭扭放风站");
        platform::init(tx.clone());
        let notify = tx.clone();
        cx.on_system_notification_response(move |r, _| {
            let _ = notify.try_send(Event::Notice(r.tag.to_string()));
        });
        let wake = tx.clone();
        cx.on_system_wake(move |_| {
            let _ = wake.try_send(Event::Tick);
        })
        .detach();
        let sleep = tx.clone();
        cx.on_system_sleep(move |_| {
            let _ = sleep.try_send(Event::Tick);
        })
        .detach();
        let displays = tx.clone();
        cx.observe_displays(move |_, _| {
            let _ = displays.try_send(Event::Displays);
        })
        .detach();
        let tray = tray::Tray::new(tx.clone())
            .map_err(|e| snapshot.borrow_mut().message = format!("托盘不可用，请保留小站窗口：{e}"))
            .ok();
        let mut c = Controller {
            preferences: preferences::Preferences::new(loaded.ui),
            floating_window: None,
            floating_menu: None,
            floating_visible: false,
            floating_failed: false,
            position_due: None,
            engine,
            snapshot,
            tx,
            jobs,
            main: None,
            overlays: vec![],
            preview: None,
            tray,
            quitting: false,
            close_explained: false,
            _lock: lock,
            activation,
            last_day: Local::now().date_naive(),
            _watcher: watcher,
            main_visible: true,
            tray_status: String::new(),
            previous_focus: None,
            notice_tags: Default::default(),
        };
        c.refresh(cx);
        if !c.floating_visible
            || c.tray.is_none()
            || !c.snapshot.borrow().storage_error.is_empty()
            || !c.snapshot.borrow().ui_error.is_empty()
        {
            c.open_main(cx);
        }
        let c = Rc::new(RefCell::new(c));
        let quitting = c.clone();
        cx.on_app_quit(move |cx| {
            let (done, rx) = async_channel::bounded(1);
            {
                let mut c = quitting.borrow_mut();
                c.hide_floating(cx);
                c.close_overlays(cx);
                if !c.quitting {
                    c.quitting = true;
                    let effects = c.engine.shutdown(platform::clock());
                    c.apply(effects, cx);
                    c.persist_pause();
                }
                c.save_ui_if_ready(true);
                let _ = c.jobs.send(Job::Barrier(done));
            }
            let timer = cx.background_executor().timer(Duration::from_secs(2));
            async move {
                futures_lite::future::or(
                    async {
                        let _ = rx.recv().await;
                    },
                    async {
                        timer.await;
                    },
                )
                .await;
            }
        })
        .detach();
        cx.spawn(async move |cx| {
            loop {
                let wait = cx.update(|cx| c.borrow().wait_ms(cx));
                let timer = cx.background_executor().timer(Duration::from_millis(wait));
                let event = futures_lite::future::or(
                    async { rx.recv().await.unwrap_or(Event::Command(Command::Quit)) },
                    async {
                        timer.await;
                        Event::Tick
                    },
                )
                .await;
                cx.update(|cx| c.borrow_mut().event(event, cx));
            }
        })
        .detach();
    });
    Ok(())
}
