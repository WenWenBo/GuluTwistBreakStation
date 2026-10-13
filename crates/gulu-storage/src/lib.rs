//! Local transactional settings and activity records.
mod ui_preferences;
use chrono::{DateTime, Duration, FixedOffset, NaiveDate};
use gulu_core::{ActivityRecord, BreakSession, Effect, Outcome, Pause, Settings};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
pub use ui_preferences::*;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("本地数据库错误：{0}")]
    Sql(#[from] rusqlite::Error),
    #[error("本地数据无法读取：{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}
pub type Result<T> = std::result::Result<T, StorageError>;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::initialize(Connection::open(path)?)
    }
    pub fn memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }
    fn initialize(connection: Connection) -> Result<Self> {
        connection.busy_timeout(std::time::Duration::from_millis(100))?;
        let version: u32 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            return Err(StorageError::Invalid(
                "数据来自更新版本，未修改原文件。请使用匹配的应用版本。".into(),
            ));
        }
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS activities (
                instance_id TEXT PRIMARY KEY, local_date TEXT NOT NULL,
                ended_unix INTEGER NOT NULL, value TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS activities_day ON activities(local_date, ended_unix);
            PRAGMA user_version=1;
            COMMIT;",
        )?;
        Ok(Self { connection })
    }
    fn preference(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row("SELECT value FROM preferences WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }
    pub fn settings(&self) -> Result<Settings> {
        let Some(json) = self.preference("settings")? else {
            return Ok(Settings::default());
        };
        let (settings, migrated) = decode_settings(&json)?;
        if migrated {
            // Keep the exact old configuration and publish the upgrade atomically.
            // A failed migration must never partially replace the user's settings.
            let tx = self.connection.unchecked_transaction()?;
            tx.execute(
                "INSERT OR IGNORE INTO preferences VALUES ('settings_v1_backup',?1)",
                [&json],
            )?;
            tx.execute(
                "UPDATE preferences SET value=?1 WHERE key='settings'",
                [serde_json::to_string(&settings)?],
            )?;
            tx.commit()?;
        }
        Ok(settings)
    }
    pub fn ui_preferences(&self) -> Result<UiPreferences> {
        let prefs: UiPreferences = self
            .preference("ui_preferences")?
            .map(|json| serde_json::from_str(&json))
            .transpose()?
            .unwrap_or_default();
        prefs
            .validate()
            .map_err(|e| StorageError::Invalid(e.into()))?;
        Ok(prefs)
    }
    pub fn save_ui_preferences(&self, prefs: &UiPreferences) -> Result<()> {
        prefs
            .validate()
            .map_err(|e| StorageError::Invalid(e.into()))?;
        self.connection.execute(
            "INSERT OR REPLACE INTO preferences VALUES ('ui_preferences',?1)",
            [serde_json::to_string(prefs)?],
        )?;
        Ok(())
    }
    pub fn settings_backup(&self) -> Result<Option<Settings>> {
        self.preference("settings_backup")?
            .map(|json| decode_settings(&json).map(|(settings, _)| settings))
            .transpose()
    }
    pub fn save_settings(&mut self, settings: &Settings) -> Result<()> {
        let settings = settings
            .clone()
            .normalized()
            .map_err(|e| StorageError::Invalid(e.to_string()))?;
        let json = serde_json::to_string(&settings)?;
        let old = self.preference("settings")?;
        let tx = self.connection.transaction()?;
        if let Some(old) = old {
            tx.execute(
                "INSERT OR REPLACE INTO preferences VALUES ('settings_backup',?1)",
                [old],
            )?;
        }
        tx.execute(
            "INSERT OR REPLACE INTO preferences VALUES ('settings',?1)",
            [json],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn pause(&self) -> Result<Pause> {
        self.preference("pause")?
            .map(|j| serde_json::from_str(&j).map_err(Into::into))
            .unwrap_or(Ok(Pause::None))
    }
    pub fn save_pause(&self, pause: &Pause) -> Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO preferences VALUES ('pause',?1)",
            [serde_json::to_string(pause)?],
        )?;
        Ok(())
    }
    pub fn commit_effects(
        &mut self,
        effects: &[Effect],
        session: Option<&BreakSession>,
    ) -> Result<()> {
        let tx = self.connection.transaction()?;
        for effect in effects {
            if let Effect::Record(record) = effect {
                tx.execute("INSERT OR IGNORE INTO activities(instance_id,local_date,ended_unix,value) VALUES (?1,?2,?3,?4)",params![record.instance_id.to_string(),record.local_date,record.ended_at.timestamp_millis(),serde_json::to_string(record)?])?;
            }
        }
        if let Some(session) = session {
            tx.execute(
                "INSERT OR REPLACE INTO preferences VALUES ('active_session',?1)",
                [serde_json::to_string(session)?],
            )?;
        } else {
            tx.execute("DELETE FROM preferences WHERE key='active_session'", [])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn recover_session(&mut self, now: DateTime<FixedOffset>) -> Result<usize> {
        let Some(json) = self.preference("active_session")? else {
            return Ok(0);
        };
        let session: BreakSession = serde_json::from_str(&json)?;
        let count = session.items.len();
        let effects: Vec<_> = session
            .items
            .into_iter()
            .map(|item| {
                Effect::Record(ActivityRecord {
                    id: item.id,
                    instance_id: item.id,
                    session_id: Some(session.id),
                    kind: item.kind,
                    outcome: Outcome::Interrupted,
                    ended_at: now,
                    local_date: now.date_naive().to_string(),
                    reason: "上次应用异常退出，已重新开始计时".into(),
                })
            })
            .collect();
        self.commit_effects(&effects, None)?;
        Ok(count)
    }
    pub fn records(&self, date: NaiveDate) -> Result<Vec<ActivityRecord>> {
        let mut statement = self
            .connection
            .prepare("SELECT value FROM activities WHERE local_date=?1 ORDER BY ended_unix DESC")?;
        let jsons = statement.query_map([date.to_string()], |r| r.get::<_, String>(0))?;
        let mut records = Vec::new();
        for json in jsons {
            records.push(serde_json::from_str(&json?)?);
        }
        Ok(records)
    }
    pub fn clear_records(&self) -> Result<()> {
        self.connection.execute("DELETE FROM activities", [])?;
        Ok(())
    }
    pub fn prune(&self, today: NaiveDate) -> Result<()> {
        let earliest = (today - Duration::days(29)).to_string();
        self.connection
            .execute("DELETE FROM activities WHERE local_date < ?1", [earliest])?;
        Ok(())
    }
}

/// Migrate only the recognized v1 shape. Malformed and future data stay untouched.
fn decode_settings(json: &str) -> Result<(Settings, bool)> {
    let mut value: serde_json::Value = serde_json::from_str(json)?;
    let version = value.get("schema_version").and_then(|v| v.as_u64());
    let migrated = match version {
        Some(1) => {
            let object = value
                .as_object_mut()
                .ok_or_else(|| StorageError::Invalid("设置格式无效".into()))?;
            let old_mode = object
                .remove("mode")
                .ok_or_else(|| StorageError::Invalid("旧设置缺少提醒方式".into()))?;
            let _: gulu_core::ReminderMode = serde_json::from_value(old_mode)?;
            let reminders = object
                .get_mut("reminders")
                .and_then(|v| v.as_array_mut())
                .filter(|items| items.len() == 4)
                .ok_or_else(|| StorageError::Invalid("旧设置必须包含四项提醒".into()))?;
            for item in reminders.iter_mut() {
                let item = item
                    .as_object_mut()
                    .ok_or_else(|| StorageError::Invalid("旧提醒格式无效".into()))?;
                item.insert("mode".into(), serde_json::json!("gentle"));
            }
            reminders.push(serde_json::to_value(
                &Settings::default().reminders[gulu_core::ReminderKind::EyeCare.index()],
            )?);
            object.insert("schema_version".into(), serde_json::json!(2));
            true
        }
        Some(2) => false,
        _ => {
            return Err(StorageError::Invalid(
                "设置版本不受支持，原数据未修改。".into(),
            ));
        }
    };
    let settings = serde_json::from_value::<Settings>(value)?
        .normalized()
        .map_err(|e| StorageError::Invalid(e.to_string()))?;
    Ok((settings, migrated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gulu_core::{ClockSample, Engine, ReminderKind};

    #[test]
    fn ui_preferences_are_independent_and_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.db");
        let mut store = Store::open(&path).unwrap();
        assert_eq!(store.ui_preferences().unwrap(), UiPreferences::default());
        store.save_settings(&Settings::default()).unwrap();
        store.save_pause(&Pause::Manual).unwrap();
        let prefs = UiPreferences {
            floating_enabled: true,
            floating_position: Some(FloatingPosition {
                display_uuid: Some("test-screen".into()),
                x: -10.,
                y: 60.,
            }),
            ..Default::default()
        };
        store.save_ui_preferences(&prefs).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.ui_preferences().unwrap(), prefs);
        assert_eq!(store.settings().unwrap(), Settings::default());
        assert!(store.settings_backup().unwrap().is_none());
        assert_eq!(store.pause().unwrap(), Pause::Manual);
    }
    #[test]
    fn invalid_or_unwritable_ui_preferences_preserve_existing_data() {
        let store = Store::memory().unwrap();
        store
            .save_ui_preferences(&UiPreferences::default())
            .unwrap();
        let future = UiPreferences {
            schema_version: 2,
            ..Default::default()
        };
        assert!(store.save_ui_preferences(&future).is_err());
        assert_eq!(store.ui_preferences().unwrap(), UiPreferences::default());
        store
            .connection
            .execute_batch("PRAGMA query_only=ON")
            .unwrap();
        assert!(
            store
                .save_ui_preferences(&UiPreferences {
                    floating_enabled: true,
                    ..Default::default()
                })
                .is_err()
        );
        assert_eq!(store.ui_preferences().unwrap(), UiPreferences::default());
        store.connection.execute_batch("PRAGMA query_only=OFF; UPDATE preferences SET value='broken' WHERE key='ui_preferences'").unwrap();
        assert!(store.ui_preferences().is_err());
        assert_eq!(store.settings().unwrap(), Settings::default());
        assert_eq!(
            store.preference("ui_preferences").unwrap().as_deref(),
            Some("broken")
        );
    }

    fn legacy_settings() -> String {
        serde_json::json!({
            "schema_version": 1, "onboarded": true, "mode": "strong",
            "reminders": [
                {"enabled": true, "interval_minutes": 17, "break_seconds": 30},
                {"enabled": false, "interval_minutes": 45, "break_seconds": 60},
                {"enabled": true, "interval_minutes": 120, "break_seconds": 180},
                {"enabled": true, "interval_minutes": 60, "break_seconds": 120}
            ],
            "weekdays": [true,true,true,true,true,false,false],
            "periods": [{"start":540,"end":1080}]
        })
        .to_string()
    }
    #[test]
    fn legacy_migration_preserves_original_and_does_not_override_later_choices() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("upgrade.db");
        let mut store = Store::open(&path).unwrap();
        let old = legacy_settings();
        store
            .connection
            .execute("INSERT INTO preferences VALUES ('settings',?1)", [&old])
            .unwrap();
        let mut e = Engine::new(
            Settings {
                onboarded: true,
                ..Default::default()
            },
            clock(),
            Pause::None,
        )
        .unwrap();
        store
            .commit_effects(&e.manual_complete(ReminderKind::Water, clock()), None)
            .unwrap();
        let mut settings = store.settings().unwrap();
        assert_eq!(settings.schema_version, 2);
        assert_eq!(settings.reminders[0].interval_minutes, 17);
        assert!(!settings.reminders[1].enabled);
        assert!(
            settings.reminders[..4]
                .iter()
                .all(|r| r.mode == gulu_core::ReminderMode::Gentle)
        );
        assert_eq!(settings.reminders[4], Settings::default().reminders[4]);
        assert_eq!(
            store.preference("settings_v1_backup").unwrap().unwrap(),
            old
        );
        assert_eq!(store.records(clock().wall.date_naive()).unwrap().len(), 1);
        settings.reminders[0].mode = gulu_core::ReminderMode::Strong;
        settings.reminders[4].mode = gulu_core::ReminderMode::Gentle;
        store.save_settings(&settings).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.settings().unwrap(), settings);
        assert_eq!(
            store.preference("settings_v1_backup").unwrap().unwrap(),
            old
        );
    }
    #[test]
    fn invalid_future_and_failed_migrations_do_not_mutate_settings() {
        let store = Store::memory().unwrap();
        let old = legacy_settings();
        store
            .connection
            .execute("INSERT INTO preferences VALUES ('settings',?1)", [&old])
            .unwrap();
        store
            .connection
            .execute_batch("PRAGMA query_only=ON")
            .unwrap();
        assert!(store.settings().is_err());
        assert_eq!(store.preference("settings").unwrap().unwrap(), old);
        assert!(store.preference("settings_v1_backup").unwrap().is_none());
        store
            .connection
            .execute_batch("PRAGMA query_only=OFF")
            .unwrap();
        for value in [
            old.replace("\"schema_version\":1", "\"schema_version\":99"),
            old.replace("\"interval_minutes\":17", "\"interval_minutes\":0"),
        ] {
            store
                .connection
                .execute(
                    "UPDATE preferences SET value=?1 WHERE key='settings'",
                    [&value],
                )
                .unwrap();
            assert!(store.settings().is_err());
            assert_eq!(store.preference("settings").unwrap().unwrap(), value);
            assert!(store.preference("settings_v1_backup").unwrap().is_none());
        }
    }

    fn clock() -> ClockSample {
        ClockSample {
            wall: DateTime::parse_from_rfc3339("2026-10-13T10:00:00+08:00").unwrap(),
            awake_ms: 0,
            continuous_ms: 0,
            locked: false,
        }
    }
    #[test]
    fn settings_survive_reopen_and_have_previous_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let mut store = Store::open(&path).unwrap();
        let mut settings = Settings::default();
        store.save_settings(&settings).unwrap();
        settings.onboarded = true;
        store.save_settings(&settings).unwrap();
        drop(store);
        let store = Store::open(&path).unwrap();
        assert!(store.settings().unwrap().onboarded);
        assert!(!store.settings_backup().unwrap().unwrap().onboarded);
    }
    #[test]
    fn replay_is_idempotent_and_clearing_records_preserves_settings() {
        let mut store = Store::memory().unwrap();
        let settings = Settings {
            onboarded: true,
            ..Default::default()
        };
        store.save_settings(&settings).unwrap();
        let mut engine = Engine::new(settings, clock(), Pause::None).unwrap();
        let effects = engine.manual_complete(ReminderKind::Water, clock());
        store.commit_effects(&effects, None).unwrap();
        store.commit_effects(&effects, None).unwrap();
        assert_eq!(store.records(clock().wall.date_naive()).unwrap().len(), 1);
        store.clear_records().unwrap();
        assert!(store.records(clock().wall.date_naive()).unwrap().is_empty());
        assert!(store.settings().unwrap().onboarded);
    }
    #[test]
    fn invalid_settings_do_not_replace_last_good_configuration() {
        let mut store = Store::memory().unwrap();
        let valid = Settings::default();
        store.save_settings(&valid).unwrap();
        let mut bad = valid.clone();
        bad.reminders[0].interval_minutes = 0;
        assert!(store.save_settings(&bad).is_err());
        assert_eq!(store.settings().unwrap(), valid);
    }
    #[test]
    fn malformed_settings_are_reported_not_silently_reset() {
        let store = Store::memory().unwrap();
        store
            .connection
            .execute("INSERT INTO preferences VALUES('settings','broken')", [])
            .unwrap();
        assert!(store.settings().is_err());
        assert_eq!(store.preference("settings").unwrap().unwrap(), "broken");
    }
    #[test]
    fn crashed_session_recovery_is_idempotent() {
        let mut store = Store::memory().unwrap();
        let settings = Settings {
            onboarded: true,
            ..Default::default()
        };
        let mut engine = Engine::new(settings, clock(), Pause::None).unwrap();
        for r in &mut engine.reminders {
            r.remaining_ms = 0;
        }
        engine.tick(clock());
        let mut c = clock();
        c.awake_ms = 1000;
        c.continuous_ms = 1000;
        c.wall += Duration::seconds(1);
        let effects = engine.tick(c);
        store
            .commit_effects(&effects, engine.session.as_ref())
            .unwrap();
        assert_eq!(store.recover_session(c.wall).unwrap(), 1);
        assert_eq!(store.recover_session(c.wall).unwrap(), 0);
        assert!(
            store
                .records(c.wall.date_naive())
                .unwrap()
                .iter()
                .all(|r| r.outcome == Outcome::Interrupted)
        );
    }
    #[test]
    fn failed_transaction_keeps_settings_and_can_retry() {
        let mut store = Store::memory().unwrap();
        let original = Settings::default();
        store.save_settings(&original).unwrap();
        store
            .connection
            .execute_batch("PRAGMA query_only=ON")
            .unwrap();
        let updated = Settings {
            onboarded: true,
            ..original.clone()
        };
        assert!(store.save_settings(&updated).is_err());
        assert_eq!(store.settings().unwrap(), original);
        store
            .connection
            .execute_batch("PRAGMA query_only=OFF")
            .unwrap();
        store.save_settings(&updated).unwrap();
        assert_eq!(store.settings().unwrap(), updated);
    }
    #[test]
    fn unsupported_schema_is_not_migrated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.db");
        let c = Connection::open(&path).unwrap();
        c.execute_batch("PRAGMA user_version=2").unwrap();
        drop(c);
        assert!(Store::open(&path).is_err());
        let c = Connection::open(&path).unwrap();
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn retention_keeps_today_and_previous_29_days() {
        let mut store = Store::memory().unwrap();
        let settings = Settings {
            onboarded: true,
            ..Default::default()
        };
        let mut e = Engine::new(settings, clock(), Pause::None).unwrap();
        let effects = e.manual_complete(ReminderKind::Water, clock());
        let Effect::Record(base) = &effects[0] else {
            panic!()
        };
        let mut effects = Vec::new();
        for days in [0, 29, 30] {
            let mut record = base.clone();
            record.id = uuid::Uuid::new_v4();
            record.instance_id = record.id;
            record.ended_at -= Duration::days(days);
            record.local_date = record.ended_at.date_naive().to_string();
            effects.push(Effect::Record(record));
        }
        store.commit_effects(&effects, None).unwrap();
        store.prune(clock().wall.date_naive()).unwrap();
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM activities", [], |r| r
                    .get::<_, u32>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn corrupt_database_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.db");
        let bytes = b"not a sqlite database";
        std::fs::write(&path, bytes).unwrap();
        assert!(Store::open(&path).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
