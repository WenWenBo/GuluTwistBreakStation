use crate::*;
use chrono::{DateTime, Duration, FixedOffset};
use uuid::Uuid;

const RECOVERY_MS: u64 = 60_000;
const NOTIFICATION_COOLDOWN_MS: u64 = 60_000;
const EXPIRY_MS: u64 = 120_000;

#[derive(Debug)]
pub struct Engine {
    pub settings: Settings,
    pub reminders: [ReminderRuntime; 4],
    pub session: Option<BreakSession>,
    pub pause: Pause,
    pub clock: ClockSample,
    work_window: Option<WorkWindow>,
    notification_cooldown_ms: u64,
    batch_deadline_ms: Option<u64>,
    pause_deadline_ms: Option<u64>,
    last_manual: [Option<u64>; 4],
    previewing: bool,
}

impl Engine {
    pub fn new(settings: Settings, clock: ClockSample, pause: Pause) -> Result<Self, ConfigError> {
        let settings = settings.normalized()?;
        let reminders = std::array::from_fn(|i| ReminderRuntime {
            remaining_ms: u64::from(settings.reminders[i].interval_minutes) * 60_000,
            phase: ReminderPhase::Waiting,
        });
        let work_window = settings.work_window(clock.wall);
        let pause_deadline_ms = pause_deadline(&pause, clock);
        Ok(Self {
            settings,
            reminders,
            session: None,
            pause,
            clock,
            work_window,
            notification_cooldown_ms: 0,
            batch_deadline_ms: None,
            pause_deadline_ms,
            last_manual: [None; 4],
            previewing: false,
        })
    }

    pub fn running(&self) -> bool {
        !self.previewing
            && self.settings.onboarded
            && self.settings.reminders.iter().any(|r| r.enabled)
            && self.work_window.is_some()
            && !self.clock.locked
            && matches!(self.pause, Pause::None)
            && self.session.is_none()
    }

    /// Preview is transient: calendar and pause state still advance, reminder intervals do not.
    pub fn set_preview(&mut self, active: bool, clock: ClockSample) -> Vec<Effect> {
        let effects = self.advance(clock, false);
        self.previewing = active;
        effects
    }
    pub fn tick(&mut self, clock: ClockSample) -> Vec<Effect> {
        self.advance(clock, true)
    }

