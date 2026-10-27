//! The fair-play policy, held in one place: computed from your live Lichess games, announced on the bus when it
//! changes, and asked by every service that gives assistance. Running work that must not continue while you play (a
//! review between positions) waits on it.

use std::sync::Arc;

use dameless_core::{
    Error,
    events::{AssistKind, Payload, Policy},
};
use tokio::sync::watch;

use crate::bus::Bus;

/// The current policy and its watchers.
pub struct PolicyState {
    tx: watch::Sender<Policy>,
    bus: Arc<Bus>,
}

impl PolicyState {
    /// Assistance allowed, until a game says otherwise.
    pub fn new(bus: Arc<Bus>) -> Self {
        Self { tx: watch::Sender::new(Policy::default()), bus }
    }

    pub fn current(&self) -> Policy {
        self.tx.borrow().clone()
    }

    /// Recomputes the policy from your live Lichess games (played here or on lichess.org), each `true` when it is
    /// correspondence. Emits `policy.changed` and wakes waiters when the policy changes; returns it.
    pub fn set_live_games(&self, correspondence: &[bool]) -> Policy {
        let policy = Policy::from_live_games(correspondence);
        let changed = self.tx.send_if_modified(|current| {
            if *current == policy {
                return false;
            }
            *current = policy.clone();
            true
        });
        if changed {
            self.bus.emit(Payload::PolicyChanged(policy.clone()));
        }
        policy
    }

    /// Whether this kind of assistance may be given now.
    pub fn allows(&self, kind: AssistKind) -> bool {
        self.tx.borrow().allows(kind)
    }

    /// `Ok` when allowed, else the fair-play error (409) every service answers with.
    pub fn check(&self, kind: AssistKind) -> Result<(), Error> {
        self.tx.borrow().check(kind)
    }

    /// A receiver that sees every change.
    pub fn watch(&self) -> watch::Receiver<Policy> {
        self.tx.subscribe()
    }

    /// Returns once this kind of assistance is allowed: at once if it is now.
    pub async fn wait_allowed(&self, kind: AssistKind) {
        let mut rx = self.tx.subscribe();
        // The sender lives as long as `self`, so waiting cannot fail while we are borrowed.
        let _ = rx.wait_for(|p| p.allows(kind)).await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use dameless_core::events::{Assist, EventKind};

    use super::*;
    use crate::bus::TopicFilter;

    #[test]
    fn changes_are_announced_once() {
        let bus = Arc::new(Bus::new());
        let policy = PolicyState::new(bus.clone());
        assert!(policy.allows(AssistKind::Eval));
        assert_eq!(policy.set_live_games(&[false]).assist, Assist::Off);
        policy.set_live_games(&[false, true]);
        assert_eq!(policy.check(AssistKind::Review).unwrap_err().status(), 409);
        assert_eq!(policy.set_live_games(&[true]).assist, Assist::ExplorerOnly);
        assert!(policy.allows(AssistKind::Explorer) && !policy.allows(AssistKind::Eval));
        policy.set_live_games(&[]);
        assert!(policy.check(AssistKind::Tablebase).is_ok());
        let kinds: Vec<_> = bus.subscribe(0, TopicFilter::default()).backlog.into_iter().map(|e| e.kind).collect();
        // Off, explorer only, allowed: the second Off was no change.
        assert_eq!(kinds, [EventKind::PolicyChanged; 3]);
    }

    #[tokio::test]
    async fn work_waits_until_allowed() {
        let policy = Arc::new(PolicyState::new(Arc::new(Bus::new())));
        policy.wait_allowed(AssistKind::Review).await;
        policy.set_live_games(&[false]);
        let waiter = {
            let policy = policy.clone();
            tokio::spawn(async move { policy.wait_allowed(AssistKind::Review).await })
        };
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiter.is_finished());
        // Correspondence only still forbids reviews.
        policy.set_live_games(&[true]);
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiter.is_finished());
        policy.set_live_games(&[]);
        tokio::time::timeout(Duration::from_secs(1), waiter).await.unwrap().unwrap();
        assert_eq!(policy.watch().borrow().assist, Assist::Allowed);
    }
}
