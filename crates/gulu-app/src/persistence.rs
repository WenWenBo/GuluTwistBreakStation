use crate::Event;
use async_channel::Sender;
use chrono::Local;
use gulu_core::*;
use gulu_storage::{Store, UiPreferences};
use std::{path::PathBuf, sync::mpsc, time::Duration};
pub enum Job {
    SaveUi(u64, UiPreferences),
    Save(Settings),
    Commit(Vec<Effect>, Option<BreakSession>, Pause),
    Clear,
    ReadDay(chrono::NaiveDate),
    Recover(bool),
    Flush,
    Barrier(async_channel::Sender<()>),
}
pub struct Loaded {
    pub ui: UiPreferences,
    pub ui_warning: String,
    pub settings: Settings,
    pub pause: Pause,
    pub records: Vec<ActivityRecord>,
    pub error: Option<String>,
    pub backup: bool,
}
pub fn start(path: PathBuf, tx: Sender<Event>) -> (mpsc::Sender<Job>, Loaded) {
    let mut store = Store::open(&path).ok();
    let initial = (|| -> anyhow::Result<Loaded> {
        let s = store
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("数据文件无法打开"))?;
        let settings = s.settings()?;
        let pause = s.pause()?;
        s.recover_session(Local::now().fixed_offset())?;
        s.prune(Local::now().date_naive())?;
        let records = s.records(Local::now().date_naive())?;
        let (ui, ui_warning) = match s.ui_preferences() {
            Ok(ui) => (ui, String::new()),
            Err(e) => (
                UiPreferences::default(),
                format!("悬浮窗设置无法读取，暂时关闭；原数据已保留：{e}"),
            ),
        };
        Ok(Loaded {
            ui,
            ui_warning,
            settings,
            pause,
            records,
            error: None,
            backup: false,
        })
    })();
    let loaded = initial.unwrap_or_else(|e| Loaded {
        ui: UiPreferences::default(),
        ui_warning: String::new(),
        settings: Settings::default(),
        pause: Pause::Manual,
        records: Vec::new(),
        error: Some(format!(
            "上次数据无法读取：{e}。提醒尚未启动，恢复时会保留原文件。"
        )),
        backup: store
            .as_ref()
            .and_then(|s| s.settings_backup().ok().flatten())
            .is_some(),
    });
    let (jobs, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut pending = Vec::new();
        let mut session = None;
        let mut pause = Pause::None;
        let mut dirty = false;
        loop {
            let job = match rx.recv_timeout(Duration::from_secs(3)) {
                Ok(j) => Some(j),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(_) => break,
            };
            let flush = matches!(job, Some(Job::Flush));
            let mut barrier = None;
            match job {
                Some(Job::SaveUi(revision, prefs)) => {
                    let result = store
                        .as_ref()
                        .ok_or_else(|| "数据不可用，请先恢复".to_string())
                        .and_then(|s| s.save_ui_preferences(&prefs).map_err(|e| e.to_string()));
                    let _ = tx.try_send(Event::UiSaved(revision, result));
                }
                Some(Job::Barrier(tx)) => barrier = Some(tx),
                Some(Job::Save(settings)) => {
                    let result = store
                        .as_mut()
                        .ok_or_else(|| "数据不可用，请先恢复".to_string())
                        .and_then(|s| s.save_settings(&settings).map_err(|e| e.to_string()))
                        .map(|_| settings);
                    let _ = tx.try_send(Event::Saved(result));
                }
                Some(Job::Commit(e, s, p)) => {
                    pending.extend(e);
                    session = s;
                    pause = p;
                    dirty = true;
                }
                Some(Job::ReadDay(day)) => {
                    let result = store
                        .as_ref()
                        .ok_or_else(|| "数据不可用".to_string())
                        .and_then(|s| s.records(day).map_err(|e| e.to_string()));
                    let _ = tx.try_send(Event::Records(day, result));
                }
                Some(Job::Clear) => {
                    let r = store
                        .as_ref()
                        .ok_or_else(|| "数据不可用".to_string())
                        .and_then(|s| s.clear_records().map_err(|e| e.to_string()));
                    if r.is_ok() {
                        pending.clear();
                    }
                    let _ = tx.try_send(Event::Cleared(r));
                }
                Some(Job::Recover(use_backup)) => {
                    let settings = if use_backup {
                        store
                            .as_ref()
                            .and_then(|s| s.settings_backup().ok().flatten())
                    } else {
                        Some(Settings::default())
                    };
                    let result = (|| -> anyhow::Result<Settings> {
                        let settings =
                            settings.ok_or_else(|| anyhow::anyhow!("没有可用的设置备份"))?;
                        drop(store.take());
                        let suffix = Local::now().format("%Y%m%d-%H%M%S-%f").to_string();
                        for ext in ["", "-wal", "-shm"] {
                            let p = PathBuf::from(format!("{}{ext}", path.display()));
                            if p.exists() {
                                std::fs::rename(
                                    &p,
                                    PathBuf::from(format!(
                                        "{}.recovery-{suffix}{ext}",
                                        path.display()
                                    )),
                                )?;
                            }
                        }
                        let mut s = Store::open(&path)?;
                        s.save_settings(&settings)?;
                        store = Some(s);
                        Ok(settings)
                    })();
                    match result {
                        Ok(s) => {
                            pending.clear();
                            dirty = false;
                            let _ = tx.try_send(Event::Recovered(Ok(s)));
                        }
                        Err(e) => {
                            let _ = tx.try_send(Event::Recovered(Err(e.to_string())));
                        }
                    }
                }
                _ => {}
            }
            if dirty {
                let r = store
                    .as_mut()
                    .ok_or_else(|| "数据不可用".to_string())
                    .and_then(|s| {
                        s.commit_effects(&pending, session.as_ref())
                            .and_then(|_| s.save_pause(&pause))
                            .and_then(|_| s.prune(Local::now().date_naive()))
                            .map_err(|e| e.to_string())
                    });
                match r {
                    Ok(()) => {
                        pending.clear();
                        dirty = false;
                        let _ = tx.try_send(Event::Persisted);
                    }
                    Err(e) => {
                        let _ = tx.try_send(Event::StorageError(format!(
                            "记录暂未保存，将自动重试：{e}"
                        )));
                    }
                }
            }
            if let Some(tx) = barrier {
                let _ = tx.try_send(());
            }
            if flush {
                let _ = tx.try_send(Event::Flushed);
            }
        }
    });
    (jobs, loaded)
}