    fn advance(&mut self, now: ClockSample, schedule: bool) -> Vec<Effect> {
        let mut effects = Vec::new();
        let old = self.clock;
        let awake_elapsed = now.awake_ms.saturating_sub(old.awake_ms);
        let continuous_elapsed = now.continuous_ms.saturating_sub(old.continuous_ms);
        let slept = continuous_elapsed > awake_elapsed.saturating_add(2000);
        let resumed = (old.locked && !now.locked) || slept;
        let previously_running = self.running();
        let had_session = self.session.is_some();
        let next_window = self.settings.work_window(now.wall);
        let changed_window = next_window != self.work_window;
        self.clock = now;

        // A wall-clock edit changes the next work boundary, never the rest deadline.
        let wall_elapsed = (now.wall - old.wall).num_milliseconds();
        let wall_changed = (wall_elapsed - i64::try_from(continuous_elapsed).unwrap_or(i64::MAX))
            .unsigned_abs()
            > 2000
            || now.wall.offset() != old.wall.offset();
        if wall_changed
            && !changed_window
            && let (Some(session), Some(end)) =
                (&mut self.session, self.settings.window_end(now.wall))
        {
            session.work_end_ms = now
                .continuous_ms
                .saturating_add((end - now.wall).num_milliseconds().max(0) as u64);
        }
        if wall_changed && changed_window {
            self.finish_session(
                Outcome::Cancelled,
                "本地时间已离开工作时段",
                now.wall,
                &mut effects,
            );
        }
        // Reconcile a rest in chronological order, even if no code ran during sleep.
        if let Some(session) = &self.session {
            let deadline = session.deadline_ms;
            let boundary = session.work_end_ms;
            if deadline <= now.continuous_ms
                && deadline < boundary
                && (schedule || deadline < now.continuous_ms)
            {
                let ended = event_wall(now, deadline);
                self.finish_session(Outcome::Timed, "休息倒计时结束", ended, &mut effects);
            } else if boundary <= now.continuous_ms || changed_window {
                let ended = event_wall(now, boundary.min(now.continuous_ms));
                self.finish_session(Outcome::Cancelled, "工作时段结束", ended, &mut effects);
            }
        }

        if changed_window {
            self.cancel_pending("工作时段结束", &mut effects);
            self.reset_all_waiting();
            self.work_window = next_window;
            self.batch_deadline_ms = None;
        }
        if self
            .pause_deadline_ms
            .is_some_and(|t| t <= now.continuous_ms)
        {
            self.pause = Pause::None;
            self.pause_deadline_ms = None;
        }

        // Do not attribute a whole suspended/changed-window gap to active work.
        let elapsed = if previously_running && !slept && !changed_window && !had_session {
            awake_elapsed
        } else {
            0
        };
        if elapsed > 0 {
            self.notification_cooldown_ms = self.notification_cooldown_ms.saturating_sub(elapsed);
            for kind in ReminderKind::ALL {
                let i = kind.index();
                if !self.settings.reminders[i].enabled {
                    continue;
                }
                let runtime = &mut self.reminders[i];
                match &mut runtime.phase {
                    ReminderPhase::Waiting => {
                        runtime.remaining_ms = runtime.remaining_ms.saturating_sub(elapsed)
                    }
                    ReminderPhase::Snoozed { remaining_ms, .. } => {
                        *remaining_ms = remaining_ms.saturating_sub(elapsed)
                    }
                    ReminderPhase::Pending {
                        notified: true,
                        age_ms,
                        ..
                    } => *age_ms = age_ms.saturating_add(elapsed),
                    _ => {}
                }
            }
        }

        if (!old.locked && now.locked) || slept {
            // Snoozes freeze, ordinary outstanding notices become stale.
            for kind in ReminderKind::ALL {
                if matches!(
                    self.reminders[kind.index()].phase,
                    ReminderPhase::Pending { .. }
                ) {
                    self.finish_item(kind, Outcome::Cancelled, "电脑锁屏或休眠", &mut effects);
                }
            }
            self.batch_deadline_ms = None;
        }
        if resumed {
            self.apply_recovery_buffer();
        }

        if !self.running() {
            return effects;
        }
        for kind in ReminderKind::ALL {
            if matches!(self.reminders[kind.index()].phase, ReminderPhase::Pending { notified: true, age_ms, .. } if age_ms >= EXPIRY_MS)
            {
                self.finish_item(kind, Outcome::Expired, "提醒未处理", &mut effects);
            }
        }
        if !schedule {
            return effects;
        }

        for kind in ReminderKind::ALL {
            let i = kind.index();
            if !self.settings.reminders[i].enabled {
                continue;
            }
            let runtime = &mut self.reminders[i];
            let instance = match &runtime.phase {
                ReminderPhase::Waiting if runtime.remaining_ms == 0 => Some(Instance {
                    id: Uuid::new_v4(),
                    kind,
                    break_seconds: self.settings.reminders[i].break_seconds,
                    created_at: now.wall,
                    gentle: self.settings.mode == ReminderMode::Gentle,
                }),
                ReminderPhase::Snoozed {
                    instance,
                    remaining_ms: 0,
                } => Some(instance.clone()),
                _ => None,
            };
            if let Some(instance) = instance {
                runtime.phase = ReminderPhase::Pending {
                    instance,
                    notified: false,
                    age_ms: 0,
                };
            }
        }
        let pending: Vec<Instance> = self
            .reminders
            .iter()
            .filter_map(|r| match &r.phase {
                ReminderPhase::Pending {
                    instance,
                    notified: false,
                    ..
                } => Some(instance.clone()),
                _ => None,
            })
            .collect();
        if pending.is_empty() {
            return effects;
        }
        let (gentle, pending): (Vec<_>, Vec<_>) = pending.into_iter().partition(|i| i.gentle);
        if !gentle.is_empty() && self.notification_cooldown_ms == 0 {
            self.mark_notified(&gentle);
            effects.push(Effect::Notify(gentle));
            self.notification_cooldown_ms = NOTIFICATION_COOLDOWN_MS;
        }
        if pending.is_empty() {
            return effects;
        }
        match self.settings.mode {
            ReminderMode::Strong => {
                let deadline = *self
                    .batch_deadline_ms
                    .get_or_insert(now.continuous_ms.saturating_add(1000));
                if now.continuous_ms >= deadline {
                    let duration_seconds = pending.iter().map(|i| i.break_seconds).max().unwrap();
                    let work_end = self.settings.window_end(now.wall).unwrap();
                    let until_end = (work_end - now.wall).num_milliseconds().max(0) as u64;
                    let session = BreakSession {
                        id: Uuid::new_v4(),
                        items: pending,
                        started_at: now.wall,
                        duration_seconds,
                        deadline_ms: now.continuous_ms + u64::from(duration_seconds) * 1000,
                        work_end_ms: now.continuous_ms.saturating_add(until_end),
                    };
                    for item in &session.items {
                        self.reminders[item.kind.index()].phase = ReminderPhase::Breaking;
                    }
                    effects.push(Effect::OpenBreak(session.clone()));
                    self.session = Some(session);
                    self.batch_deadline_ms = None;
                }
            }
            ReminderMode::Gentle => {
                if self.notification_cooldown_ms == 0 {
                    self.mark_notified(&pending);
                    effects.push(Effect::Notify(pending));
                    self.notification_cooldown_ms = NOTIFICATION_COOLDOWN_MS;
                }
            }
        }
        effects
    }

