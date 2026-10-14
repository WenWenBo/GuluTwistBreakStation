pub mod backend;
pub mod controller;
pub mod dialog;
pub mod state;
use gulu_storage::UpdatePackage;
use std::sync::{Arc, mpsc};
pub enum Work {
    Restore(u64, Option<UpdatePackage>),
    Check(u64),
    Verify(u64, UpdatePackage),
}
pub enum ResultEvent {
    Restored(u64, Result<Option<UpdatePackage>, String>),
    Checked(u64, Result<Option<UpdatePackage>, String>),
    Progress(u64, i16),
    Verified(u64, Result<(), backend::PreparationError>),
}
pub fn worker(
    backend: Arc<backend::Backend>,
    tx: async_channel::Sender<crate::Event>,
) -> mpsc::Sender<Work> {
    let (jobs, rx) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(job) = rx.recv() {
            let result = match job {
                Work::Restore(id, p) => {
                    ResultEvent::Restored(id, backend.restore(p).map_err(|e| e.to_string()))
                }
                Work::Verify(id, p) => ResultEvent::Verified(id, backend.prepare(&p)),
                Work::Check(id) => {
                    let (progress, rx) = mpsc::channel();
                    let events = tx.clone();
                    let forward = std::thread::spawn(move || {
                        let mut last =
                            std::time::Instant::now() - std::time::Duration::from_secs(1);
                        while let Ok(p) = rx.recv() {
                            if last.elapsed().as_millis() >= 150 || p == 100 {
                                let _ = events
                                    .try_send(crate::Event::Update(ResultEvent::Progress(id, p)));
                                last = std::time::Instant::now();
                            }
                        }
                    });
                    let result = backend.check(progress).map_err(|e| e.to_string());
                    let _ = forward.join();
                    ResultEvent::Checked(id, result)
                }
            };
            let _ = tx.try_send(crate::Event::Update(result));
        }
    });
    jobs
}
