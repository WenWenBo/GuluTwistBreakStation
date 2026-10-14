use gulu_storage::UpdatePreferences;
use std::collections::HashSet;

#[derive(Clone, Default)]
pub struct View {
    pub status: String,
    pub error: String,
    pub notes: String,
    pub automatic: bool,
    pub available: bool,
    pub busy: bool,
    pub ready: bool,
    pub preparing: bool,
    pub last_success: i64,
}

pub struct State {
    pub prefs: UpdatePreferences,
    pub busy: bool,
    pub available: bool,
    pub restored: bool,
    pub operation: u64,
    pub revision: u64,
    pub saving: bool,
    pub suppress_download_prompt: bool,
    pub pending_automatic: Option<bool>,
    pub status: String,
    pub error: String,
    pub next: i64,
    pub last_manual: i64,
    failures: usize,
    jitter: i64,
    deferred: HashSet<String>,
}
impl State {
    pub fn new(prefs: UpdatePreferences, now: i64, jitter: i64) -> Self {
        Self {
            prefs,
            busy: false,
            available: false,
            restored: false,
            operation: 0,
            revision: 0,
            saving: false,
            suppress_download_prompt: false,
            pending_automatic: None,
            status: "正在读取更新状态…".into(),
            error: String::new(),
            next: now + 30 + jitter % 61,
            last_manual: 0,
            failures: 0,
            jitter,
            deferred: HashSet::new(),
        }
    }
    pub fn view(&self) -> View {
        View {
            status: self.status.clone(),
            error: self.error.clone(),
            automatic: self.pending_automatic.unwrap_or(self.prefs.automatic),
            notes: self
                .prefs
                .ready
                .as_ref()
                .map(|p| p.notes.clone())
                .unwrap_or_default(),
            available: self.available,
            busy: self.busy || self.saving,
            preparing: false,
            ready: self.restored && self.prefs.ready.is_some(),
            last_success: self.prefs.last_success,
        }
    }
    pub fn due(&self, now: i64) -> bool {
        self.available
            && self.restored
            && !self.busy
            && !self.saving
            && self.prefs.automatic
            && self.prefs.ready.is_none()
            && (now >= self.next || now < self.prefs.last_attempt)
            && elapsed(now, self.prefs.last_attempt) >= 900
            && (now >= self.prefs.retry_after || now < self.prefs.last_attempt)
    }
    pub fn wait_ms(&self, now: i64) -> u64 {
        if !self.available
            || !self.restored
            || self.busy
            || self.saving
            || !self.prefs.automatic
            || self.prefs.ready.is_some()
        {
            return 30_000;
        }
        if self.due(now) {
            return 1;
        }
        let next = self
            .next
            .max(self.prefs.last_attempt.saturating_add(900))
            .max(self.prefs.retry_after);
        next.saturating_sub(now)
            .saturating_mul(1000)
            .clamp(1, 30_000) as u64
    }
    pub fn begin(&mut self, now: i64, manual: bool) -> bool {
        if !self.available
            || !self.restored
            || self.busy
            || self.saving
            || self.prefs.ready.is_some()
        {
            return false;
        }
        if manual && elapsed(now, self.last_manual) < 60 {
            self.error = "请稍候一分钟再检查更新".into();
            return false;
        }
        if manual {
            self.last_manual = now;
        }
        self.prefs.last_attempt = now;
        // Disabling automatic updates suppresses only the download already in flight.
        self.suppress_download_prompt = false;
        self.busy = true;
        self.operation += 1;
        self.status = "正在检查更新…".into();
        self.error.clear();
        true
    }
    pub fn checked(&mut self, now: i64, success: bool) {
        self.busy = false;
        if success {
            self.failures = 0;
            self.prefs.last_success = now;
            self.prefs.retry_after = 0;
            self.next = now + 12 * 3600 + self.jitter % 1801;
        } else {
            let delay = [900, 3600, 10800, 43200][self.failures.min(3)];
            self.failures += 1;
            self.next = now + delay + self.jitter % 61;
            self.prefs.retry_after = self.next;
        }
    }
    pub fn defer_version(&mut self, version: String) {
        self.deferred.insert(version);
    }
    pub fn later(&mut self) {
        if let Some(p) = &self.prefs.ready {
            self.deferred.insert(p.version.clone());
        }
    }
    pub fn should_prompt(&self) -> bool {
        self.restored
            && !self.busy
            && !self.saving
            && self
                .prefs
                .ready
                .as_ref()
                .is_some_and(|p| !self.deferred.contains(&p.version))
    }
    pub fn ready_status(&mut self) {
        self.status = self
            .prefs
            .ready
            .as_ref()
            .map(|p| format!("{} 已下载，重启后更新", p.version))
            .unwrap_or_else(|| "可检查更新".into());
    }
}
fn elapsed(now: i64, then: i64) -> i64 {
    if then == 0 || now < then {
        i64::MAX
    } else {
        now.saturating_sub(then)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn later_survives_window_reopen_but_not_process_restart() {
        let package = gulu_storage::UpdatePackage {
            source: "source".into(),
            channel: "channel".into(),
            package_id: "app".into(),
            version: "2.0.0".into(),
            file_name: "app-full.nupkg".into(),
            size: 1,
            sha256: "a".repeat(64),
            notes: String::new(),
        };
        let prefs = UpdatePreferences {
            automatic: false,
            ready: Some(package),
            ..Default::default()
        };
        let mut s = State::new(prefs.clone(), 1000, 0);
        s.restored = true;
        assert!(s.should_prompt());
        s.later();
        assert!(!s.should_prompt());
        assert!(!s.due(999999));
        let mut restarted = State::new(prefs, 1001, 0);
        assert!(!restarted.should_prompt());
        restarted.restored = true;
        assert!(restarted.should_prompt());
        restarted.busy = true;
        assert!(!restarted.should_prompt());
    }
    #[test]
    fn persisted_attempt_limits_restarts_and_rollback_still_recovers() {
        let prefs = UpdatePreferences {
            last_attempt: 10000,
            retry_after: 10900,
            ..Default::default()
        };
        let mut s = State::new(prefs, 10001, 0);
        s.restored = true;
        s.available = true;
        assert!(!s.due(10032));
        assert!(s.due(10900));
        assert!(s.due(9000));
        s.prefs.automatic = false;
        assert!(!s.due(10900));
        assert!(s.begin(10900, true));
    }
    #[test]
    fn restart_rate_limit_backoff_and_clock_rollback() {
        let mut s = State::new(UpdatePreferences::default(), 10000, 0);
        s.available = true;
        s.restored = true;
        assert!(!s.due(10001));
        assert!(s.due(10030));
        assert!(s.begin(10030, false));
        assert!(!s.begin(10030, true));
        s.checked(10031, false);
        assert!(!s.due(10032));
        assert!(s.due(10931));
        assert!(s.begin(10931, true));
        s.checked(10932, true);
        assert!(!s.begin(10933, true));
        assert!(s.begin(9000, true)); // a clock adjustment cannot disable manual checks forever
    }
    #[test]
    fn disabling_automatic_only_suppresses_the_current_download_prompt() {
        let mut s = State::new(UpdatePreferences::default(), 10000, 0);
        s.available = true;
        s.restored = true;
        assert!(s.begin(10030, false));
        s.prefs.automatic = false;
        s.suppress_download_prompt = true;
        assert!(!s.begin(10031, true));
        assert!(s.suppress_download_prompt);
        s.checked(10032, false);
        assert!(s.begin(10100, true));
        assert!(!s.suppress_download_prompt);
    }
}
