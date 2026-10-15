use crate::{Command, Controller, Event, floating::*, persistence::Job, platform};
use gpui_kit::*;
use std::{rc::Rc, time::Instant};

impl Controller {
    pub(crate) fn save_ui_if_ready(&mut self, force: bool) {
        if !force && self.position_due.is_some_and(|due| due > Instant::now()) {
            return;
        }
        let request = if self.quitting {
            self.preferences.final_save()
        } else {
            self.preferences.begin_save()
        };
        if let Some((revision, prefs)) = request {
            self.position_due = None;
            if self.jobs.send(Job::SaveUi(revision, prefs)).is_err() {
                self.preferences.finish_save(revision, false);
                self.snapshot.borrow_mut().ui_error = "悬浮窗设置未保存：存储线程不可用".into();
            }
        }
    }
    pub(crate) fn set_floating(&mut self, enabled: bool, cx: &mut App) {
        if self.snapshot.borrow().recovery {
            return;
        }
        if self.preferences.set_enabled(enabled) {
            self.floating_failed = false;
            self.snapshot.borrow_mut().ui_error.clear();
            if !enabled {
                self.hide_floating(cx);
            }
            self.save_ui_if_ready(true);
        }
    }
    pub(crate) fn close_floating_menu(&mut self, cx: &mut App) {
        if let Some(menu) = self.floating_menu.take() {
            let _ = menu.update(cx, |_, w, _| w.remove_window());
        }
    }
    pub(crate) fn hide_floating(&mut self, cx: &mut App) {
        self.close_floating_list(cx);
        self.close_floating_menu(cx);
        if self.floating_visible
            && let Some((handle, _)) = &self.floating_window
        {
            let _ = handle.update(cx, |_, w, _| platform::hide(w));
        }
        self.floating_visible = false;
    }
    pub(crate) fn floating_display(&self, cx: &App) -> Option<Rc<dyn PlatformDisplay>> {
        let saved = self
            .preferences
            .current
            .floating_position
            .as_ref()
            .and_then(|p| p.display_uuid.as_deref());
        cx.displays()
            .into_iter()
            .find(|d| {
                saved.is_some()
                    && d.uuid()
                        .ok()
                        .is_some_and(|id| Some(id.to_string()).as_deref() == saved)
            })
            .or_else(|| cx.primary_display())
    }
    pub(crate) fn floating_bounds(&self, display: &dyn PlatformDisplay) -> Bounds<Pixels> {
        let position = self
            .preferences
            .current
            .floating_position
            .as_ref()
            .filter(|p| {
                p.display_uuid.is_none()
                    || display.uuid().ok().is_some_and(|id| {
                        p.display_uuid.as_deref() == Some(id.to_string().as_str())
                    })
            });
        placed_bounds(
            display.visible_bounds(),
            position,
            size(px(WIDTH), px(HEIGHT)),
        )
    }
    pub(crate) fn reposition_floating(&mut self, cx: &mut App) {
        if let Some(display) = self.floating_display(cx)
            && let Some((handle, _)) = &self.floating_window
        {
            let bounds = self.floating_bounds(display.as_ref());
            let result = handle.update(cx, |_, w, cx| {
                if w.bounds() != bounds || w.display(cx).is_none_or(|d| d.id() != display.id()) {
                    platform::position_floating(w, bounds, display.as_ref(), cx)
                } else {
                    Ok(())
                }
            });
            if let Err(e) = result.and_then(|r| r) {
                self.snapshot.borrow_mut().ui_error = format!("悬浮窗位置调整失败：{e}");
            }
        }
    }
    pub(crate) fn sync_floating(&mut self, cx: &mut App) {
        if let Some(tray) = &self.tray {
            tray.floating
                .set_text(if self.preferences.current.floating_enabled {
                    "隐藏悬浮窗"
                } else {
                    "显示悬浮窗"
                });
            tray.floating
                .set_enabled(!self.preferences.saving() && !self.snapshot.borrow().recovery);
        }
        let show = self.preferences.current.floating_enabled
            && !self.quitting
            && !self.engine.clock.locked
            && self.engine.settings.onboarded
            && !self.snapshot.borrow().recovery
            && self.engine.session.is_none()
            && self.preview.is_none();
        if !show {
            self.hide_floating(cx);
            return;
        }
        if self.floating_failed {
            return;
        }
        if self.floating_window.is_none() {
            let result = (|| -> anyhow::Result<()> {
                let display = self
                    .floating_display(cx)
                    .ok_or_else(|| anyhow::anyhow!("没有显示器"))?;
                let bounds = self.floating_bounds(display.as_ref());
                let shared = self.snapshot.clone();
                let tx = self.tx.clone();
                let close = tx.clone();
                let handle = cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        window_min_size: Some(size(px(WIDTH), px(HEIGHT))),
                        display_id: Some(display.id()),
                        titlebar: None,
                        focus: false,
                        show: false,
                        is_resizable: false,
                        is_minimizable: false,
                        window_background: WindowBackgroundAppearance::Transparent,
                        ..Default::default()
                    },
                    move |w, cx| {
                        w.set_window_title("咕嘟扭扭 · 悬浮窗");
                        w.on_window_should_close(cx, move |_, _| {
                            let _ = close.try_send(Event::Command(Command::SetFloating(false)));
                            false
                        });
                        cx.new(|cx| FloatingView::new(shared, tx, w, cx))
                    },
                )?;
                let view = handle.entity(cx)?;
                self.floating_window = Some((handle.into(), view));
                handle.update(cx, |_, w, cx| {
                    platform::prepare_floating(w, false)?;
                    platform::position_floating(w, bounds, display.as_ref(), cx)
                })??;
                Ok(())
            })();
            if let Err(e) = result {
                if let Some((handle, _)) = self.floating_window.take() {
                    let _ = handle.update(cx, |_, w, _| w.remove_window());
                }
                self.floating_failed = true;
                self.snapshot.borrow_mut().ui_error =
                    format!("悬浮窗未能打开，请关闭开关后重试：{e}");
                self.open_main(cx);
                return;
            }
        }
        if !self.floating_visible {
            self.reposition_floating(cx);
            if let Some((handle, _)) = &self.floating_window {
                let _ = handle.update(cx, |_, w, _| platform::show_floating(w));
            }
            self.floating_visible = true;
        }
        if let Some((_, view)) = &self.floating_window {
            view.update(cx, |_, cx| cx.notify());
        }
        if let Some(menu) = self.floating_menu {
            let _ = menu.update(cx, |_, w, _| w.refresh());
        }
    }
    pub(crate) fn open_floating_menu(&mut self, cx: &mut App) {
        self.close_floating_list(cx);
        if self.floating_menu.is_some() {
            self.close_floating_menu(cx);
            return;
        }
        if !self.floating_visible {
            return;
        }
        let Some((handle, _)) = &self.floating_window else {
            return;
        };
        let Ok(Some((anchor, display))) =
            handle.update(cx, |_, w, cx| w.display(cx).map(|d| (w.bounds(), d)))
        else {
            return;
        };
        let work = display.visible_bounds();
        let below = anchor.origin.y + anchor.size.height + px(8.);
        let y = if below + px(MENU_HEIGHT) > work.origin.y + work.size.height {
            anchor.origin.y - px(MENU_HEIGHT) - px(8.)
        } else {
            below
        };
        let position = gulu_storage::FloatingPosition {
            display_uuid: None,
            x: f32::from(anchor.origin.x + anchor.size.width - px(MENU_WIDTH) - work.origin.x),
            y: f32::from(y - work.origin.y),
        };
        let bounds = placed_bounds(work, Some(&position), size(px(MENU_WIDTH), px(MENU_HEIGHT)));
        let shared = self.snapshot.clone();
        let tx = self.tx.clone();
        let close = tx.clone();
        let result = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                display_id: Some(display.id()),
                window_min_size: Some(bounds.size),
                titlebar: None,
                focus: true,
                show: false,
                is_resizable: false,
                is_minimizable: false,
                is_movable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            move |w, cx| {
                w.set_window_title("咕嘟扭扭 · 悬浮窗菜单");
                w.on_window_should_close(cx, move |_, _| {
                    let _ = close.try_send(Event::Command(Command::CloseFloatingMenu));
                    false
                });
                cx.new(|cx| MenuView::new(shared, tx, w, cx))
            },
        );
        match result {
            Ok(handle) => {
                self.floating_menu = Some(handle.into());
                let prepared = handle.update(cx, |_, w, cx| {
                    platform::prepare_floating(w, true)?;
                    platform::position_floating(w, bounds, display.as_ref(), cx)?;
                    platform::activate_floating_menu(w);
                    Ok::<(), anyhow::Error>(())
                });
                if let Err(e) = prepared.and_then(|r| r) {
                    self.close_floating_menu(cx);
                    self.snapshot.borrow_mut().ui_error = format!("悬浮窗菜单未能打开：{e}");
                    self.open_main(cx);
                }
            }
            Err(e) => {
                self.snapshot.borrow_mut().ui_error = format!("悬浮窗菜单未能打开：{e}");
                self.open_main(cx);
            }
        }
    }
}
