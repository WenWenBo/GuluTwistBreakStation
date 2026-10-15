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
        ..Default::default()
    };
    for r in &mut s.reminders {
        r.mode = mode;
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
fn compact_summary_orders_real_times_and_snoozes_without_mutating() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    let water = e.instance(ReminderKind::Water).unwrap().id;
    e.respond(water, Response::Snooze(5), clock(300));
    for kind in [
        ReminderKind::Stretch,
        ReminderKind::Walk,
        ReminderKind::Stand,
        ReminderKind::EyeCare,
    ] {
        let id = e.instance(kind).unwrap().id;
        e.respond(id, Response::Complete, clock(300));
    }
    e.settings.reminders[4].enabled = false;
    e.reminders[1].remaining_ms = 300_001;
    e.settings.reminders[2].enabled = false;
    e.reminders[2].remaining_ms = 1;
    e.reminders[3].remaining_ms = 300_000;
    let expected = ReminderSummary::Upcoming {
        kinds: vec![ReminderKind::Water, ReminderKind::Stand],
        remaining_ms: 300_000,
        snoozed: false,
    };
    assert_eq!(e.reminder_summary(), expected);
    assert_eq!(e.reminder_summary(), expected);
    assert_eq!(e.instance(ReminderKind::Water).unwrap().id, water);
    e.reminders[3].remaining_ms = 300_001;
    assert_eq!(
        e.reminder_summary(),
        ReminderSummary::Upcoming {
            kinds: vec![ReminderKind::Water],
            remaining_ms: 300_000,
            snoozed: true
        }
    );
}
#[test]
fn compact_summary_distinguishes_undelivered_due_and_breaking() {
    let mut e = engine(ReminderMode::Strong);
    e.tick(clock(300));
    assert_eq!(
        e.reminder_summary(),
        ReminderSummary::Due {
            kinds: ReminderKind::ALL.to_vec(),
            notified: false
        }
    );
    e.tick(clock(301));
    assert_eq!(e.reminder_summary(), ReminderSummary::None);
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    assert_eq!(
        e.reminder_summary(),
        ReminderSummary::Due {
            kinds: ReminderKind::ALL.to_vec(),
            notified: true
        }
    );
    e.set_pause(Pause::Manual, clock(300));
    let before = e.reminder_summary();
    e.tick(clock(330));
    assert_eq!(e.reminder_summary(), before);
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
    assert_eq!(s.items.len(), ReminderKind::COUNT);
    assert_eq!(s.duration_seconds, 60);
    assert!(e.tick(clock(340)).is_empty());
    assert_eq!(
        records(&e.tick(clock(361))),
        vec![Outcome::Timed; ReminderKind::COUNT]
    );
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
    assert_eq!(records(&effects), vec![Outcome::Timed; ReminderKind::COUNT]);
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
        assert_eq!(records(&e.tick(c)), vec![outcome; ReminderKind::COUNT]);
    }
}
#[test]
fn wall_edit_does_not_extend_rest() {
    let mut e = engine(ReminderMode::Strong);
    e.tick(clock(300));
    e.tick(clock(301));
    let mut c = clock(331);
    c.wall = clock(1).wall;
    assert_eq!(
        records(&e.tick(c)),
        vec![Outcome::Timed; ReminderKind::COUNT]
    );
}
#[test]
fn gentle_stays_pending_and_duplicate_callback_is_ignored() {
    let mut e = engine(ReminderMode::Gentle);
    let effects = e.tick(clock(300));
    assert!(matches!(effects[0], Effect::Notify(_)));
    let id = e.pending()[0].id;
    assert_eq!(
        records(&e.respond(id, Response::Complete, clock(310))),
        vec![Outcome::Manual]
    );
    assert!(e.respond(id, Response::Complete, clock(310)).is_empty());
    assert!(e.tick(clock(420)).is_empty());
    assert_eq!(e.pending().len(), ReminderKind::COUNT - 1);
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
fn lock_preserves_gentle_pending() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    let id = e.pending()[0].id;
    let mut c = clock(310);
    c.locked = true;
    e.tick(c);
    assert_eq!(e.pending()[0].id, id);
    assert_eq!(e.reminders[0].remaining_ms, 0);
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
    for r in &mut s.reminders {
        r.interval_minutes = 6;
    }
    assert_eq!(
        records(&e.replace_settings(s, clock(310)).unwrap()),
        vec![Outcome::Cancelled; ReminderKind::COUNT]
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
        vec![Outcome::Cancelled; ReminderKind::COUNT]
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
    assert_eq!(
        records(&e.tick(c)),
        vec![Outcome::Timed; ReminderKind::COUNT]
    );
}
#[test]
fn gentle_items_appear_on_time_without_notification_cooldown() {
    let mut e = engine(ReminderMode::Gentle);
    e.reminders[1].remaining_ms = 330_000;
    e.reminders[2].remaining_ms = 340_000;
    e.tick(clock(300));
    for (time, kind) in [(330, ReminderKind::Stretch), (340, ReminderKind::Walk)] {
        let effects = e.tick(clock(time));
        assert!(
            matches!(&effects[0], Effect::Notify(items) if items.len() == 1 && items[0].kind == kind)
        );
    }
    assert!(e.tick(clock(360)).is_empty());
}
#[test]
fn interrupt_resets_merged_and_buffers_other_items() {
    let mut e = engine(ReminderMode::Strong);
    e.reminders[3].remaining_ms = 320_000;
    e.tick(clock(300));
    e.tick(clock(301));
    assert_eq!(
        records(&e.interrupt(clock(305))),
        vec![Outcome::Interrupted; ReminderKind::COUNT - 1]
    );
    assert_eq!(e.reminders[3].remaining_ms, 60_000);
    assert!(e.tick(clock(306)).is_empty());
}
#[test]
fn next_work_period_discards_old_instances() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    assert_eq!(
        records(&e.tick(clock(10800))),
        vec![Outcome::Cancelled; ReminderKind::COUNT]
    );
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
            4 => e.manual_complete(
                ReminderKind::ALL[(seed % ReminderKind::COUNT as u64) as usize],
                now,
            ),
            5 => {
                let mut s = e.settings.clone();
                let r = &mut s.reminders[(seed % ReminderKind::COUNT as u64) as usize];
                r.mode = if r.mode == ReminderMode::Strong {
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
            assert!(s.items.len() <= ReminderKind::COUNT);
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
    assert_eq!(
        records(&e.tick(clock(331))),
        vec![Outcome::Timed; ReminderKind::COUNT]
    );
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

#[test]
fn default_eye_care_is_twenty_minutes_twenty_seconds_and_others_are_gentle() {
    let s = Settings::default();
    let eye = &s.reminders[ReminderKind::EyeCare.index()];
    assert!(eye.enabled);
    assert_eq!(
        (eye.interval_minutes, eye.break_seconds, eye.mode),
        (20, 20, ReminderMode::Strong)
    );
    assert!(
        s.reminders[..4]
            .iter()
            .all(|r| r.mode == ReminderMode::Gentle)
    );
    let mut e = Engine::new(
        Settings {
            onboarded: true,
            ..s
        },
        clock(0),
        Pause::None,
    )
    .unwrap();
    e.tick(clock(1200));
    e.tick(clock(1201));
    let session = e.session.as_ref().unwrap();
    assert_eq!(session.items.len(), 1);
    assert_eq!(session.items[0].kind, ReminderKind::EyeCare);
    assert_eq!(session.duration_seconds, 20);
    let effects = e.tick(clock(1221));
    assert_eq!(records(&effects), vec![Outcome::Timed]);
}

#[test]
fn mixed_due_items_only_force_eye_care_and_restore_gentle_todo() {
    let mut e = engine(ReminderMode::Gentle);
    e.settings.reminders[4].mode = ReminderMode::Strong;
    e.settings.reminders[4].break_seconds = 20;
    let notices = e.tick(clock(300));
    assert!(matches!(&notices[0], Effect::Notify(items) if items.len() == 4));
    let water = e.instance(ReminderKind::Water).unwrap().id;
    e.tick(clock(301));
    assert_eq!(e.session.as_ref().unwrap().items.len(), 1);
    assert_eq!(
        e.session.as_ref().unwrap().items[0].kind,
        ReminderKind::EyeCare
    );
    assert_eq!(records(&e.tick(clock(321))), vec![Outcome::Timed]);
    assert_eq!(e.instance(ReminderKind::Water).unwrap().id, water);
    assert_eq!(e.pending().len(), 4);
    assert!(
        matches!(e.upcoming_summary(), ReminderSummary::Upcoming { kinds, remaining_ms: 300_000, .. } if kinds == vec![ReminderKind::EyeCare])
    );
    assert_eq!(
        records(&e.respond(water, Response::Complete, clock(322))),
        vec![Outcome::Manual]
    );
    assert_eq!(e.reminders[0].remaining_ms, 300_000);
    assert_eq!(e.pending().len(), 3);
    assert!(e.respond(water, Response::Complete, clock(323)).is_empty());
}

#[test]
fn gentle_todos_survive_lock_sleep_pause_but_end_at_work_boundary_and_exit() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    let ids: Vec<_> = e.pending().iter().map(|i| i.id).collect();
    let mut locked = clock(310);
    locked.locked = true;
    assert!(e.tick(locked).is_empty());
    let mut resumed = clock(900);
    resumed.awake_ms = 310_000;
    assert!(e.tick(resumed).is_empty());
    assert!(e.set_pause(Pause::Manual, resumed).is_empty());
    e.tick(clock(3600));
    e.set_pause(Pause::None, clock(3601));
    assert_eq!(e.pending().iter().map(|i| i.id).collect::<Vec<_>>(), ids);
    assert!(e.next_wake_ms() >= 1000);
    assert_eq!(
        records(&e.tick(clock(10800))),
        vec![Outcome::Cancelled; ReminderKind::COUNT]
    );
    assert!(e.pending().is_empty());
    assert!(
        e.respond(ids[0], Response::Complete, clock(10801))
            .is_empty()
    );
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    assert_eq!(
        records(&e.shutdown(clock(310))),
        vec![Outcome::Cancelled; ReminderKind::COUNT]
    );
    assert!(e.pending().is_empty());
    let restarted = Engine::new(e.settings, clock(320), Pause::None).unwrap();
    assert!(restarted.pending().is_empty());
    assert_eq!(restarted.reminders[0].remaining_ms, 300_000);
}

#[test]
fn mode_changes_apply_to_next_instance_and_eye_can_be_gentle() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    let water = e.instance(ReminderKind::Water).unwrap().id;
    assert!(e.instance(ReminderKind::EyeCare).unwrap().gentle);
    let mut settings = e.settings.clone();
    settings.reminders[0].mode = ReminderMode::Strong;
    assert!(e.replace_settings(settings, clock(301)).unwrap().is_empty());
    assert!(e.instance(ReminderKind::Water).unwrap().gentle);
    e.respond(water, Response::Complete, clock(302));
    e.tick(clock(602));
    e.tick(clock(603));
    assert_eq!(e.session.as_ref().unwrap().items.len(), 1);
    assert_eq!(
        e.session.as_ref().unwrap().items[0].kind,
        ReminderKind::Water
    );
    assert!(e.instance(ReminderKind::EyeCare).unwrap().gentle);
}

#[test]
fn exit_permission_is_captured_and_does_not_block_natural_or_global_end() {
    let mut e = engine(ReminderMode::Strong);
    e.settings.disable_fullscreen_exit = true;
    e.tick(clock(300));
    e.tick(clock(301));
    let id = e.session.as_ref().unwrap().id;
    let mut settings = e.settings.clone();
    settings.disable_fullscreen_exit = false;
    assert!(e.replace_settings(settings, clock(302)).unwrap().is_empty());
    assert!(e.session.as_ref().unwrap().disable_exit);
    assert!(records(&e.interrupt_session(id, clock(303))).is_empty());
    assert!(e.session.is_some());
    assert_eq!(
        records(&e.tick(clock(331))),
        vec![Outcome::Timed; ReminderKind::COUNT]
    );
    e.tick(clock(631));
    e.tick(clock(632));
    let next = e.session.as_ref().unwrap().id;
    assert!(!e.session.as_ref().unwrap().disable_exit);
    assert!(records(&e.interrupt_session(id, clock(633))).is_empty());
    assert_eq!(
        records(&e.interrupt_session(next, clock(634))),
        vec![Outcome::Interrupted; ReminderKind::COUNT]
    );

    let mut e = engine(ReminderMode::Strong);
    e.settings.disable_fullscreen_exit = true;
    e.tick(clock(300));
    e.tick(clock(301));
    assert_eq!(
        records(&e.set_pause(Pause::Manual, clock(302))),
        vec![Outcome::Cancelled; ReminderKind::COUNT]
    );
    assert!(e.session.is_none());
}

#[test]
fn exit_setting_preserves_intervals_and_old_serialized_data() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(120));
    let rounds = e.reminders.each_ref().map(|r| r.round);
    let mut settings = e.settings.clone();
    settings.disable_fullscreen_exit = true;
    assert!(e.replace_settings(settings, clock(120)).unwrap().is_empty());
    assert_eq!(e.reminders[0].remaining_ms, 180_000);
    assert_eq!(e.reminders.each_ref().map(|r| r.round), rounds);
    let mut old = serde_json::to_value(&e.settings).unwrap();
    old.as_object_mut()
        .unwrap()
        .remove("disable_fullscreen_exit");
    assert!(
        !serde_json::from_value::<Settings>(old)
            .unwrap()
            .disable_fullscreen_exit
    );
}

