use gulu_storage::{FloatingPosition, UiPreferences};

/// A single writer with an acknowledged baseline. Movement can arrive while saving.
pub struct Preferences {
    pub current: UiPreferences,
    saved: UiPreferences,
    pending: Option<(u64, UiPreferences)>,
    dirty: bool,
    revision: u64,
}
impl Preferences {
    pub fn new(prefs: UiPreferences) -> Self {
        Self {
            saved: prefs.clone(),
            current: prefs,
            pending: None,
            dirty: false,
            revision: 0,
        }
    }
    pub fn saving(&self) -> bool {
        self.pending.is_some()
    }
    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        if self.saving() || self.current.floating_enabled == enabled {
            return false;
        }
        self.current.floating_enabled = enabled;
        self.dirty = true;
        true
    }
    pub fn set_position(&mut self, position: Option<FloatingPosition>) -> bool {
        if self.current.floating_position == position {
            return false;
        }
        self.current.floating_position = position;
        self.dirty = true;
        true
    }
    pub fn begin_save(&mut self) -> Option<(u64, UiPreferences)> {
        if self.saving() || !self.dirty {
            return None;
        }
        self.revision += 1;
        let request = (self.revision, self.current.clone());
        self.pending = Some(request.clone());
        self.dirty = false;
        Some(request)
    }
    pub fn final_save(&mut self) -> Option<(u64, UiPreferences)> {
        if !self.dirty {
            return None;
        }
        // The storage queue is FIFO: enqueue the latest move before the quit barrier,
        // even when the previous write has not been acknowledged on the UI thread.
        self.pending = None;
        self.begin_save()
    }
    pub fn finish_save(&mut self, revision: u64, success: bool) {
        if self.pending.as_ref().is_none_or(|(id, _)| *id != revision) {
            return;
        }
        let (_, saved) = self.pending.take().unwrap();
        if success {
            self.saved = saved;
        } else {
            self.current.floating_enabled = self.saved.floating_enabled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn position(x: f32) -> Option<FloatingPosition> {
        Some(FloatingPosition {
            display_uuid: None,
            x,
            y: 10.,
        })
    }
    #[test]
    fn movement_during_save_is_not_lost_or_allowed_to_reenable_a_hidden_window() {
        let mut p = Preferences::new(UiPreferences::default());
        assert!(p.set_enabled(true));
        let (id, _) = p.begin_save().unwrap();
        assert!(!p.set_enabled(false));
        p.set_position(position(20.));
        p.finish_save(id, true);
        assert!(p.set_enabled(false));
        let (next, prefs) = p.begin_save().unwrap();
        assert!(!prefs.floating_enabled);
        assert_eq!(prefs.floating_position, position(20.));
        p.finish_save(id, true); // stale completion cannot acknowledge the current write
        assert!(p.saving());
        p.finish_save(next, true);
        assert!(p.begin_save().is_none());
    }
    #[test]
    fn failed_toggle_rolls_back_but_failed_movement_does_not_jump() {
        let mut p = Preferences::new(UiPreferences::default());
        p.set_enabled(true);
        p.set_position(position(40.));
        let (id, _) = p.begin_save().unwrap();
        p.finish_save(id, false);
        assert!(!p.current.floating_enabled);
        assert_eq!(p.current.floating_position, position(40.));
        assert!(p.begin_save().is_none()); // no failed-save busy loop
        p.set_position(position(50.));
        assert!(p.begin_save().is_some());
    }
    #[test]
    fn quit_queues_the_last_move_before_the_barrier() {
        let mut p = Preferences::new(UiPreferences::default());
        p.set_position(position(10.));
        let (first, _) = p.begin_save().unwrap();
        p.set_position(position(80.));
        let (last, prefs) = p.final_save().unwrap();
        assert_eq!(prefs.floating_position, position(80.));
        p.finish_save(first, true);
        assert!(p.saving());
        p.finish_save(last, true);
        assert!(p.final_save().is_none());
    }
}
