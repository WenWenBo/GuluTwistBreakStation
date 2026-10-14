/// Main-window navigation survives a lock or an active rest without opening a second window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    Restore,
    Home,
    Settings,
}

#[derive(Default)]
pub struct PendingOpen(Option<OpenTarget>);

impl PendingOpen {
    pub fn request(&mut self, target: OpenTarget) {
        // A background error/reopen must not erase an explicit destination.
        if self.0.is_none() || target != OpenTarget::Restore {
            self.0 = Some(target);
        }
    }

    pub fn take_ready(
        &mut self,
        locked: bool,
        resting: bool,
        quitting: bool,
    ) -> Option<OpenTarget> {
        if quitting {
            self.0 = None;
            None
        } else if locked || resting {
            None
        } else {
            self.0.take()
        }
    }
}

pub fn needs_main_on_start(onboarded: bool, tray_available: bool, needs_attention: bool) -> bool {
    !onboarded || !tray_available || needs_attention
}

/// Failure injection is opt-in and only active with diagnostic logging enabled.
pub fn diagnostic_failure(component: &str) -> bool {
    std::env::var_os("GULU_DIAGNOSTICS").is_some()
        && std::env::var("GULU_TEST_FAILURES")
            .is_ok_and(|v| v.split(',').any(|name| name.trim() == component))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_only_opens_for_onboarding_or_an_unusable_resident_entry() {
        for onboarded in [false, true] {
            for tray_available in [false, true] {
                for needs_attention in [false, true] {
                    let show = needs_main_on_start(onboarded, tray_available, needs_attention);
                    if onboarded && tray_available && !needs_attention {
                        assert!(
                            !show,
                            "normal startup must not depend on the floating switch"
                        );
                    } else {
                        assert!(show, "onboarding and recovery must remain reachable");
                    }
                }
            }
        }
    }

    #[test]
    fn deferred_navigation_waits_for_unlock_and_rest_end_and_opens_once() {
        let mut pending = PendingOpen::default();
        pending.request(OpenTarget::Settings);
        pending.request(OpenTarget::Restore);
        assert_eq!(pending.take_ready(true, true, false), None);
        assert_eq!(pending.take_ready(false, true, false), None);
        assert_eq!(pending.take_ready(true, false, false), None);
        assert_eq!(
            pending.take_ready(false, false, false),
            Some(OpenTarget::Settings)
        );
        assert_eq!(pending.take_ready(false, false, false), None);
    }

    #[test]
    fn last_explicit_destination_wins_and_quit_discards_it() {
        let mut pending = PendingOpen::default();
        pending.request(OpenTarget::Home);
        pending.request(OpenTarget::Settings);
        assert_eq!(
            pending.take_ready(false, false, false),
            Some(OpenTarget::Settings)
        );
        pending.request(OpenTarget::Home);
        assert_eq!(pending.take_ready(true, true, true), None);
        assert_eq!(pending.take_ready(false, false, false), None);
    }
}
