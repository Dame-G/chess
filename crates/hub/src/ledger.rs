//! The core ledger: how the machine's cores are shared out.
//!
//! Live play comes first. A game with a Dameless seat, and live evaluation on the board, **reserve** their cores: a
//! reservation is always granted, even when that oversubscribes the machine, because a live game never waits. Work
//! (trials, tunes, reviews, builds) is **granted** what is left after reservations and earlier grants, between
//! `want_min` and `want_max` cores, or nothing yet. Both come back as guards that hand their cores back when dropped,
//! and every change wakes [`Ledger::changed`] so the scheduler can rebalance: shrink a trial at its next round, or
//! start what was waiting.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use dameless_core::WorkId;
use serde::Serialize;
use tokio::sync::watch;
use utoipa::ToSchema;

/// The ledger.
pub struct Ledger {
    total: u32,
    state: Mutex<State>,
    changed: watch::Sender<u64>,
}

#[derive(Default)]
struct State {
    next: u64,
    reservations: BTreeMap<u64, (String, u32)>,
    grants: BTreeMap<u64, (WorkId, u32)>,
}

impl State {
    fn reserved(&self) -> u32 {
        self.reservations.values().map(|(_, n)| n).sum()
    }

    fn granted(&self) -> u32 {
        self.grants.values().map(|(_, n)| n).sum()
    }
}

