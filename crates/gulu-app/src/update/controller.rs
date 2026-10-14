use super::{ResultEvent, Work, backend::Backend, dialog::Dialog};
use crate::{
    Controller, Event,
    persistence::{Job, UpdateExit},
    platform,
};
use gpui_kit::*;
use gulu_core::Engine;
use gulu_storage::{UpdatePackage, UpdatePreferences};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub struct Restart {
    id: u64,
    package: UpdatePackage,
    staged: Option<Engine>,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
}
impl Restart {
    fn authorized(&self) -> bool {
        !self.cancelled.load(Ordering::SeqCst) && Instant::now() < self.deadline
    }
}
impl Controller {
    fn show_update_error(&mut self, cx: &mut App) {
        self.snapshot.borrow_mut().message = self.update.error.clone();
        self.open_main(cx);
    }
    pub(crate) fn cancel_update(&mut self) {
        if let Some(r) = &self.update_restart {
            r.cancelled.store(true, Ordering::SeqCst);
        }
    }
    pub(crate) fn init_update(&mut self, warning: String) {
        if !warning.is_empty() {
            self.update.error = warning;
            self.update.status = "更新不可用".into();
            return;
        }
        match Backend::new() {
            Ok(backend) => {
                let backend = Arc::new(backend);
                let worker = super::worker(backend.clone(), self.tx.clone());
                self.update.available = true;
                self.update.busy = true;
                self.update.operation += 1;
                let _ = worker.send(Work::Restore(
                    self.update.operation,
                    self.update.prefs.ready.clone(),
                ));
                self.update_backend = Some(backend);
                self.update_jobs = Some(worker);
            }
            Err(e) => self.update.status = e.to_string(),
        }
    }
    pub(crate) fn save_update(&mut self, mut prefs: UpdatePreferences) {
        if let Some(value) = self.update.pending_automatic {
            prefs.automatic = value;
        }
        self.update.revision += 1;
        self.update.saving = true;
        if self
            .jobs
            .send(Job::SaveUpdate(self.update.revision, prefs))
            .is_err()
        {
            self.update.saving = false;
            self.update.error = "更新设置未保存：存储线程不可用".into();
        }
    }
    pub(crate) fn update_saved(
        &mut self,
        id: u64,
        prefs: UpdatePreferences,
        result: Result<(), String>,
    ) {
        if id != self.update.revision {
            return;
        }
        self.update.saving = false;
        self.update.pending_automatic = None;
        match result {
            Ok(()) => {
                self.update.prefs = prefs;
                if self.update.prefs.ready.is_some() {
                    self.update.ready_status();
                }
            }
            Err(e) => {
                self.update.error = format!("更新设置未保存：{e}。请重启应用后重试。");
                self.update.available = false;
            }
        }
    }
    pub(crate) fn check_update(&mut self, manual: bool) {
        let now = chrono::Utc::now().timestamp();
        if self.update.begin(now, manual) {
            self.save_update(self.update.prefs.clone());
            if let Some(worker) = &self.update_jobs {
                let _ = worker.send(Work::Check(self.update.operation));
            }
        }
    }
    pub(crate) fn update_allowed(&self) -> bool {
        !self.quitting
            && !self.snapshot.borrow().recovery
            && self.engine.settings.onboarded
            && self.engine.session.is_none()
            && self.preview.is_none()
            && !platform::clock().locked
            && !self.snapshot.borrow().saving
    }
    pub(crate) fn close_update_dialog(&mut self, cx: &mut App) {
        if let Some((w, _)) = self.update_dialog.take() {
            let _ = w.update(cx, |_, w, _| w.remove_window());
        }
    }
    pub(crate) fn restart_update(&mut self, cx: &mut App) {
        if !self.update_allowed() {
            self.update.error = "请结束休息、解锁屏幕并完成当前设置后再更新".into();
            self.show_update_error(cx);
            return;
        }
        if self.update.busy
            || self.update.saving
            || self.preferences.saving()
            || self.update_restart.is_some()
        {
            return;
        }
        let Some(package) = self.update.prefs.ready.clone() else {
            return;
        };
        if let Some((handle, view)) = &self.main {
            let view = view.clone();
            let needs_draft = handle
                .update(cx, |_, w, cx| {
                    view.update(cx, |v, cx| v.guard_update(w, cx))
                })
                .unwrap_or(true);
            if needs_draft {
                self.update.later();
                self.close_update_dialog(cx);
                self.open_main(cx);
                return;
            }
        }
        self.update.later();
        self.close_update_dialog(cx);
        self.update.busy = true;
        self.update.operation += 1;
        self.update.status = "正在复核更新包…".into();
        self.update.error.clear();
        self.update_restart = Some(Restart {
            id: self.update.operation,
            package: package.clone(),
            staged: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + Duration::from_secs(10),
        });
        if let Some(worker) = &self.update_jobs {
            let _ = worker.send(Work::Verify(self.update.operation, package));
        }
    }
    pub(crate) fn update_event(&mut self, event: ResultEvent, cx: &mut App) {
        match event {
            ResultEvent::Progress(id, p) if id == self.update.operation => {
                self.update.status = format!("正在后台下载更新… {}%", p.clamp(0, 100))
            }
            ResultEvent::Restored(id, result) if id == self.update.operation => {
                self.update.busy = false;
                self.update.restored = true;
                let mut prefs = self.update.prefs.clone();
                match result {
                    Ok(p) => prefs.ready = p,
                    Err(e) => {
                        prefs.ready = None;
                        self.update.error = format!("缓存无法恢复：{e}。可重新检查更新。");
                    }
                }
                self.update.prefs.ready = None; // expose Ready only after the metadata write succeeds
                self.update.ready_status();
                self.save_update(prefs);
            }
            ResultEvent::Checked(id, result) if id == self.update.operation => {
                self.update
                    .checked(chrono::Utc::now().timestamp(), result.is_ok());
                let mut prefs = self.update.prefs.clone();
                match result {
                    Ok(Some(p)) => {
                        if self.update.suppress_download_prompt {
                            self.update.defer_version(p.version.clone());
                        }
                        prefs.ready = Some(p);
                        self.update.status = "下载完成，正在保存更新状态…".into();
                    }
                    Ok(None) => self.update.status = "已是最新正式版".into(),
                    Err(e) => {
                        self.update.error = format!("更新失败：{e}。可以继续使用当前版本。");
                        self.update.ready_status();
                    }
                }
                self.save_update(prefs);
            }
            ResultEvent::Verified(id, result) if id == self.update.operation => {
                if self.update_restart.as_ref().is_none_or(|r| r.id != id) {
                    self.update.busy = false;
                    self.update.ready_status();
                    return;
                }
                let cancelled = !self.update_restart.as_ref().unwrap().authorized();
                if result.is_err() || cancelled || !self.update_allowed() {
                    if result.as_ref().is_err_and(|e| e.invalid_cache) {
                        self.update.prefs.ready = None;
                        self.save_update(self.update.prefs.clone());
                    }
                    self.update.error = result
                        .err()
                        .map(|e| e.message)
                        .unwrap_or_else(|| "本次重启已取消，请稍后重试".into());
                    self.update_restart = None;
                    self.update.busy = false;
                    self.update.ready_status();
                    self.show_update_error(cx);
                    return;
                }
                let mut staged = self.engine.clone();
                let effects = staged.shutdown(platform::clock());
                let request = self.update_restart.as_mut().unwrap();
                request.staged = Some(staged.clone());
                self.update.status = "正在保存数据，准备重启…".into();
                let job = UpdateExit {
                    id,
                    effects,
                    pause: staged.pause.clone(),
                    ui: self.preferences.current.clone(),
                    update: self.update.prefs.clone(),
                    cancelled: request.cancelled.clone(),
                };
                if self
                    .jobs
                    .send(Job::PrepareUpdateExit(Box::new(job)))
                    .is_err()
                {
                    self.update_prepared(id, Err("存储线程不可用".into()), cx);
                }
            }
            _ => {}
        }
    }
    pub(crate) fn update_prepared(&mut self, id: u64, result: Result<(), String>, cx: &mut App) {
        if self.update_restart.as_ref().is_none_or(|r| r.id != id) {
            return;
        }
        let mut request = self.update_restart.take().unwrap();
        if result.is_ok() {
            // Even a late committed result must reconcile memory, never restore pre-commit state.
            if let Some(staged) = request.staged.take() {
                self.engine = staged;
                let _ = self
                    .jobs
                    .send(Job::ReadDay(self.engine.clock.wall.date_naive()));
            }
        }
        let result = result.and_then(|()| {
            if !request.authorized() || !self.update_allowed() {
                return Err("本次重启已取消；已保存的数据保持有效".into());
            }
            self.update_backend
                .as_ref()
                .ok_or_else(|| "更新器不可用".to_string())?
                .arm(&request.package)
                .map_err(|e| e.to_string())
        });
        match result {
            Ok(()) => {
                self.update_armed = true;
                self.quitting = true; // on_app_quit must not generate any more shutdown effects
                self.hide_floating(cx);
                self.close_reminder_rows(cx);
                self.close_update_dialog(cx);
                self.close_overlays(cx);
                self.tray.take();
                cx.quit();
            }
            Err(e) => {
                self.update.busy = false;
                self.update.ready_status();
                self.update.error = format!("未重启：{e}");
                self.show_update_error(cx);
            }
        }
    }
    pub(crate) fn sync_update(&mut self, cx: &mut App) {
        if self.quitting {
            return;
        }
        if let Some(r) = &self.update_restart
            && Instant::now() >= r.deadline
            && !r.cancelled.swap(true, Ordering::SeqCst)
        {
            self.update.error = "保存或复核超时，本次重启已取消；正在确认后台结果。".into();
            if r.staged.is_none() {
                self.update_restart = None;
            }
        }
        let now = chrono::Utc::now().timestamp();
        if self.update.due(now) {
            self.check_update(false);
        }
        let allowed = self.update_allowed() && self.update_restart.is_none();
        if !allowed {
            self.close_update_dialog(cx);
        }
        let mut view = self.update.view();
        view.preparing = self.update_restart.is_some();
        self.snapshot.borrow_mut().update = view;
        if allowed
            && self.update.should_prompt()
            && self.update_dialog.is_none()
            && Instant::now() >= self.update_prompt_due
        {
            self.update_prompt_due = Instant::now() + Duration::from_secs(60);
            let shared = self.snapshot.clone();
            let tx = self.tx.clone();
            let close = tx.clone();
            match gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(480.), px(360.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(400.), px(300.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("小站更新".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                move |w, cx| {
                    w.on_window_should_close(cx, move |_, _| {
                        let _ = close.try_send(Event::Command(crate::Command::UpdateLater));
                        false
                    });
                    cx.new(|cx| Dialog::new(shared, tx, w, cx))
                },
            ) {
                Ok(window) => {
                    self.update_dialog = Some(window);
                    cx.activate(true);
                }
                Err(e) => self.update.error = format!("更新提示无法打开：{e}；可在关于小站中更新"),
            }
        }
        if let Some((_, view)) = &self.update_dialog {
            view.update(cx, |_, cx| cx.notify());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Restart;
    use gulu_storage::UpdatePackage;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };
    #[test]
    fn late_success_and_cancelled_request_never_reauthorize_restart() {
        let package = UpdatePackage {
            source: "source".into(),
            channel: "channel".into(),
            package_id: "app".into(),
            version: "2.0.0".into(),
            file_name: "app-full.nupkg".into(),
            size: 1,
            sha256: "a".repeat(64),
            notes: String::new(),
        };
        let mut request = Restart {
            id: 1,
            package,
            staged: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now() + Duration::from_secs(10),
        };
        assert!(request.authorized());
        request.deadline = Instant::now() - Duration::from_secs(1);
        assert!(!request.authorized());
        request.deadline = Instant::now() + Duration::from_secs(10);
        request.cancelled.store(true, Ordering::SeqCst);
        assert!(!request.authorized()); // a successful storage receipt does not change the intent
    }
}