    pub fn set_pause(&mut self, pause: Pause, now: ClockSample) -> Vec<Effect> {
        let mut effects = self.advance(now, false);
        if pause.active(now.wall) {
            self.finish_session(Outcome::Cancelled, "提醒已暂停", now.wall, &mut effects);
            for kind in ReminderKind::ALL {
                if matches!(
                    self.reminders[kind.index()].phase,
                    ReminderPhase::Pending { .. }
                ) {
                    self.finish_item(kind, Outcome::Cancelled, "提醒已暂停", &mut effects);
                }
            }
            self.batch_deadline_ms = None;
        }
        self.pause_deadline_ms = pause_deadline(&pause, now);
        self.pause = pause;
        effects
    }

    pub fn interrupt(&mut self, now: ClockSample) -> Vec<Effect> {
        let mut effects = self.advance(now, false);
        self.finish_session(
            Outcome::Interrupted,
            "紧急退出本次休息",
            now.wall,
            &mut effects,
        );
        effects
    }

    pub fn overlay_failed(&mut self, session_id: Uuid, now: ClockSample) -> Vec<Effect> {
        if !self.session.as_ref().is_some_and(|s| s.id == session_id) {
            return Vec::new();
        }
        let mut session = self.session.take().unwrap();
        for item in &mut session.items {
            item.gentle = true;
            self.reminders[item.kind.index()].phase = ReminderPhase::Pending {
                instance: item.clone(),
                notified: true,
                age_ms: 0,
            };
        }
        self.clock = now;
        self.notification_cooldown_ms = NOTIFICATION_COOLDOWN_MS;
        // This instance is gentle even though the user's global mode stays strong.
        vec![Effect::CloseBreak, Effect::Notify(session.items)]
    }

    pub fn respond(&mut self, id: Uuid, response: Response, now: ClockSample) -> Vec<Effect> {
        let mut effects = self.advance(now, false);
        let kind = ReminderKind::ALL
            .into_iter()
            .find(|kind| matches!(&self.reminders[kind.index()].phase,ReminderPhase::Pending{instance,..} if instance.id==id));
        let Some(kind) = kind else {
            return effects;
        };
        match response {
            Response::Complete => {
                self.finish_item(kind, Outcome::Manual, "用户确认已完成", &mut effects)
            }
            Response::Skip => {
                self.finish_item(kind, Outcome::Skipped, "用户跳过本次提醒", &mut effects)
            }
            Response::Snooze(minutes) if [5, 10, 15].contains(&minutes) => {
                let instance = self.instance(kind).unwrap().clone();
                self.reminders[kind.index()].phase = ReminderPhase::Snoozed {
                    instance,
                    remaining_ms: u64::from(minutes) * 60_000,
                };
            }
            Response::Snooze(_) => {}
        }
        effects
    }

    pub fn manual_complete(&mut self, kind: ReminderKind, now: ClockSample) -> Vec<Effect> {
        let mut effects = self.advance(now, false);
        if self.session.is_some() || !self.settings.onboarded {
            return effects;
        }
        if self.last_manual[kind.index()]
            .is_some_and(|t| now.continuous_ms.saturating_sub(t) < 1000)
        {
            return effects;
        }
        self.last_manual[kind.index()] = Some(now.continuous_ms);
        if self.instance(kind).is_some() {
            self.finish_item(kind, Outcome::Manual, "主动完成", &mut effects);
        } else {
            let instance = Instance {
                id: Uuid::new_v4(),
                kind,
                break_seconds: self.settings.reminders[kind.index()].break_seconds,
                created_at: now.wall,
                gentle: true,
            };
            effects.push(record(
                instance,
                None,
                Outcome::Manual,
                now.wall,
                "主动完成",
            ));
            self.reset_item(kind);
        }
        effects
    }

