use crate::{Command, Controller, Event, floating, platform, ui};
use async_channel::Sender;
use gpui_kit::{component::button::*, *};
use gulu_core::*;

const ROW_HEIGHT: f32 = 40.;
const GAP: f32 = 6.;

/// Place the whole stack before placing individual windows, so rows never overlap.
fn row_bounds(
    work: Bounds<Pixels>,
    anchor: Bounds<Pixels>,
    attached: bool,
    count: usize,
) -> Vec<Bounds<Pixels>> {
    if count == 0 {
        return vec![];
    }
    let total = (ROW_HEIGHT + GAP) * count as f32 - GAP;
    let below = anchor.origin.y + anchor.size.height + px(GAP);
    let y = if !attached {
        anchor.origin.y
    } else if below + px(total) <= work.origin.y + work.size.height {
        below
    } else {
        anchor.origin.y - px(GAP + total)
    };
    let position = gulu_storage::FloatingPosition {
        display_uuid: None,
        x: (anchor.origin.x - work.origin.x).into(),
        y: (y - work.origin.y).into(),
    };
    let stack =
        floating::placed_bounds(work, Some(&position), size(px(floating::WIDTH), px(total)));
    (0..count)
        .map(|i| {
            Bounds::new(
                stack.origin + point(px(0.), px(i as f32 * (ROW_HEIGHT + GAP))),
                size(px(floating::WIDTH), px(ROW_HEIGHT)),
            )
        })
        .collect()
}

struct ReminderRow {
    item: Instance,
    tx: Sender<Event>,
}
impl ReminderRow {
    fn new(item: Instance, tx: Sender<Event>, w: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_window_appearance(w, |_, _, cx| cx.notify())
            .detach();
        Self { item, tx }
    }
}
impl Render for ReminderRow {
    fn render(&mut self, w: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (paper, _, ink, line) = floating::colors(w);
        let tx = self.tx.clone();
        let id = self.item.id;
        let kind = self.item.kind;
        let title = match kind {
            ReminderKind::Water => "喝口水吧",
            ReminderKind::Stretch => "放松一下颈肩",
            ReminderKind::Walk => "有需要就放个风",
            ReminderKind::Stand => "起来站一站",
            ReminderKind::EyeCare => "让眼睛休息一下",
        };
        div()
            .id("reminder-row")
            .role(Role::Group)
            .aria_label(format!("{}，待完成", kind.title()))
            .font_family(if cfg!(target_os = "macos") {
                "PingFang SC"
            } else {
                "Microsoft YaHei"
            })
            .w(px(floating::WIDTH))
            .h(px(ROW_HEIGHT))
            .rounded(px(ROW_HEIGHT / 2.))
            .bg(rgb(paper))
            .border_1()
            .border_color(rgb(line))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                ui::icon(ui::KIND_ICONS[kind.index()])
                    .text_color(rgb(ink))
                    .size(px(18.)),
            )
            .child(
                div()
                    .flex_1()
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(rgb(ink))
                    .child(title),
            )
            .child(
                Button::new("complete-reminder")
                    .label("✓")
                    .rounded_full()
                    .accessibility_label(format!("完成{}", kind.title()))
                    .tooltip("完成本次提醒")
                    .w(px(28.))
                    .h(px(28.))
                    .p_0()
                    .bg(rgb(line))
                    .text_color(rgb(ink))
                    .on_click(move |_, _, _| {
                        let _ =
                            tx.try_send(Event::Command(Command::Respond(id, Response::Complete)));
                    }),
            )
    }
}

impl Controller {
    pub(crate) fn close_reminder_rows(&mut self, cx: &mut App) {
        for (_, window) in self.reminder_windows.drain() {
            let _ = window.update(cx, |_, w, _| w.remove_window());
        }
    }

