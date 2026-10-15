use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderKind {
    Water,
    Stretch,
    Walk,
    Stand,
    EyeCare,
}

impl ReminderKind {
    pub const COUNT: usize = 5;
    pub const ALL: [Self; Self::COUNT] = [
        Self::Water,
        Self::Stretch,
        Self::Walk,
        Self::Stand,
        Self::EyeCare,
    ];
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|v| *v == self).unwrap()
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Water => "咕嘟喝水",
            Self::Stretch => "扭扭颈肩",
            Self::Walk => "如厕放风",
            Self::Stand => "站立活动",
            Self::EyeCare => "保护眼睛",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            Self::Water => "咕嘟一口，给自己一点水分。",
            Self::Stretch => "放松肩膀，轻轻活动到舒适的位置。",
            Self::Walk => "有需要的话，去放个风，顺便离开工位走走。",
            Self::Stand => "站一站，让身体换个姿势。",
            Self::EyeCare => "让眼睛休息一下，看看远处。",
        }
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Water => "水",
            Self::Stretch => "扭",
            Self::Walk => "风",
            Self::Stand => "站",
            Self::EyeCare => "眼",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderMode {
    Strong,
    Gentle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderConfig {
    pub mode: ReminderMode,
    pub enabled: bool,
    pub interval_minutes: u32,
    pub break_seconds: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkPeriod {
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub schema_version: u32,
    pub onboarded: bool,
    #[serde(default)]
    // Retain the persisted key; true now hides the exit button and rejects manual exit.
    pub disable_fullscreen_exit: bool,
    pub reminders: [ReminderConfig; ReminderKind::COUNT],
    /// Monday first.
    pub weekdays: [bool; 7],
    pub periods: Vec<WorkPeriod>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 2,
            onboarded: false,
            disable_fullscreen_exit: false,
            reminders: std::array::from_fn(|i| {
                let (interval_minutes, break_seconds) =
                    [(30, 30), (40, 60), (90, 180), (60, 120), (20, 20)][i];
                ReminderConfig {
                    mode: if i == ReminderKind::EyeCare.index() {
                        ReminderMode::Strong
                    } else {
                        ReminderMode::Gentle
                    },
                    enabled: true,
                    interval_minutes,
                    break_seconds,
                }
            }),
            weekdays: [true, true, true, true, true, false, false],
            periods: vec![
                WorkPeriod {
                    start: 9 * 60,
                    end: 12 * 60,
                },
                WorkPeriod {
                    start: 13 * 60,
                    end: 18 * 60,
                },
            ],
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct ConfigError(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkWindow {
    pub date: NaiveDate,
    pub start: u16,
    pub end: u16,
}

impl Settings {
    pub fn normalized(mut self) -> Result<Self, ConfigError> {
        if self.schema_version != 2 {
            return Err(ConfigError("设置版本不受支持，请使用兼容版本打开。".into()));
        }
        if !self.weekdays.iter().any(|v| *v) {
            return Err(ConfigError("请选择至少一个提醒日。".into()));
        }
        if self.periods.is_empty() || self.periods.len() > 4 {
            return Err(ConfigError("请配置 1–4 个工作时段。".into()));
        }
        self.periods.sort_by_key(|p| p.start);
        let mut periods: Vec<WorkPeriod> = Vec::new();
        for p in self.periods {
            if p.start >= p.end || p.end > 1440 {
                return Err(ConfigError(
                    "工作时段结束必须晚于开始；跨午夜请拆成两段。".into(),
                ));
            }
            if let Some(last) = periods.last_mut() {
                if p.start < last.end {
                    return Err(ConfigError("工作时段不能重叠。".into()));
                }
                if p.start == last.end {
                    last.end = p.end;
                    continue;
                }
            }
            periods.push(p);
        }
        self.periods = periods;
        for (i, r) in self.reminders.iter().enumerate() {
            if !(5..=240).contains(&r.interval_minutes) {
                return Err(ConfigError(format!(
                    "{}的间隔须为 5–240 分钟。",
                    ReminderKind::ALL[i].title()
                )));
            }
            if !(5..=1800).contains(&r.break_seconds) {
                return Err(ConfigError(format!(
                    "{}的休息时长须为 5–1800 秒。",
                    ReminderKind::ALL[i].title()
                )));
            }
        }
        Ok(self)
    }
    pub fn work_window(&self, now: DateTime<FixedOffset>) -> Option<WorkWindow> {
        if !self.weekdays[now.weekday().num_days_from_monday() as usize] {
            return None;
        }
        let minute = (now.hour() * 60 + now.minute()) as u16;
        self.periods
            .iter()
            .find(|p| p.start <= minute && minute < p.end)
            .map(|p| WorkWindow {
                date: now.date_naive(),
                start: p.start,
                end: p.end,
            })
    }
    pub fn window_end(&self, now: DateTime<FixedOffset>) -> Option<DateTime<FixedOffset>> {
        self.work_window(now)
            .map(|w| midnight(now) + Duration::minutes(w.end.into()))
    }
    pub fn next_start(
        &self,
        now: DateTime<FixedOffset>,
        next_day_only: bool,
    ) -> DateTime<FixedOffset> {
        for day in 0..=7 {
            if next_day_only && day == 0 {
                continue;
            }
            let date = midnight(now) + Duration::days(day);
            if !self.weekdays[date.weekday().num_days_from_monday() as usize] {
                continue;
            }
            for p in &self.periods {
                let start = date + Duration::minutes(p.start.into());
                if start > now {
                    return start;
                }
            }
        }
        now + Duration::days(1)
    }
}

pub fn midnight(now: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
    now.with_hour(0)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap()
}