    pub fn replace_settings(
        &mut self,
        settings: Settings,
        now: ClockSample,
    ) -> Result<Vec<Effect>, ConfigError> {
        let settings = settings.normalized()?;
        let mut effects = self.advance(now, false);
        let reset_all = settings.mode != self.settings.mode
            || settings.weekdays != self.settings.weekdays
            || settings.periods != self.settings.periods
            || settings.onboarded != self.settings.onboarded;
        let reset_items: Vec<_> = ReminderKind::ALL
            .into_iter()
            .filter(|kind| {
                let i = kind.index();
                reset_all
                    || settings.reminders[i].enabled != self.settings.reminders[i].enabled
                    || settings.reminders[i].interval_minutes
                        != self.settings.reminders[i].interval_minutes
            })
            .collect();
        for kind in &reset_items {
            self.finish_item(*kind, Outcome::Cancelled, "提醒设置已变更", &mut effects);
            if let Some(session) = &mut self.session
                && let Some(pos) = session.items.iter().position(|item| item.kind == *kind)
            {
                let item = session.items.remove(pos);
                effects.push(record(
                    item,
                    Some(session.id),
                    Outcome::Cancelled,
                    now.wall,
                    "提醒设置已变更",
                ));
            }
        }
        if self.session.as_ref().is_some_and(|s| s.items.is_empty()) {
            self.session = None;
            effects.push(Effect::CloseBreak);
        }
        self.settings = settings;
        for kind in reset_items {
            self.reset_item(kind);
        }
        if reset_all {
            self.batch_deadline_ms = None;
            self.notification_cooldown_ms = 0;
        }
        self.work_window = self.settings.work_window(now.wall);
        Ok(effects)
    }

    pub fn shutdown(&mut self, now: ClockSample) -> Vec<Effect> {
        let mut effects = self.advance(now, false);
        self.finish_session(Outcome::Interrupted, "应用退出", now.wall, &mut effects);
        self.cancel_pending("应用退出", &mut effects);
        effects
    }

    pub fn instance(&self, kind: ReminderKind) -> Option<&Instance> {
        match &self.reminders[kind.index()].phase {
            ReminderPhase::Pending { instance, .. } | ReminderPhase::Snoozed { instance, .. } => {
                Some(instance)
            }
            _ => None,
        }
    }

