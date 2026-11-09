// FilePath: src-tauri/src/features/health/store.rs
//! The observed facts behind the app's health, shared by everything that observes one. A
//! change wakes the publisher (see `super::spawn`), which derives the issues and tells the UI,
//! the tray and the log; code that only holds `&AppState` (transcription, the capture worker)
//! records facts here without needing the app handle.

use std::sync::Mutex;

use sv_domain::{evaluate, Health, HealthFacts, Platform};
use tokio::sync::Notify;

use crate::state::lock;

#[derive(Debug, Default)]
pub(crate) struct HealthStore {
    facts: Mutex<HealthFacts>,
    /// The health last published, so only a change is announced.
    published: Mutex<Health>,
    changed: Notify,
}

impl HealthStore {
    /// Applies `change` to the facts and wakes the publisher when they differ.
    pub(crate) fn update(&self, change: impl FnOnce(&mut HealthFacts)) {
        let changed = {
            let mut facts = lock(&self.facts);
            let before = facts.clone();
            change(&mut facts);
            *facts != before
        };
        if changed {
            self.changed.notify_one();
        }
    }

    pub(crate) fn facts(&self) -> HealthFacts {
        lock(&self.facts).clone()
    }

    /// The health the facts imply right now (not waiting for the publisher).
    pub(crate) fn current(&self) -> Health {
        evaluate(&lock(&self.facts), Platform::current())
    }

    /// Waits until a fact changes.
    pub(crate) async fn changed(&self) {
        self.changed.notified().await;
    }

    /// Records `health` as published; returns whether it differs from the last one.
    pub(crate) fn publish(&self, health: &Health) -> bool {
        let mut published = lock(&self.published);
        if *published == *health {
            return false;
        }
        *published = health.clone();
        true
    }
}

#[cfg(test)]
mod tests {
    use sv_domain::{InputState, IssueKind};

    use super::*;

    #[tokio::test]
    async fn a_change_wakes_the_publisher_and_publishes_once() {
        let store = HealthStore::default();
        store.update(|facts| facts.input = InputState::NoDevice);
        // The permit stored by notify_one makes this return at once.
        store.changed().await;
        let health = store.current();
        assert_eq!(health.issues[0].kind, IssueKind::NoMicrophone);
        assert!(store.publish(&health));
        assert!(!store.publish(&health));
        assert!(store.publish(&Health::default()));
    }

    #[test]
    fn an_update_that_changes_nothing_is_not_a_change() {
        let store = HealthStore::default();
        store.update(|facts| facts.input = InputState::Unknown);
        assert_eq!(store.facts(), HealthFacts::default());
    }
}
