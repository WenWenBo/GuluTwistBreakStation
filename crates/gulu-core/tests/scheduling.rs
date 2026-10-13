use chrono::{DateTime, Duration};
use gulu_core::*;
fn clock(s: u64) -> ClockSample {
    ClockSample {
        wall: DateTime::parse_from_rfc3339("2026-10-13T09:00:00+08:00").unwrap()
            + Duration::seconds(s as i64),
        awake_ms: s * 1000,
        continuous_ms: s * 1000,
        locked: false,
    }
}
fn engine(mode: ReminderMode) -> Engine {
    let mut s = Settings {
        onboarded: true,
        mode,
        ..Default::default()
    };
    for r in &mut s.reminders {
        r.interval_minutes = 5;
        r.break_seconds = 30;
    }
    Engine::new(s, clock(0), Pause::None).unwrap()
}
fn records(e: &[Effect]) -> Vec<Outcome> {
    e.iter()
        .filter_map(|e| {
            if let Effect::Record(r) = e {
                Some(r.outcome.clone())
            } else {
                None
            }
        })
        .collect()
}
#[test]
fn onboarding_does_not_schedule() {
    let mut e = Engine::new(Settings::default(), clock(0), Pause::None).unwrap();
    assert!(e.tick(clock(7200)).is_empty());
}
#[test]
fn merged_session_max_not_sum_and_intervals_freeze() {
    let mut e = engine(ReminderMode::Strong);
    e.settings.reminders[1].break_seconds = 60;
    assert!(e.tick(clock(300)).is_empty());
    let effects = e.tick(clock(301));
    assert!(matches!(effects[0], Effect::OpenBreak(_)));
    let s = e.session.as_ref().unwrap();
    assert_eq!(s.items.len(), 4);
    assert_eq!(s.duration_seconds, 60);
    assert!(e.tick(clock(340)).is_empty());
    assert_eq!(records(&e.tick(clock(361))), vec![Outcome::Timed; 4]);
    assert!(e.session.is_none());
    assert_eq!(e.reminders[0].remaining_ms, 300_000);
}
#[test]
fn lock_freezes_and_unlock_has_buffer() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(280));
    let mut c = clock(290);
    c.locked = true;
    e.tick(c);
    assert_eq!(e.reminders[0].remaining_ms, 10_000);
    e.tick(clock(400));
    assert_eq!(e.reminders[0].remaining_ms, 60_000);
    assert!(e.tick(clock(459)).is_empty());
    assert!(matches!(e.tick(clock(460))[0], Effect::Notify(_)));
}
#[test]
fn sleep_break_completes_at_deadline_once() {
    let mut e = engine(ReminderMode::Strong);
    e.tick(clock(300));
    e.tick(clock(301));
    let mut c = clock(500);
    c.awake_ms = 302_000;
    let effects = e.tick(c);
    assert_eq!(records(&effects), vec![Outcome::Timed; 4]);
    if let Effect::Record(r) = &effects[1] {
        assert_eq!(r.ended_at, clock(331).wall);
    }
    assert!(e.tick(c).is_empty());
}
#[test]
fn boundary_wins_tie_and_sleep_orders_events() {
    for (duration, outcome) in [
        (29, Outcome::Timed),
        (30, Outcome::Cancelled),
        (60, Outcome::Cancelled),
    ] {
        let mut e = engine(ReminderMode::Strong);
        e.settings.periods = vec![WorkPeriod {
            start: 540,
            end: 546,
        }];
        e = Engine::new(e.settings, clock(0), Pause::None).unwrap();
        for r in &mut e.settings.reminders {
            r.break_seconds = duration;
        }
        e.tick(clock(329));
        e.tick(clock(330));
        let mut c = clock(400);
        c.awake_ms = 331_000;
        assert_eq!(records(&e.tick(c)), vec![outcome; 4]);
    }
}
#[test]
fn wall_edit_does_not_extend_rest() {
    let mut e = engine(ReminderMode::Strong);
    e.tick(clock(300));
    e.tick(clock(301));
    let mut c = clock(331);
    c.wall = clock(1).wall;
    assert_eq!(records(&e.tick(c)), vec![Outcome::Timed; 4]);
}
#[test]
fn gentle_expiry_and_duplicate_callback() {
    let mut e = engine(ReminderMode::Gentle);
    let effects = e.tick(clock(300));
    assert!(matches!(effects[0], Effect::Notify(_)));
    let id = e.pending()[0].id;
    assert_eq!(
        records(&e.respond(id, Response::Complete, clock(310))),
        vec![Outcome::Manual]
    );
    assert!(e.respond(id, Response::Complete, clock(310)).is_empty());
    assert_eq!(records(&e.tick(clock(420))), vec![Outcome::Expired; 3]);
}
#[test]
fn snooze_freezes_in_pause() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    let id = e.pending()[0].id;
    e.respond(id, Response::Snooze(5), clock(300));
    e.set_pause(Pause::Manual, clock(350));
    e.tick(clock(500));
    e.set_pause(Pause::None, clock(500));
    assert!(e.tick(clock(749)).is_empty());
    assert!(
        e.tick(clock(750))
            .iter()
            .any(|e| matches!(e, Effect::Notify(_)))
    );
}
#[test]
fn lock_does_not_subtract_elapsed_from_reset_pending() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    let mut c = clock(310);
    c.locked = true;
    e.tick(c);
    assert_eq!(e.reminders[0].remaining_ms, 300_000);
}
#[test]
fn failed_overlay_snooze_stays_gentle() {
    let mut e = engine(ReminderMode::Strong);
    e.tick(clock(300));
    e.tick(clock(301));
    let id = e.session.as_ref().unwrap().id;
    e.overlay_failed(id, clock(301));
    let item = e.pending()[0].id;
    e.respond(item, Response::Snooze(5), clock(301));
    let effects = e.tick(clock(601));
    assert!(effects.iter().any(|x| matches!(x, Effect::Notify(_))));
    assert!(e.session.is_none());
}
#[test]
fn pause_expiry_uses_continuous_clock() {
    let mut e = engine(ReminderMode::Gentle);
    e.set_pause(Pause::Until(clock(60).wall), clock(0));
    let mut c = clock(30);
    c.wall = clock(3600).wall;
    e.tick(c);
    assert_eq!(e.pause, Pause::Until(clock(60).wall));
    c.continuous_ms = 60_000;
    c.awake_ms = 60_000;
    e.tick(c);
    assert_eq!(e.pause, Pause::None);
}
#[test]
fn config_validation_and_adjacent_periods() {
    let s = Settings {
        periods: vec![
            WorkPeriod {
                start: 540,
                end: 600,
            },
            WorkPeriod {
                start: 600,
                end: 660,
            },
        ],
        ..Default::default()
    };
    assert_eq!(s.normalized().unwrap().periods.len(), 1);
    let mut s = Settings::default();
    s.reminders[0].interval_minutes = 4;
    assert!(s.normalized().is_err());
}
#[test]
fn manual_double_click_is_one_record() {
    let mut e = engine(ReminderMode::Strong);
    assert_eq!(
        records(&e.manual_complete(ReminderKind::Water, clock(10))).len(),
        1
    );
    assert!(e.manual_complete(ReminderKind::Water, clock(10)).is_empty());
}
#[test]
fn config_changes_cancel_old_instances() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    let id = e.pending()[0].id;
    let mut s = e.settings.clone();
    s.mode = ReminderMode::Strong;
    assert_eq!(
        records(&e.replace_settings(s, clock(310)).unwrap()),
        vec![Outcome::Cancelled; 4]
    );
    assert!(e.respond(id, Response::Complete, clock(311)).is_empty());
}
#[test]
fn pause_at_exact_deadline_cancels() {
    let mut e = engine(ReminderMode::Strong);
    e.tick(clock(300));
    e.tick(clock(301));
    assert_eq!(
        records(&e.set_pause(Pause::Manual, clock(331))),
        vec![Outcome::Cancelled; 4]
    );
}
#[test]
fn wall_clock_rollback_moves_boundary_but_not_rest_deadline() {
    let mut e = engine(ReminderMode::Strong);
    e.settings.periods = vec![WorkPeriod {
        start: 540,
        end: 546,
    }];
    e = Engine::new(e.settings, clock(0), Pause::None).unwrap();
    for r in &mut e.settings.reminders {
        r.break_seconds = 60;
    }
    e.tick(clock(329));
    e.tick(clock(330));
    let mut c = clock(350);
    c.wall = clock(100).wall;
    e.tick(c);
    c.continuous_ms = 390_000;
    c.awake_ms = 390_000;
    c.wall = clock(140).wall;
    assert_eq!(records(&e.tick(c)), vec![Outcome::Timed; 4]);
}
#[test]
fn notification_cooldown_batches_later_items() {
    let mut e = engine(ReminderMode::Gentle);
    e.reminders[1].remaining_ms = 330_000;
    e.reminders[2].remaining_ms = 340_000;
    assert_eq!(
        e.tick(clock(300))
            .iter()
            .filter(|e| matches!(e, Effect::Notify(_)))
            .count(),
        1
    );
    assert!(e.tick(clock(330)).is_empty());
    assert!(e.tick(clock(340)).is_empty());
    let effects = e.tick(clock(360));
    if let Effect::Notify(items) = &effects[0] {
        assert_eq!(items.len(), 2);
    } else {
        panic!("missing summary");
    }
}
#[test]
fn interrupt_resets_merged_and_buffers_other_items() {
    let mut e = engine(ReminderMode::Strong);
    e.reminders[3].remaining_ms = 320_000;
    e.tick(clock(300));
    e.tick(clock(301));
    assert_eq!(
        records(&e.interrupt(clock(305))),
        vec![Outcome::Interrupted; 3]
    );
    assert_eq!(e.reminders[3].remaining_ms, 60_000);
    assert!(e.tick(clock(306)).is_empty());
}
#[test]
fn next_work_period_discards_old_instances() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    assert_eq!(records(&e.tick(clock(10800))), vec![Outcome::Cancelled; 4]);
    e.tick(clock(14400));
    assert_eq!(e.reminders[0].remaining_ms, 300_000);
}
#[test]
fn ordinary_sleep_preserves_interval() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(100));
    let mut c = clock(1000);
    c.awake_ms = 100_000;
    e.tick(c);
    assert_eq!(e.reminders[0].remaining_ms, 200_000);
}
#[test]
fn mixed_event_sequences_never_commit_instance_twice() {
    let mut e = engine(ReminderMode::Strong);
    let mut now = clock(0);
    let mut seed = 731u64;
    let mut completed = std::collections::HashSet::new();
    for _ in 0..4000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let delta = 1 + (seed % 100);
        now.continuous_ms += delta * 1000;
        now.awake_ms += delta * 1000;
        now.wall += Duration::seconds(delta as i64);
        let effects = match (seed >> 16) % 9 {
            0 => {
                now.locked = !now.locked;
                e.tick(now)
            }
            1 => e.interrupt(now),
            2 => e.set_pause(Pause::Manual, now),
            3 => e.set_pause(Pause::None, now),
            4 => e.manual_complete(ReminderKind::ALL[(seed % 4) as usize], now),
            5 => {
                let mut s = e.settings.clone();
                s.mode = if s.mode == ReminderMode::Strong {
                    ReminderMode::Gentle
                } else {
                    ReminderMode::Strong
                };
                e.replace_settings(s, now).unwrap()
            }
            6 => {
                if let Some(i) = e.pending().first() {
                    e.respond(i.id, Response::Snooze(5), now)
                } else {
                    e.tick(now)
                }
            }
            _ => e.tick(now),
        };
        for effect in effects {
            if let Effect::Record(r) = effect {
                assert!(completed.insert(r.id), "duplicate terminal record");
            }
        }
        if let Some(s) = &e.session {
            let kinds = s
                .items
                .iter()
                .map(|i| i.kind)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(kinds.len(), s.items.len());
            assert!(s.items.len() <= 4);
            assert_eq!(
                s.duration_seconds,
                s.items.iter().map(|i| i.break_seconds).max().unwrap()
            );
        }
    }
}
#[test]
fn changing_duration_only_affects_next_instance() {
    let mut e = engine(ReminderMode::Strong);
    e.tick(clock(300));
    e.tick(clock(301));
    let before = e.session.clone().unwrap();
    let mut s = e.settings.clone();
    s.reminders[0].break_seconds = 1800;
    e.replace_settings(s, clock(305)).unwrap();
    assert_eq!(e.session.as_ref().unwrap().deadline_ms, before.deadline_ms);
    assert_eq!(records(&e.tick(clock(331))), vec![Outcome::Timed; 4]);
}
#[test]
fn next_selected_workday_skips_today_and_weekend() {
    let s = Settings::default();
    let friday = DateTime::parse_from_rfc3339("2026-10-16T10:00:00+08:00").unwrap();
    assert_eq!(
        s.next_start(friday, true).to_rfc3339(),
        "2026-10-19T09:00:00+08:00"
    );
}
#[test]
fn duplicate_snooze_does_not_restart_countdown() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    let id = e.pending()[0].id;
    e.respond(id, Response::Snooze(5), clock(300));
    e.respond(id, Response::Snooze(5), clock(320));
    if let ReminderPhase::Snoozed { remaining_ms, .. } = e.reminders[0].phase {
        assert_eq!(remaining_ms, 280_000);
    } else {
        panic!();
    }
}
#[test]
fn preview_does_not_consume_interval_or_write_records() {
    let mut e = engine(ReminderMode::Strong);
    e.tick(clock(100));
    assert!(e.set_preview(true, clock(100)).is_empty());
    assert!(e.tick(clock(105)).is_empty());
    assert!(e.set_preview(false, clock(105)).is_empty());
    assert_eq!(e.reminders[0].remaining_ms, 200_000);
    e.tick(clock(106));
    assert_eq!(e.reminders[0].remaining_ms, 199_000);
}