    pub fn pending(&self) -> Vec<Instance> {
        self.reminders
            .iter()
            .filter_map(|r| match &r.phase {
                ReminderPhase::Pending { instance, .. } => Some(instance.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn reminder_summary(&self) -> ReminderSummary {
        // Already delivered notices take precedence over notices waiting for cooldown.
        for delivered in [true, false] {
            let kinds: Vec<_> = ReminderKind::ALL.into_iter().filter(|kind| {
                let i = kind.index();
                self.settings.reminders[i].enabled
                    && matches!(self.reminders[i].phase, ReminderPhase::Pending { notified, .. } if notified == delivered)
            }).collect();
            if !kinds.is_empty() {
                return ReminderSummary::Due {
                    kinds,
                    notified: delivered,
                };
            }
        }
        let candidates: Vec<_> = ReminderKind::ALL
            .into_iter()
            .filter_map(|kind| {
                let i = kind.index();
                if !self.settings.reminders[i].enabled {
                    return None;
                }
                match self.reminders[i].phase {
                    ReminderPhase::Waiting => Some((kind, self.reminders[i].remaining_ms, false)),
                    ReminderPhase::Snoozed { remaining_ms, .. } => Some((kind, remaining_ms, true)),
                    _ => None,
                }
            })
            .collect();
        let Some(remaining_ms) = candidates.iter().map(|(_, ms, _)| *ms).min() else {
            return ReminderSummary::None;
        };
        let tied: Vec<_> = candidates
            .into_iter()
            .filter(|(_, ms, _)| *ms == remaining_ms)
            .collect();
        ReminderSummary::Upcoming {
            kinds: tied.iter().map(|(kind, _, _)| *kind).collect(),
            remaining_ms,
            snoozed: tied.iter().all(|(_, _, snoozed)| *snoozed),
        }
    }

    /// A bounded watchdog also notices missed native events and wall-clock edits.
    pub fn next_wake_ms(&self) -> u64 {
        if let Some(session) = &self.session {
            return session
                .deadline_ms
                .min(session.work_end_ms)
                .saturating_sub(self.clock.continuous_ms)
                .clamp(1, 60_000);
        }
        let mut wait = 60_000;
        if let Some(t) = self.pause_deadline_ms {
            wait = wait.min(t.saturating_sub(self.clock.continuous_ms).max(1));
        }
        let boundary = self
            .settings
            .window_end(self.clock.wall)
            .unwrap_or_else(|| self.settings.next_start(self.clock.wall, false));
        wait = wait.min((boundary - self.clock.wall).num_milliseconds().max(1) as u64);
        if let Some(deadline) = self.batch_deadline_ms {
            wait = wait.min(deadline.saturating_sub(self.clock.continuous_ms).max(1));
        }
        if !self.running() {
            return wait;
        }
        for (i, r) in self.reminders.iter().enumerate() {
            if !self.settings.reminders[i].enabled {
                continue;
            }
            let remaining = match &r.phase {
                ReminderPhase::Waiting => r.remaining_ms,
                ReminderPhase::Snoozed { remaining_ms, .. } => *remaining_ms,
                ReminderPhase::Pending {
                    notified: true,
                    age_ms,
                    ..
                } => EXPIRY_MS.saturating_sub(*age_ms),
                ReminderPhase::Pending {
                    notified: false, ..
                } => self.notification_cooldown_ms.max(1000),
                ReminderPhase::Breaking => 60_000,
            };
            wait = wait.min(remaining.max(1));
        }
        wait
    }

    fn mark_notified(&mut self, instances: &[Instance]) {
        for item in instances {
            if let ReminderPhase::Pending {
                notified, age_ms, ..
            } = &mut self.reminders[item.kind.index()].phase
            {
                *notified = true;
                *age_ms = 0;
            }
        }
    }
    fn interval(&self, kind: ReminderKind) -> u64 {
        u64::from(self.settings.reminders[kind.index()].interval_minutes) * 60_000
    }
    fn reset_item(&mut self, kind: ReminderKind) {
        self.reminders[kind.index()] = ReminderRuntime {
            remaining_ms: self.interval(kind),
            phase: ReminderPhase::Waiting,
        };
    }
    fn reset_all_waiting(&mut self) {
        for kind in ReminderKind::ALL {
            self.reset_item(kind);
        }
    }
    fn apply_recovery_buffer(&mut self) {
        for r in &mut self.reminders {
            match &mut r.phase {
                ReminderPhase::Waiting => r.remaining_ms = r.remaining_ms.max(RECOVERY_MS),
                ReminderPhase::Snoozed { remaining_ms, .. } => {
                    *remaining_ms = (*remaining_ms).max(RECOVERY_MS)
                }
                _ => {}
            }
        }
    }
    fn finish_item(
        &mut self,
        kind: ReminderKind,
        outcome: Outcome,
        reason: &str,
        effects: &mut Vec<Effect>,
    ) {
        if let Some(item) = self.instance(kind).cloned() {
            effects.push(record(item, None, outcome, self.clock.wall, reason));
            self.reset_item(kind);
        }
    }
    fn cancel_pending(&mut self, reason: &str, effects: &mut Vec<Effect>) {
        for kind in ReminderKind::ALL {
            self.finish_item(kind, Outcome::Cancelled, reason, effects);
        }
    }
    fn finish_session(
        &mut self,
        outcome: Outcome,
        reason: &str,
        ended: DateTime<FixedOffset>,
        effects: &mut Vec<Effect>,
    ) {
        if let Some(session) = self.session.take() {
            effects.push(Effect::CloseBreak);
            for item in session.items {
                let kind = item.kind;
                effects.push(record(
                    item,
                    Some(session.id),
                    outcome.clone(),
                    ended,
                    reason,
                ));
                self.reset_item(kind);
            }
            self.apply_recovery_buffer();
        }
    }
}

fn event_wall(now: ClockSample, event_ms: u64) -> DateTime<FixedOffset> {
    now.wall
        - Duration::milliseconds(
            now.continuous_ms
                .saturating_sub(event_ms)
                .min(i64::MAX as u64) as i64,
        )
}
fn record(
    item: Instance,
    session_id: Option<Uuid>,
    outcome: Outcome,
    ended_at: DateTime<FixedOffset>,
    reason: &str,
) -> Effect {
    Effect::Record(ActivityRecord {
        id: item.id,
        instance_id: item.id,
        session_id,
        kind: item.kind,
        outcome,
        ended_at,
        local_date: ended_at.date_naive().to_string(),
        reason: reason.into(),
    })
}

fn pause_deadline(pause: &Pause, now: ClockSample) -> Option<u64> {
    match pause {
        Pause::Until(t) => Some(
            now.continuous_ms
                .saturating_add((*t - now.wall).num_milliseconds().max(0) as u64),
        ),
        _ => None,
    }
}