#[test]
fn active_list_orders_all_phases_and_filters_disabled() {
    let mut e = engine(ReminderMode::Gentle);
    e.tick(clock(300));
    let water = e.instance(ReminderKind::Water).unwrap().id;
    e.respond(water, Response::Snooze(5), clock(300));
    e.manual_complete(ReminderKind::Stand, clock(300));
    e.settings.reminders[2].enabled = false;
    let items = e.active_reminders();
    assert_eq!(
        items.iter().map(|i| i.kind).collect::<Vec<_>>(),
        vec![
            ReminderKind::Water,
            ReminderKind::Stand,
            ReminderKind::Stretch,
            ReminderKind::EyeCare
        ]
    );
    assert_eq!(items[0].state, ActivityState::Snoozed);
    assert_eq!(items[1].state, ActivityState::Waiting);
    assert_eq!(items[2].state, ActivityState::Pending);
    assert_eq!(items[2].remaining_ms, 0);
    e.set_pause(Pause::Manual, clock(300));
    assert!(e.active_reminders().is_empty());
}

#[test]
fn round_completion_ignores_old_actions_after_other_entry_points_and_boundaries() {
    let mut e = engine(ReminderMode::Gentle);
    let kind = ReminderKind::Water;
    let round = e.reminders[0].round;
    e.tick(clock(300));
    assert_eq!(e.reminders[0].round, round);
    let id = e.instance(kind).unwrap().id;
    e.respond(id, Response::Complete, clock(300));
    let (accepted, effects) = e.complete_round(kind, round, true, clock(302));
    assert!(!accepted);
    assert!(records(&effects).is_empty());
    let next = e.reminders[0].round;
    let (accepted, effects) = e.complete_round(kind, next, true, clock(303));
    assert!(accepted);
    assert_eq!(records(&effects), vec![Outcome::Manual]);
    assert_eq!(e.reminders[0].remaining_ms, 300_000);
    assert!(e.instance(ReminderKind::Stand).is_some());
    assert!(!e.complete_round(kind, next, true, clock(305)).0);
    let round = e.reminders[0].round;
    e.set_pause(Pause::Manual, clock(305));
    assert!(!e.complete_round(kind, round, true, clock(306)).0);
    e.set_pause(Pause::None, clock(307));
    assert!(!e.complete_round(kind, round, true, clock(10_800)).0);
}
