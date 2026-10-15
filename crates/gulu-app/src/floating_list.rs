use crate::{Command, Controller, Event, Snapshot, floating, platform, ui};
use async_channel::Sender;
use gpui_kit::{
    component::{ActiveTheme, Disableable, StyledExt, button::*},
    *,
};
use gulu_core::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};
use uuid::Uuid;

const ROW_HEIGHT: f32 = 52.;
const GAP: f32 = 6.;

pub struct ListView {
    shared: Rc<RefCell<Snapshot>>,
    tx: Sender<Event>,
    pub id: Uuid,
    focus: FocusHandle,
    above_anchor: bool,
    order: Vec<ReminderKind>,
    feedback: HashMap<ReminderKind, Instant>,
    pending: Option<ReminderKind>,
    pressed: bool,
    message: String,
}
impl ListView {
    fn new(
        shared: Rc<RefCell<Snapshot>>,
        tx: Sender<Event>,
        id: Uuid,
        above_anchor: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_window_activation(w, |this, w, _| {
            if !w.is_window_active() {
                let _ = this
                    .tx
                    .try_send(Event::Command(Command::CheckFloatingListFocus));
            }
        })
        .detach();
        cx.observe_window_appearance(w, |_, _, cx| cx.notify())
            .detach();
        let focus = cx.focus_handle();
        focus.focus(w, cx);
        Self {
            shared,
            tx,
            id,
            focus,
            above_anchor,
            order: Vec::new(),
            feedback: HashMap::new(),
            pending: None,
            pressed: false,
            message: String::new(),
        }
    }
    pub fn completed(&mut self, kind: ReminderKind, accepted: bool, cx: &mut Context<Self>) {
        self.pending = None;
        if accepted {
            self.feedback
                .insert(kind, Instant::now() + Duration::from_secs(1));
            self.message.clear();
        } else {
            self.message = "状态已更新，请重新选择。".into();
        }
        cx.notify();
    }
    pub fn next_feedback_ms(&self) -> Option<u64> {
        self.feedback
            .values()
            .map(|t| {
                t.saturating_duration_since(Instant::now())
                    .as_millis()
                    .max(1) as u64
            })
            .min()
    }
}
impl Render for ListView {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (paper, muted, green, line) = floating::colors(w);
        let snapshot = self.shared.borrow();
        let mut items = snapshot.active_reminders.clone();
        let storage_error = !snapshot.storage_error.is_empty();
        drop(snapshot);
        // The shortest countdown belongs at the edge nearest the capsule.
        items.sort_by(|a, b| {
            let remaining = if self.above_anchor {
                b.remaining_ms.cmp(&a.remaining_ms)
            } else {
                a.remaining_ms.cmp(&b.remaining_ms)
            };
            remaining.then_with(|| a.kind.index().cmp(&b.kind.index()))
        });
        self.feedback.retain(|kind, until| {
            *until > Instant::now() && items.iter().any(|item| item.kind == *kind)
        });
        if self.feedback.is_empty() && self.pending.is_none() && !self.pressed {
            self.order = items.iter().map(|item| item.kind).collect();
        }
        self.order
            .retain(|kind| items.iter().any(|item| item.kind == *kind));
        for item in &items {
            if !self.order.contains(&item.kind) {
                self.order.push(item.kind);
            }
        }
        let mut content = div()
            .id("activity-list")
            .track_focus(&self.focus)
            .role(Role::Group)
            .aria_label("正在进行的事项，剩余时间越少越靠近悬浮窗")
            .font_family(cx.theme().font_family.clone())
            .size_full()
            .overflow_y_scroll()
            .v_flex()
            .gap(px(GAP))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    let _ = this.tx.try_send(Event::Command(Command::CloseFloatingList));
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.pressed = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.pressed = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.pressed = false;
                    cx.notify();
                }),
            );
        for kind in self.order.clone() {
            let Some(item) = items.iter().find(|item| item.kind == kind) else {
                continue;
            };
            let round = item.round;
            let completed = self.feedback.contains_key(&kind);
            let status = if completed {
                "已完成".into()
            } else if item.state == ActivityState::Pending {
                "待完成".into()
            } else {
                format!(
                    "{}{}",
                    if item.state == ActivityState::Snoozed {
                        "稍后 · "
                    } else {
                        ""
                    },
                    ui::duration(item.remaining_ms.div_ceil(1000))
                )
            };
            content = content.child(div().id(("activity", kind.index()))
                .role(Role::Group).aria_label(kind.title())
                .h(px(ROW_HEIGHT)).flex_shrink_0().w_full().rounded(px(ROW_HEIGHT / 2.))
                .bg(rgb(paper)).border_1().border_color(rgb(line)).px(px(12.))
                .flex().items_center().gap(px(8.))
                .child(ui::icon(ui::KIND_ICONS[kind.index()]).text_color(rgb(green)))
                .child(div().v_flex().flex_1().min_w_0()
                    .child(div().text_size(px(12.)).text_color(rgb(green)).child(kind.title()))
                    .child(div().text_size(px(15.)).line_height(px(20.)).text_color(rgb(muted))
                        .font_family(if cfg!(target_os = "macos") { "Menlo" } else { "Consolas" }).child(status)))
                .child(Button::new(("activity-complete", kind.index())).label("✓")
                    .accessibility_label(format!("我已完成{}", kind.title()))
                    .tooltip(format!("我已完成{}", kind.title()))
                    .rounded_full().w(px(28.)).h(px(28.)).p_0().bg(rgb(line)).text_color(rgb(green))
                    .disabled(completed || self.pending.is_some())
                    .on_click(cx.listener(move |this, event, _, cx| {
                        if matches!(event, ClickEvent::Mouse(mouse) if mouse.up.click_count > 1) { return; }
                        this.pending = Some(kind);
                        let _ = this.tx.try_send(Event::Command(Command::CompleteFromList(this.id, kind, round)));
                        cx.notify();
                    }))));
        }
        let message = if storage_error {
            "记录保存异常，请打开小站查看。"
        } else {
            &self.message
        };
        if !message.is_empty() {
            content = content.child(
                div()
                    .bg(rgb(paper))
                    .p_2()
                    .text_size(px(11.))
                    .text_color(rgb(green))
                    .child(message.to_string()),
            );
        }
        content
    }
}

