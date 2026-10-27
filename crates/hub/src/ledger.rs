//! The core ledger: how the machine's cores are shared out.
//!
//! Live play comes first. A game with a Dameless seat, and live evaluation on the board, **reserve** their cores: a
//! reservation is always granted, even when that oversubscribes the machine, because a live game never waits. Work
//! (trials, tunes, reviews, builds) is **granted** what is left after reservations and earlier grants, between
//! `want_min` and `want_max` cores, or nothing yet. Both come back as guards that hand their cores back when dropped,
//! and every change wakes [`Ledger::changed`] so the scheduler can rebalance: shrink a trial at its next round, or
//! start what was waiting.
//!
//! Work waits **in order**. Work that cannot be granted yet waits as a [`CoreRequest`] (what
//! [`crate::WorkContext::cores`] does), keyed by its priority and when it began waiting. While a request of higher
//! priority, or of the same priority and waiting longer, is waiting, nothing behind it is granted, even when the cores
//! it wants would fit: otherwise a long trial that hands its cores back between rounds and asks again at once would
//! take them every time, and a build or review waiting beside it would never run. Reservations never wait and never
//! queue.
//!
//! The ledger also holds the machine's **GPUs** (one, or none, on this machine): a training run is granted the GPU
//! whole ([`Ledger::grant_gpu`]) beside a few cores for its data loader, so a trial can play while a net trains.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use dameless_core::{WorkId, work::Priority};
use serde::Serialize;
use tokio::sync::watch;
use utoipa::ToSchema;

/// The ledger.
pub struct Ledger {
    total: u32,
    gpus: u32,
    state: Mutex<State>,
    changed: watch::Sender<u64>,
}

#[derive(Default)]
struct State {
    next: u64,
    reservations: BTreeMap<u64, (String, u32)>,
    grants: BTreeMap<u64, (WorkId, u32)>,
    gpu_grants: BTreeMap<u64, WorkId>,
    /// Work waiting for cores, first in line first: by priority rank, then by when it began waiting.
    core_waiters: BTreeMap<(u8, u64), CoreWaiter>,
    /// Work waiting for a GPU, in the same order.
    gpu_waiters: BTreeMap<(u8, u64), WorkId>,
}

struct CoreWaiter {
    min: u32,
}

impl State {
    fn reserved(&self) -> u32 {
        self.reservations.values().map(|(_, n)| n).sum()
    }

    fn granted(&self) -> u32 {
        self.grants.values().map(|(_, n)| n).sum()
    }

    /// Whether a core request at `key` must let someone ahead of it go first. A waiter that wants more cores than the
    /// machine has can never be granted, so it holds nobody up.
    fn core_waiter_ahead(&self, key: (u8, u64), total: u32) -> bool {
        self.core_waiters.range(..key).any(|(_, w)| w.min <= total)
    }

    fn gpu_waiter_ahead(&self, key: (u8, u64)) -> bool {
        self.gpu_waiters.range(..key).next().is_some()
    }
}