/// The ledger as the status line shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct LedgerSnapshot {
    pub total: u32,
    /// Held by live games and live evaluation.
    pub reserved: u32,
    /// Held by work.
    pub granted: u32,
    /// What work can still be granted.
    pub free: u32,
    /// Live play holds more than the machine has, with work on top: games are played anyway, a little slower.
    pub oversubscribed: bool,
    pub reservations: Vec<ReservationInfo>,
    pub grants: Vec<GrantInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct ReservationInfo {
    /// Who holds it, for people: `game:12`, `eval:3f9a`.
    pub owner: String,
    pub cores: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct GrantInfo {
    pub work: WorkId,
    pub cores: u32,
}

impl Ledger {
    /// A ledger of `total` cores (at least one).
    pub fn new(total: u32) -> Arc<Self> {
        Arc::new(Self { total: total.max(1), state: Mutex::new(State::default()), changed: watch::Sender::new(0) })
    }

    pub fn total(&self) -> u32 {
        self.total
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn bump(&self) {
        self.changed.send_modify(|n| *n += 1);
    }

    /// Reserves `cores` for live play. Always granted, at once: when the machine is already full the ledger is
    /// oversubscribed (see [`LedgerSnapshot::oversubscribed`]) until work gives cores back.
    pub fn reserve(self: &Arc<Self>, owner: impl Into<String>, cores: u32) -> Reservation {
        let id = {
            let mut s = self.state();
            s.next += 1;
            let id = s.next;
            s.reservations.insert(id, (owner.into(), cores));
            id
        };
        self.bump();
        Reservation { ledger: self.clone(), id, cores }
    }

    /// Grants work between `want_min` and `want_max` cores from what is free, as many as it can; none while fewer than
    /// `want_min` (at least one) are free.
    pub fn grant(self: &Arc<Self>, work: WorkId, want_min: u32, want_max: u32) -> Option<Grant> {
        let want_min = want_min.max(1);
        let want_max = want_max.max(want_min);
        let (id, cores) = {
            let mut s = self.state();
            let free = self.total.saturating_sub(s.reserved() + s.granted());
            if free < want_min {
                return None;
            }
            let cores = free.min(want_max);
            s.next += 1;
            let id = s.next;
            s.grants.insert(id, (work, cores));
            (id, cores)
        };
        self.bump();
        Some(Grant { ledger: self.clone(), id, work, cores })
    }

    /// Cores work could be granted now.
    pub fn free(&self) -> u32 {
        let s = self.state();
        self.total.saturating_sub(s.reserved() + s.granted())
    }

    pub fn snapshot(&self) -> LedgerSnapshot {
        let s = self.state();
        let (reserved, granted) = (s.reserved(), s.granted());
        LedgerSnapshot {
            total: self.total,
            reserved,
            granted,
            free: self.total.saturating_sub(reserved + granted),
            oversubscribed: reserved + granted > self.total,
            reservations: s
                .reservations
                .values()
                .map(|(owner, cores)| ReservationInfo { owner: owner.clone(), cores: *cores })
                .collect(),
            grants: s.grants.values().map(|(work, cores)| GrantInfo { work: *work, cores: *cores }).collect(),
        }
    }

    /// A receiver woken whenever a reservation or grant is made or given back.
    pub fn changed(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    fn release_reservation(&self, id: u64) {
        self.state().reservations.remove(&id);
        self.bump();
    }

    fn release_grant(&self, id: u64) {
        self.state().grants.remove(&id);
        self.bump();
    }
}

/// Cores held by live play; given back when dropped.
pub struct Reservation {
    ledger: Arc<Ledger>,
    id: u64,
    cores: u32,
}

impl Reservation {
    pub fn cores(&self) -> u32 {
        self.cores
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.ledger.release_reservation(self.id);
    }
}

impl std::fmt::Debug for Reservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reservation").field("cores", &self.cores).finish()
    }
}

/// Cores granted to work; given back when dropped.
pub struct Grant {
    ledger: Arc<Ledger>,
    id: u64,
    work: WorkId,
    cores: u32,
}

impl Grant {
    pub fn cores(&self) -> u32 {
        self.cores
    }

    pub fn work(&self) -> WorkId {
        self.work
    }
}

impl Drop for Grant {
    fn drop(&mut self) {
        self.ledger.release_grant(self.id);
    }
}

impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grant").field("work", &self.work).field("cores", &self.cores).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_takes_what_live_play_leaves() {
        let ledger = Ledger::new(8);
        let game = ledger.reserve("game:1", 2);
        assert_eq!(ledger.free(), 6);
        let trial = ledger.grant(WorkId(1), 2, 100).unwrap();
        assert_eq!(trial.cores(), 6);
        assert!(ledger.grant(WorkId(2), 1, 4).is_none());
        drop(game);
        let review = ledger.grant(WorkId(2), 1, 4).unwrap();
        assert_eq!((review.work(), review.cores()), (WorkId(2), 2));
        drop(trial);
        assert_eq!(ledger.free(), 6);
        // A grant never exceeds what was asked.
        assert_eq!(ledger.grant(WorkId(3), 1, 3).unwrap().cores(), 3);
        // Asking for no minimum still means at least one.
        assert_eq!(ledger.grant(WorkId(4), 0, 0).unwrap().cores(), 1);
    }

    #[test]
    fn live_play_never_waits() {
        let ledger = Ledger::new(4);
        let _trial = ledger.grant(WorkId(1), 2, 4).unwrap();
        let eval = ledger.reserve("eval:abc", 1);
        let game = ledger.reserve("game:7", 2);
        let s = ledger.snapshot();
        assert_eq!((s.total, s.reserved, s.granted, s.free, s.oversubscribed), (4, 3, 4, 0, true));
        assert_eq!(s.reservations.len(), 2);
        assert_eq!(s.grants, [GrantInfo { work: WorkId(1), cores: 4 }]);
        assert_eq!((eval.cores(), game.cores()), (1, 2));
        assert!(ledger.grant(WorkId(2), 1, 1).is_none());
    }

    #[test]
    fn changes_wake_the_scheduler() {
        let ledger = Ledger::new(2);
        let mut rx = ledger.changed();
        assert!(!rx.has_changed().unwrap());
        let r = ledger.reserve("game:1", 1);
        assert!(rx.has_changed().unwrap());
        rx.mark_unchanged();
        drop(r);
        assert!(rx.has_changed().unwrap());
        assert_eq!(Ledger::new(0).total(), 1);
    }
}
