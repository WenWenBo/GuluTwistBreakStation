use crate::ReminderKind;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct ClockSample {
    pub wall: DateTime<FixedOffset>,
    /// Monotonic time excluding system sleep.
    pub awake_ms: u64,
    /// Monotonic time including system sleep, unaffected by wall-clock edits.
    pub continuous_ms: u64,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Pause {
    #[default]
    None,
    Manual,
    Until(DateTime<FixedOffset>),
}
impl Pause {
    pub fn active(&self, wall: DateTime<FixedOffset>) -> bool {
        match self {
            Self::None => false,
            Self::Manual => true,
            Self::Until(t) => *t > wall,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Timed,
    Manual,
    Skipped,
    Interrupted,
    Expired,
    Cancelled,
}
impl Outcome {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Timed => "计时完成",
            Self::Manual => "手动完成",
            Self::Skipped => "已跳过",
            Self::Interrupted => "已中断",
            Self::Expired => "未处理",
            Self::Cancelled => "已取消",
        }
    }
    pub fn completed(&self) -> bool {
        matches!(self, Self::Timed | Self::Manual)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityRecord {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub session_id: Option<Uuid>,
    pub kind: ReminderKind,
    pub outcome: Outcome,
    pub ended_at: DateTime<FixedOffset>,
    pub local_date: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: Uuid,
    pub kind: ReminderKind,
    pub break_seconds: u32,
    pub created_at: DateTime<FixedOffset>,
    #[serde(default)]
    pub gentle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakSession {
    pub id: Uuid,
    pub items: Vec<Instance>,
    pub started_at: DateTime<FixedOffset>,
    pub deadline_ms: u64,
    pub work_end_ms: u64,
    pub duration_seconds: u32,
}
impl BreakSession {
    pub fn remaining_seconds(&self, continuous_ms: u64) -> u64 {
        self.deadline_ms
            .saturating_sub(continuous_ms)
            .div_ceil(1000)
    }
}

#[derive(Debug, Clone)]
pub enum ReminderPhase {
    Waiting,
    Pending {
        instance: Instance,
        notified: bool,
        age_ms: u64,
    },
    Snoozed {
        instance: Instance,
        remaining_ms: u64,
    },
    Breaking,
}
#[derive(Debug, Clone)]
pub struct ReminderRuntime {
    pub remaining_ms: u64,
    pub phase: ReminderPhase,
}

/// Read-only summary for compact views. Times remain effective working time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReminderSummary {
    None,
    Upcoming {
        kinds: Vec<ReminderKind>,
        remaining_ms: u64,
        snoozed: bool,
    },
    Due {
        kinds: Vec<ReminderKind>,
        notified: bool,
    },
}

#[derive(Debug, Clone)]
pub enum Effect {
    OpenBreak(BreakSession),
    CloseBreak,
    Notify(Vec<Instance>),
    Record(ActivityRecord),
}

#[derive(Debug, Clone, Copy)]
pub enum Response {
    Complete,
    Snooze(u32),
    Skip,
}