/// Where a request that has not waited stands in line: behind everyone of its priority or higher.
fn newcomer(priority: Priority) -> (u8, u64) {
    (priority.rank(), u64::MAX)
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
    /// GPUs on the machine (`DAMELESS_GPUS`, else what nvidia-smi finds).
    pub gpus: u32,
    /// The work holding each GPU in use.
    pub gpu_grants: Vec<WorkId>,
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
    /// A ledger of `total` cores (at least one) and no GPU.
    pub fn new(total: u32) -> Arc<Self> {
        Self::with_gpus(total, 0)
    }

    /// A ledger of `total` cores (at least one) and `gpus` GPUs.
    pub fn with_gpus(total: u32, gpus: u32) -> Arc<Self> {
        Arc::new(Self {
            total: total.max(1),
            gpus,
            state: Mutex::new(State::default()),
            changed: watch::Sender::new(0),
        })
    }

    /// GPUs on the machine.
    pub fn gpus(&self) -> u32 {
        self.gpus
    }

    /// Grants work a whole GPU, or nothing while every GPU is held (or there is none), or while work of `priority` or
    /// higher waits for one ([`Ledger::request_gpu`]).
    pub fn grant_gpu(self: &Arc<Self>, work: WorkId, priority: Priority) -> Option<GpuGrant> {
        self.try_gpu(work, newcomer(priority))
    }

    /// Joins the line for a GPU: [`GpuRequest::grant`] grants one when it is this request's turn. Leaves the line when
    /// granted or dropped.
    pub fn request_gpu(self: &Arc<Self>, work: WorkId, priority: Priority) -> GpuRequest {
        let key = {
            let mut s = self.state();
            s.next += 1;
            let key = (priority.rank(), s.next);
            s.gpu_waiters.insert(key, work);
            key
        };
        GpuRequest { ledger: self.clone(), key, work }
    }

    fn try_gpu(self: &Arc<Self>, work: WorkId, key: (u8, u64)) -> Option<GpuGrant> {
        let id = {
            let mut s = self.state();
            if s.gpu_grants.len() as u32 >= self.gpus || s.gpu_waiter_ahead(key) {
                return None;
            }
            s.gpu_waiters.remove(&key);
            s.next += 1;
            let id = s.next;
            s.gpu_grants.insert(id, work);
            id
        };
        self.bump();
        Some(GpuGrant { ledger: self.clone(), id, work })
    }

    /// GPUs free now.
    pub fn free_gpus(&self) -> u32 {
        self.gpus.saturating_sub(self.state().gpu_grants.len() as u32)
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
    /// `want_min` (at least one) are free, or while work of `priority` or higher waits for cores
    /// ([`Ledger::request_cores`]).
    pub fn grant(self: &Arc<Self>, work: WorkId, priority: Priority, want_min: u32, want_max: u32) -> Option<Grant> {
        self.try_cores(work, newcomer(priority), want_min, want_max)
    }

    /// Joins the line for cores: [`CoreRequest::grant`] grants between `want_min` and `want_max` when it is this
    /// request's turn and enough are free. Until then nothing of lower priority, nor of the same priority that asked
    /// later, is granted. Leaves the line when granted or dropped, so a wait that is cancelled holds nobody up.
    pub fn request_cores(
        self: &Arc<Self>,
        work: WorkId,
        priority: Priority,
        want_min: u32,
        want_max: u32,
    ) -> CoreRequest {
        let want_min = want_min.max(1);
        let want_max = want_max.max(want_min);
        let key = {
            let mut s = self.state();
            s.next += 1;
            let key = (priority.rank(), s.next);
            s.core_waiters.insert(key, CoreWaiter { min: want_min });
            key
        };
        CoreRequest { ledger: self.clone(), key, work, want_min, want_max }
    }

    fn try_cores(self: &Arc<Self>, work: WorkId, key: (u8, u64), want_min: u32, want_max: u32) -> Option<Grant> {
        let want_min = want_min.max(1);
        let want_max = want_max.max(want_min);
        let (id, cores) = {
            let mut s = self.state();
            let free = self.total.saturating_sub(s.reserved() + s.granted());
            if free < want_min || s.core_waiter_ahead(key, self.total) {
                return None;
            }
            s.core_waiters.remove(&key);
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
            gpus: self.gpus,
            gpu_grants: s.gpu_grants.values().copied().collect(),
        }
    }

    /// How many work items wait for cores now.
    pub fn waiting_for_cores(&self) -> usize {
        self.state().core_waiters.len()
    }

    /// Whether work of higher priority than `priority` waits for cores now (work it could ever be granted). A long unit
    /// of lower-priority work (a trial's round) ends early on this, so the waiter does not wait out the whole unit.
    pub fn waiting_ahead_of(&self, priority: Priority) -> bool {
        self.state().core_waiters.range(..(priority.rank(), 0)).any(|(_, w)| w.min <= self.total)
    }

    /// A receiver woken whenever a reservation or grant is made or given back, or a waiter leaves the line.
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

    fn release_gpu(&self, id: u64) {
        self.state().gpu_grants.remove(&id);
        self.bump();
    }

    /// Takes a waiter out of the line, waking the others when it was still in it (it may have held them up).
    fn leave_line(&self, key: (u8, u64), gpu: bool) {
        let left = {
            let mut s = self.state();
            if gpu { s.gpu_waiters.remove(&key).is_some() } else { s.core_waiters.remove(&key).is_some() }
        };
        if left {
            self.bump();
        }
    }
}

/// A place in the line for cores ([`Ledger::request_cores`]); leaves it when dropped.
pub struct CoreRequest {
    ledger: Arc<Ledger>,
    key: (u8, u64),
    work: WorkId,
    want_min: u32,
    want_max: u32,
}

impl CoreRequest {
    /// The cores, when it is this request's turn and enough are free; the request then leaves the line.
    pub fn grant(&self) -> Option<Grant> {
        self.ledger.try_cores(self.work, self.key, self.want_min, self.want_max)
    }
}

impl Drop for CoreRequest {
    fn drop(&mut self) {
        self.ledger.leave_line(self.key, false);
    }
}

impl std::fmt::Debug for CoreRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoreRequest").field("work", &self.work).field("rank", &self.key.0).finish()
    }
}

