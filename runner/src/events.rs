//! The live event stream. Every event gets a monotonic id, and the last few thousand are kept so a client that
//! reconnects (`Last-Event-ID`, or `?since=` from the CLI) misses nothing.

use std::{collections::VecDeque, sync::Mutex};

use serde::Serialize;
use tokio::sync::broadcast;
use utoipa::ToSchema;

const KEEP: usize = 4096;

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Event {
    pub id: u64,
    /// What happened, such as `job.progress` or `analysis.info`.
    pub kind: String,
    pub data: serde_json::Value,
}

pub struct Events {
    inner: Mutex<Inner>,
    tx: broadcast::Sender<Event>,
}

struct Inner {
    next: u64,
    recent: VecDeque<Event>,
}

impl Events {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(KEEP);
        Self { inner: Mutex::new(Inner { next: 1, recent: VecDeque::with_capacity(KEEP) }), tx }
    }

    pub fn emit(&self, kind: &str, data: serde_json::Value) {
        let mut inner = self.inner.lock().unwrap();
        let event = Event { id: inner.next, kind: kind.to_owned(), data };
        inner.next += 1;
        if inner.recent.len() == KEEP {
            inner.recent.pop_front();
        }
        inner.recent.push_back(event.clone());
        // Sent under the lock, so a subscriber that replays then listens sees every id exactly once.
        let _ = self.tx.send(event);
    }

    /// Events after `since`, then a receiver for what comes next.
    pub fn subscribe(&self, since: u64) -> (Vec<Event>, broadcast::Receiver<Event>) {
        let inner = self.inner.lock().unwrap();
        let backlog = inner.recent.iter().filter(|e| e.id > since).cloned().collect();
        (backlog, self.tx.subscribe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_after_since_then_streams() {
        let events = Events::new();
        events.emit("a", serde_json::json!(1));
        events.emit("b", serde_json::json!(2));
        let (backlog, mut rx) = events.subscribe(1);
        assert_eq!(backlog.iter().map(|e| e.id).collect::<Vec<_>>(), vec![2]);
        events.emit("c", serde_json::json!(3));
        assert_eq!(rx.try_recv().unwrap().id, 3);
    }
}