    pub(crate) fn sync_reminder_rows(&mut self, cx: &mut App) {
        let mut items: Vec<_> = self
            .engine
            .pending()
            .into_iter()
            .filter(|i| i.gentle)
            .collect();
        items.sort_by_key(|i| (i.created_at, i.kind.index()));
        let ids: std::collections::HashSet<_> = items.iter().map(|i| i.id).collect();
        self.reminder_failed.retain(|id| ids.contains(id));
        if self.quitting
            || self.snapshot.borrow().recovery
            || !self.engine.running()
            || self.floating_menu.is_some()
            || self.floating_list.is_some()
        {
            self.close_reminder_rows(cx);
            return;
        }
        self.reminder_windows.retain(|id, window| {
            if ids.contains(id) {
                return true;
            }
            let _ = window.update(cx, |_, w, _| w.remove_window());
            false
        });
        if items.is_empty() {
            return;
        }
        // When attached, use live bounds so a native drag moves the rows immediately.
        let attached = self.floating_visible;
        let live = self
            .floating_window
            .as_ref()
            .filter(|_| attached)
            .and_then(|(handle, _)| {
                handle
                    .update(cx, |_, w, cx| w.display(cx).map(|d| (w.bounds(), d)))
                    .ok()
                    .flatten()
            });
        let Some((anchor, display)) = live.or_else(|| {
            self.floating_display(cx)
                .map(|d| (self.floating_bounds(d.as_ref()), d))
        }) else {
            for item in items {
                self.reminder_row_failed(&item, "没有可用显示器".into(), cx);
            }
            return;
        };
        let positions = row_bounds(display.visible_bounds(), anchor, attached, items.len());
        for (item, bounds) in items.iter().zip(positions) {
            if self.reminder_failed.contains(&item.id) {
                continue;
            }
            let result = (|| -> anyhow::Result<()> {
                let is_new = !self.reminder_windows.contains_key(&item.id);
                let handle = if let Some(handle) = self.reminder_windows.get(&item.id) {
                    *handle
                } else {
                    let tx = self.tx.clone();
                    let close = tx.clone();
                    let instance = item.clone();
                    let handle = cx.open_window(
                        WindowOptions {
                            window_bounds: Some(WindowBounds::Windowed(bounds)),
                            window_min_size: Some(bounds.size),
                            display_id: Some(display.id()),
                            titlebar: None,
                            focus: false,
                            show: false,
                            is_resizable: false,
                            is_minimizable: false,
                            is_movable: false,
                            window_background: WindowBackgroundAppearance::Transparent,
                            ..Default::default()
                        },
                        move |w, cx| {
                            w.set_window_title(&format!(
                                "咕嘟扭扭 · {}提醒",
                                instance.kind.title()
                            ));
                            // A system close opens the existing full controls; it cannot
                            // silently complete or discard an outstanding reminder.
                            w.on_window_should_close(cx, move |_, _| {
                                let _ = close.try_send(Event::Command(Command::OpenHome));
                                false
                            });
                            cx.new(|cx| ReminderRow::new(instance, tx, w, cx))
                        },
                    )?;
                    let handle: AnyWindowHandle = handle.into();
                    self.reminder_windows.insert(item.id, handle);
                    handle.update(cx, |_, w, _| platform::prepare_floating(w, false))??;
                    handle
                };
                handle.update(cx, |_, w, cx| {
                    if w.bounds() != bounds || w.display(cx).is_none_or(|d| d.id() != display.id())
                    {
                        platform::position_floating(w, bounds, display.as_ref(), cx)?;
                    }
                    if is_new {
                        platform::show_floating(w);
                    }
                    Ok::<(), anyhow::Error>(())
                })??;
                Ok(())
            })();
            if let Err(error) = result {
                self.reminder_row_failed(item, error.to_string(), cx);
            }
        }
    }

    fn reminder_row_failed(&mut self, item: &Instance, error: String, cx: &mut App) {
        if let Some(window) = self.reminder_windows.remove(&item.id) {
            let _ = window.update(cx, |_, w, _| w.remove_window());
        }
        if self.reminder_failed.insert(item.id) {
            self.snapshot.borrow_mut().message =
                format!("提醒条未能显示：{error}。请在小站处理待办。");
            self.notify_fallback(std::slice::from_ref(item), cx);
            self.open_main(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn stack_flips_above_bottom_edge_and_stays_in_negative_origin_work_area() {
        let work = Bounds::new(point(px(-1920.), px(-300.)), size(px(1920.), px(1080.)));
        let anchor = Bounds::new(point(px(-230.), px(700.)), size(px(200.), px(52.)));
        let rows = row_bounds(work, anchor, true, 5);
        assert_eq!(rows.len(), 5);
        assert!(rows[4].origin.y + rows[4].size.height < anchor.origin.y);
        for (i, row) in rows.iter().enumerate() {
            assert!(row.origin.x >= work.origin.x && row.origin.y >= work.origin.y);
            assert!(row.origin.x + row.size.width <= work.origin.x + work.size.width);
            if i > 0 {
                assert!(row.origin.y > rows[i - 1].origin.y + rows[i - 1].size.height);
            }
        }
        let detached = row_bounds(work, anchor, false, 5);
        assert_eq!(
            detached[4].origin.y + detached[4].size.height,
            work.origin.y + work.size.height
        );
        let top = Bounds::new(work.origin + point(px(24.), px(24.)), anchor.size);
        assert_eq!(
            row_bounds(work, top, true, 1)[0].origin.y,
            top.origin.y + px(58.)
        );
    }
}
