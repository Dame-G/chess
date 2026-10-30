//! The order a review searches positions in: **runs**, each a game's positions in the order they were played, searched
//! one after another on one engine that keeps its hash table between them.
//!
//! At a fixed node budget a search takes as long whatever its table holds, so the order does not make a review faster;
//! it makes it better for the same time. The position after a move is, nearly always, deep inside the tree the search
//! of the position before explored, so its own search starts from what that one found and ends deeper. Each run starts
//! on a clean table (`ucinewgame`), so a search's result depends only on the positions before it in its run.
//!
//! Engines share one [`RunQueue`]: an engine with nothing to do takes the next queued run, and when none is queued it
//! takes the second half of the longest run another engine has left, so a single game is spread over every engine and
//! no engine idles while another has a long way to go.

use std::collections::VecDeque;

/// One position to search, and the run it is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Next<J> {
    pub job: J,
    /// The first position of a run: the engine starts it on a clean table.
    pub fresh: bool,
}

/// Runs waiting and runs being searched, one per engine. Not shared by itself: the caller puts it behind a lock.
#[derive(Debug)]
pub struct RunQueue<J> {
    queued: VecDeque<VecDeque<J>>,
    active: Vec<VecDeque<J>>,
}

impl<J> RunQueue<J> {
    /// A queue for `engines` engines, numbered from 0.
    pub fn new(engines: usize) -> Self {
        Self { queued: VecDeque::new(), active: (0..engines).map(|_| VecDeque::new()).collect() }
    }

    /// Queues a run at the back. An empty one is dropped.
    pub fn push(&mut self, run: impl IntoIterator<Item = J>) {
        let run: VecDeque<J> = run.into_iter().collect();
        if !run.is_empty() {
            self.queued.push_back(run);
        }
    }

    /// Positions not handed out yet, queued or left in a run being searched.
    pub fn len(&self) -> usize {
        self.queued.iter().chain(&self.active).map(VecDeque::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Makes room for `engines` engines (a new batch may have more cores than the last). Engines beyond it must have
    /// given their runs back.
    pub fn resize(&mut self, engines: usize) {
        debug_assert!(self.active.iter().skip(engines).all(VecDeque::is_empty));
        self.active.resize_with(engines, VecDeque::new);
    }

    /// The next position for `engine`: the next of its own run; else the first of the next queued run; else the first
    /// of the second half of the longest run another engine has left (at least two positions left). `None` when there
    /// is nothing left to do.
    pub fn next(&mut self, engine: usize) -> Option<Next<J>> {
        if let Some(job) = self.active[engine].pop_front() {
            return Some(Next { job, fresh: false });
        }
        let run = match self.queued.pop_front() {
            Some(run) => run,
            None => {
                let (longest, len) =
                    self.active.iter().enumerate().map(|(i, r)| (i, r.len())).max_by_key(|&(i, len)| (len, i))?;
                if len < 2 {
                    return None;
                }
                // The busy engine keeps the half it is about to search; this one takes the rest.
                self.active[longest].split_off(len.div_ceil(2))
            }
        };
        self.active[engine] = run;
        let job = self.active[engine].pop_front()?;
        Some(Next { job, fresh: true })
    }

    /// `engine` stops (its batch winds down): what is left of its run goes back to the front of the queue, to be
    /// started again on a clean table.
    pub fn give_back(&mut self, engine: usize) {
        let left = std::mem::take(&mut self.active[engine]);
        if !left.is_empty() {
            self.queued.push_front(left);
        }
    }

    /// Every engine stops: all runs left go back to the queue, in the order they were in.
    pub fn give_back_all(&mut self) {
        for engine in (0..self.active.len()).rev() {
            self.give_back(engine);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(q: &mut RunQueue<u32>, engine: usize) -> Vec<(u32, bool)> {
        std::iter::from_fn(|| q.next(engine).map(|n| (n.job, n.fresh))).collect()
    }

    #[test]
    fn one_engine_searches_each_run_in_order_starting_each_afresh() {
        let mut q = RunQueue::new(1);
        q.push([1, 2, 3]);
        q.push(Vec::<u32>::new());
        q.push([10, 11]);
        assert_eq!(q.len(), 5);
        assert_eq!(drain(&mut q, 0), [(1, true), (2, false), (3, false), (10, true), (11, false)]);
        assert!(q.is_empty());
    }

    #[test]
    fn engines_take_queued_runs_then_split_the_longest_one_left() {
        let mut q = RunQueue::new(3);
        q.push(0..10);
        assert_eq!(q.next(0), Some(Next { job: 0, fresh: true }));
        // Nothing queued: engine 1 takes the back half of what engine 0 has left (1..10, nine positions).
        assert_eq!(q.next(1), Some(Next { job: 6, fresh: true }));
        // Engine 2 splits the longest left: engine 0's 1..6 (five) over engine 1's 7..10 (three).
        assert_eq!(q.next(2), Some(Next { job: 4, fresh: true }));
        // Then engine 0, its own run done, splits engine 1's 7..10 twice: 9, then 8.
        assert_eq!(drain(&mut q, 0), [(1, false), (2, false), (3, false), (9, true), (8, true)]);
        assert_eq!(q.next(1), Some(Next { job: 7, fresh: false }));
        assert_eq!(drain(&mut q, 2), [(5, false)]);
        assert!(q.is_empty() && q.next(1).is_none());
    }

    #[test]
    fn a_run_of_one_is_not_split() {
        let mut q = RunQueue::new(2);
        q.push([1, 2]);
        assert_eq!(q.next(0).unwrap().job, 1);
        assert_eq!(q.next(1), None);
        assert_eq!(q.next(0), Some(Next { job: 2, fresh: false }));
    }

    #[test]
    fn runs_given_back_start_again_first_and_afresh() {
        let mut q = RunQueue::new(2);
        q.push([1, 2, 3]);
        q.push([10, 11, 12]);
        assert_eq!(q.next(0).unwrap().job, 1);
        assert_eq!(q.next(1).unwrap().job, 10);
        q.give_back_all();
        assert_eq!(q.len(), 4);
        q.resize(1);
        assert_eq!(drain(&mut q, 0), [(2, true), (3, false), (11, true), (12, false)]);
        q.resize(4);
        q.push([20]);
        assert_eq!(q.next(3), Some(Next { job: 20, fresh: true }));
    }
}
