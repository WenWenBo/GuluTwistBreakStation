use crate::{Command, Event, Snapshot};
use async_channel::Sender;
use gpui_kit::{
    component::{Disableable, StyledExt, button::*},
    *,
};
use std::{cell::RefCell, rc::Rc};
pub struct Dialog {
    shared: Rc<RefCell<Snapshot>>,
    tx: Sender<Event>,
    focus: FocusHandle,
}
impl Dialog {
    pub fn new(
        shared: Rc<RefCell<Snapshot>>,
        tx: Sender<Event>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(w, cx);
        Self { shared, tx, focus }
    }
}
impl Render for Dialog {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let s = self.shared.borrow();
        let later = self.tx.clone();
        let restart = self.tx.clone();
        let esc = self.tx.clone();
        div()
            .id("update-dialog")
            .track_focus(&self.focus)
            .role(Role::Dialog)
            .aria_label("新版本已准备好")
            .on_key_down(move |e, _, _| {
                if e.keystroke.key == "escape" {
                    let _ = esc.try_send(Event::Command(Command::UpdateLater));
                }
            })
            .size_full()
            .v_flex()
            .gap_4()
            .p_6()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x24332d))
            .font_family(if cfg!(target_os = "macos") {
                "PingFang SC"
            } else {
                "Microsoft YaHei"
            })
            .child(
                div()
                    .id("update-title")
                    .role(Role::Label)
                    .aria_label("新版本已准备好")
                    .text_xl()
                    .child("新版本已准备好"),
            )
            .child(
                div()
                    .id("update-status")
                    .role(Role::Label)
                    .aria_label(s.update.status.clone())
                    .child(s.update.status.clone()),
            )
            .child(
                div()
                    .id("update-notes")
                    .role(Role::Label)
                    .aria_label(s.update.notes.clone())
                    .flex_1()
                    .overflow_y_scroll()
                    .text_sm()
                    .child(if s.update.notes.is_empty() {
                        "更新已下载。立即重启会保存数据并重新打开小站。".into()
                    } else {
                        s.update.notes.clone()
                    }),
            )
            .child(
                div()
                    .id("update-error")
                    .role(Role::Label)
                    .aria_label(s.update.error.clone())
                    .text_sm()
                    .child(s.update.error.clone()),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .justify_end()
                    .child(
                        Button::new("update-later")
                            .label("稍后")
                            .on_click(move |_, _, _| {
                                let _ = later.try_send(Event::Command(Command::UpdateLater));
                            }),
                    )
                    .child(
                        Button::new("update-restart")
                            .label("立即重启")
                            .primary()
                            .disabled(s.update.busy)
                            .on_click(move |_, _, _| {
                                let _ = restart.try_send(Event::Command(Command::RestartUpdate));
                            }),
                    ),
            )
    }
}