fn list_bounds(work: Bounds<Pixels>, anchor: Bounds<Pixels>, count: usize) -> Bounds<Pixels> {
    let height = (count as f32 * (ROW_HEIGHT + GAP) - GAP)
        .max(ROW_HEIGHT)
        .min(f32::from(work.size.height));
    let below = anchor.origin.y + anchor.size.height + px(GAP);
    let y = if below + px(height) <= work.origin.y + work.size.height {
        below
    } else {
        anchor.origin.y - px(height + GAP)
    };
    floating::placed_bounds(
        work,
        Some(&gulu_storage::FloatingPosition {
            display_uuid: None,
            x: (anchor.origin.x - work.origin.x).into(),
            y: (y - work.origin.y).into(),
        }),
        size(px(floating::WIDTH), px(height)),
    )
}

impl Controller {
    pub(crate) fn close_floating_list(&mut self, cx: &mut App) {
        if let Some((handle, _)) = self.floating_list.take() {
            let _ = handle.update(cx, |_, w, _| w.remove_window());
        }
        self.snapshot.borrow_mut().list_open = false;
    }
    pub(crate) fn check_floating_list_focus(&mut self, cx: &mut App) {
        let inside = self.floating_window.as_ref().is_some_and(|(h, _)| {
            h.update(cx, |_, w, _| {
                w.is_window_active() || platform::contains_pointer(w)
            })
            .unwrap_or(false)
        }) || self.floating_list.as_ref().is_some_and(|(h, _)| {
            h.update(cx, |_, w, _| {
                w.is_window_active() || platform::contains_pointer(w)
            })
            .unwrap_or(false)
        });
        if !inside {
            self.close_floating_list(cx);
        }
    }
    pub(crate) fn toggle_floating_list(&mut self, cx: &mut App) {
        if self.floating_list.is_some() {
            self.close_floating_list(cx);
            return;
        }
        if !self.floating_visible
            || self.engine.active_reminders().is_empty()
            || self.snapshot.borrow().recovery
        {
            return;
        }
        self.close_floating_menu(cx);
        let result = (|| -> anyhow::Result<()> {
            let (anchor, display) = self
                .floating_window
                .as_ref()
                .unwrap()
                .0
                .update(cx, |_, w, cx| (w.bounds(), w.display(cx)))?;
            let display = display.ok_or_else(|| anyhow::anyhow!("没有可用显示器"))?;
            let bounds = list_bounds(
                display.visible_bounds(),
                anchor,
                self.engine.active_reminders().len(),
            );
            let above_anchor = bounds.origin.y < anchor.origin.y;
            let shared = self.snapshot.clone();
            let tx = self.tx.clone();
            let close = tx.clone();
            let id = Uuid::new_v4();
            let (handle, view) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    display_id: Some(display.id()),
                    titlebar: None,
                    focus: true,
                    show: false,
                    is_resizable: false,
                    is_minimizable: false,
                    is_movable: false,
                    window_background: WindowBackgroundAppearance::Transparent,
                    ..Default::default()
                },
                cx,
                move |w, cx| {
                    w.set_window_title("咕嘟扭扭 · 事项清单");
                    w.on_window_should_close(cx, move |_, _| {
                        let _ = close.try_send(Event::Command(Command::CloseFloatingList));
                        false
                    });
                    cx.new(|cx| ListView::new(shared, tx, id, above_anchor, w, cx))
                },
            )?;
            self.floating_list = Some((handle, view));
            handle.update(cx, |_, w, cx| {
                // The Kit root supplies an opaque theme background even for transparent windows.
                gpui_kit::base::Root::update(w, cx, |root, _, cx| {
                    root.style().background = Some(rgba(0x00000000).into());
                    cx.notify();
                });
                platform::prepare_floating(w, true)?;
                platform::position_floating(w, bounds, display.as_ref(), cx)?;
                platform::activate_floating_menu(w);
                Ok::<(), anyhow::Error>(())
            })??;
            self.snapshot.borrow_mut().list_open = true;
            Ok(())
        })();
        if let Err(error) = result {
            self.close_floating_list(cx);
            self.snapshot.borrow_mut().ui_error = format!("事项清单未能打开：{error}");
            self.open_main(cx);
        }
    }
    pub(crate) fn sync_floating_list(&mut self, cx: &mut App) {
        if !self.floating_visible
            || self.engine.active_reminders().is_empty()
            || self.snapshot.borrow().recovery
        {
            self.close_floating_list(cx);
            return;
        }
        let Some((handle, view)) = self.floating_list.clone() else {
            return;
        };
        if let Some((anchor, _)) = &self.floating_window
            && let Ok((anchor, Some(display))) =
                anchor.update(cx, |_, w, cx| (w.bounds(), w.display(cx)))
        {
            let bounds = list_bounds(
                display.visible_bounds(),
                anchor,
                self.engine.active_reminders().len(),
            );
            let result = handle.update(cx, |_, w, cx| {
                if w.bounds() != bounds {
                    platform::position_floating(w, bounds, display.as_ref(), cx)?;
                }
                Ok::<(), anyhow::Error>(())
            });
            if let Err(error) = result.and_then(|r| r) {
                self.close_floating_list(cx);
                self.snapshot.borrow_mut().ui_error = format!("事项清单位置调整失败：{error}");
                return;
            }
            view.update(cx, |view, _| {
                view.above_anchor = bounds.origin.y < anchor.origin.y;
            });
        }
        view.update(cx, |_, cx| cx.notify());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn list_flips_above_and_stays_in_small_work_area() {
        let work = Bounds::new(point(px(100.), px(50.)), size(px(800.), px(600.)));
        let anchor = Bounds::new(
            point(px(650.), px(590.)),
            size(px(floating::WIDTH), px(52.)),
        );
        let bounds = list_bounds(work, anchor, 5);
        assert!(bounds.origin.y + bounds.size.height < anchor.origin.y);
        assert!(bounds.origin.x + bounds.size.width <= work.origin.x + work.size.width);
        let small = Bounds::new(work.origin, size(px(300.), px(200.)));
        let bounds = list_bounds(small, anchor, 5);
        assert_eq!(bounds.size.height, small.size.height);
        assert_eq!(bounds.origin.y, small.origin.y);
    }
}