/// A place in the line for a GPU ([`Ledger::request_gpu`]); leaves it when dropped.
pub struct GpuRequest {
    ledger: Arc<Ledger>,
    key: (u8, u64),
    work: WorkId,
}

impl GpuRequest {
    /// The GPU, when it is this request's turn and one is free; the request then leaves the line.
    pub fn grant(&self) -> Option<GpuGrant> {
        self.ledger.try_gpu(self.work, self.key)
    }
}

impl Drop for GpuRequest {
    fn drop(&mut self) {
        self.ledger.leave_line(self.key, true);
    }
}

impl std::fmt::Debug for GpuRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuRequest").field("work", &self.work).field("rank", &self.key.0).finish()
    }
}

/// A GPU granted to work (a training run); given back when dropped.
pub struct GpuGrant {
    ledger: Arc<Ledger>,
    id: u64,
    work: WorkId,
}

impl GpuGrant {
    pub fn work(&self) -> WorkId {
        self.work
    }
}

impl Drop for GpuGrant {
    fn drop(&mut self) {
        self.ledger.release_gpu(self.id);
    }
}

impl std::fmt::Debug for GpuGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuGrant").field("work", &self.work).finish()
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

    const P: Priority = Priority::Review;

    #[test]
    fn work_takes_what_live_play_leaves() {
        let ledger = Ledger::new(8);
        let game = ledger.reserve("game:1", 2);
        assert_eq!(ledger.free(), 6);
        let trial = ledger.grant(WorkId(1), P, 2, 100).unwrap();
        assert_eq!(trial.cores(), 6);
        assert!(ledger.grant(WorkId(2), P, 1, 4).is_none());
        drop(game);
        let review = ledger.grant(WorkId(2), P, 1, 4).unwrap();
        assert_eq!((review.work(), review.cores()), (WorkId(2), 2));
        drop(trial);
        assert_eq!(ledger.free(), 6);
        // A grant never exceeds what was asked.
        assert_eq!(ledger.grant(WorkId(3), P, 1, 3).unwrap().cores(), 3);
        // Asking for no minimum still means at least one.
        assert_eq!(ledger.grant(WorkId(4), P, 0, 0).unwrap().cores(), 1);
    }

    #[test]
    fn live_play_never_waits() {
        let ledger = Ledger::new(4);
        let _trial = ledger.grant(WorkId(1), P, 2, 4).unwrap();
        let eval = ledger.reserve("eval:abc", 1);
        let game = ledger.reserve("game:7", 2);
        let s = ledger.snapshot();
        assert_eq!((s.total, s.reserved, s.granted, s.free, s.oversubscribed), (4, 3, 4, 0, true));
        assert_eq!(s.reservations.len(), 2);
        assert_eq!(s.grants, [GrantInfo { work: WorkId(1), cores: 4 }]);
        assert_eq!((eval.cores(), game.cores()), (1, 2));
        assert!(ledger.grant(WorkId(2), P, 1, 1).is_none());
    }

    #[test]
    fn the_gpu_is_granted_whole_to_one_work_at_a_time() {
        let ledger = Ledger::with_gpus(4, 1);
        let rx = ledger.changed();
        let gpu = ledger.grant_gpu(WorkId(1), P).unwrap();
        assert!(rx.has_changed().unwrap());
        assert_eq!((gpu.work(), ledger.free_gpus()), (WorkId(1), 0));
        assert!(ledger.grant_gpu(WorkId(2), P).is_none());
        // Cores are a separate account.
        assert_eq!(ledger.grant(WorkId(1), P, 2, 4).unwrap().cores(), 4);
        let s = ledger.snapshot();
        assert_eq!((s.gpus, s.gpu_grants.clone()), (1, vec![WorkId(1)]));
        drop(gpu);
        assert_eq!(ledger.free_gpus(), 1);
        assert!(ledger.grant_gpu(WorkId(2), P).is_some());
        assert!(Ledger::new(4).grant_gpu(WorkId(3), P).is_none(), "no GPU, no grant");
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

    #[test]
    fn a_lower_priority_request_waits_behind_a_higher_one() {
        let ledger = Ledger::new(4);
        // A trial holds every core; a build asks for cores and waits.
        let trial = ledger.grant(WorkId(1), Priority::Idea, 1, 4).unwrap();
        assert!(!ledger.waiting_ahead_of(Priority::Idea));
        let build = ledger.request_cores(WorkId(2), Priority::Interactive, 1, 4);
        assert!(build.grant().is_none());
        assert_eq!(ledger.waiting_for_cores(), 1);
        // The trial can see the build waiting ahead of it, and end its round early; the build sees nobody ahead.
        assert!(ledger.waiting_ahead_of(Priority::Idea) && ledger.waiting_ahead_of(Priority::Review));
        assert!(!ledger.waiting_ahead_of(Priority::Interactive));
        // The trial hands its cores back between rounds and asks again at once: the build goes first.
        let rx = ledger.changed();
        drop(trial);
        assert!(rx.has_changed().unwrap());
        assert!(ledger.grant(WorkId(1), Priority::Idea, 1, 4).is_none(), "the trial must not jump the build");
        let trial_waits = ledger.request_cores(WorkId(1), Priority::Idea, 1, 4);
        assert!(trial_waits.grant().is_none());
        let b = build.grant().unwrap();
        assert_eq!(b.cores(), 4);
        assert_eq!(ledger.waiting_for_cores(), 1, "the build left the line when granted");
        drop(build);
        assert_eq!(ledger.waiting_for_cores(), 1);
        // Work of higher priority than the trial is not held up by it.
        drop(b);
        let review = ledger.grant(WorkId(3), Priority::Review, 1, 1).unwrap();
        assert_eq!(review.cores(), 1);
        // The trial gets the rest once nobody is ahead of it.
        assert_eq!(trial_waits.grant().unwrap().cores(), 3);
        assert_eq!(ledger.waiting_for_cores(), 0);
        // Live play never queues, whoever waits.
        let _queued = ledger.request_cores(WorkId(4), Priority::Interactive, 1, 1);
        assert_eq!(ledger.reserve("game:1", 2).cores(), 2);
    }

    #[test]
    fn equal_priority_is_first_come_first_served() {
        let ledger = Ledger::new(2);
        let holder = ledger.grant(WorkId(9), P, 2, 2).unwrap();
        let first = ledger.request_cores(WorkId(1), P, 2, 2);
        let second = ledger.request_cores(WorkId(2), P, 1, 1);
        drop(holder);
        // The second would fit in what is free, but the first asked earlier.
        assert!(second.grant().is_none());
        assert!(ledger.grant(WorkId(3), P, 1, 1).is_none(), "a newcomer of the same priority waits too");
        let a = first.grant().unwrap();
        assert_eq!(a.work(), WorkId(1));
        drop(a);
        assert_eq!(second.grant().unwrap().work(), WorkId(2));
        // A request asking for more than the machine has can never be granted, so it holds nobody up.
        let _never = ledger.request_cores(WorkId(5), Priority::Interactive, 3, 3);
        assert!(ledger.grant(WorkId(6), Priority::Idea, 1, 1).is_some());
        assert!(!ledger.waiting_ahead_of(Priority::Idea), "nor makes anyone end a unit early");
    }

    #[test]
    fn a_waiter_that_gives_up_holds_nobody_up() {
        let ledger = Ledger::new(2);
        let holder = ledger.grant(WorkId(9), P, 2, 2).unwrap();
        let build = ledger.request_cores(WorkId(1), Priority::Interactive, 1, 2);
        let trial = ledger.request_cores(WorkId(2), Priority::Idea, 1, 2);
        drop(holder);
        assert!(trial.grant().is_none());
        // The build is stopped while it waits: its place in the line goes with it, and the others are woken.
        let mut rx = ledger.changed();
        rx.mark_unchanged();
        drop(build);
        assert!(rx.has_changed().unwrap());
        assert_eq!(trial.grant().unwrap().cores(), 2);
        assert_eq!(ledger.waiting_for_cores(), 0);
    }

    #[test]
    fn the_gpu_goes_to_waiters_in_order() {
        let ledger = Ledger::with_gpus(4, 1);
        let held = ledger.grant_gpu(WorkId(1), Priority::Idea).unwrap();
        let low = ledger.request_gpu(WorkId(2), Priority::Idea);
        let high = ledger.request_gpu(WorkId(3), Priority::Interactive);
        drop(held);
        assert!(low.grant().is_none());
        assert!(ledger.grant_gpu(WorkId(4), Priority::Review).is_none());
        let g = high.grant().unwrap();
        assert_eq!(g.work(), WorkId(3));
        drop(g);
        drop(low);
        assert!(ledger.grant_gpu(WorkId(4), Priority::Review).is_some(), "a dropped waiter leaves the line");
    }
}
