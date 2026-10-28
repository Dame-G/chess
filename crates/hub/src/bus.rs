//! The event bus: every event gets a monotonic id, and the last few thousand that last are kept so a client that
//! reconnects (`Last-Event-ID`, or `?since=`) misses nothing. A search's progress and log lines (`game.thinking`,
//! `eval.info`, `work.log`) are sent live but never kept: the next one replaces them, and several a second would push
//! everything else out of the replay.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use dameless_core::events::{Event, Payload, Topic};
use tokio::sync::broadcast;

/// How many events are kept for replay.
pub const KEEP: usize = 4096;

/// An event and the topic it was published on, as the bus passes it to subscribers.
#[derive(Clone, Debug)]
struct Sent {
    event: Event,
    topic: Topic,
}

/// Which topics a subscriber wants.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopicFilter {
    /// Topic patterns (`games`, `game:12`, `game:*`); empty means every topic but evaluation sessions and game detail.
    patterns: Vec<String>,
}

impl TopicFilter {
    /// From `topics=games,game:12,work` (empty or missing: the default).
    pub fn parse(topics: Option<&str>) -> Self {
        let patterns =
            topics.unwrap_or_default().split(',').map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned).collect();
        Self { patterns }
    }

    pub fn of(patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self { patterns: patterns.into_iter().map(Into::into).collect() }
    }

    pub fn matches(&self, topic: &Topic) -> bool {
        if self.patterns.is_empty() { topic.in_default() } else { self.patterns.iter().any(|p| topic.matches(p)) }
    }
}

/// The bus.
pub struct Bus {
    inner: Mutex<Inner>,
    tx: broadcast::Sender<Sent>,
    watchers: Arc<Mutex<Watchers>>,
}

/// The filters of the subscriptions open now, so the bus can say whether anyone listens to a topic.
#[derive(Default)]
struct Watchers {
    next: u64,
    filters: HashMap<u64, TopicFilter>,
}

/// A subscription's entry in [`Watchers`], removed when the subscription is dropped.
struct Watch {
    id: u64,
    watchers: Arc<Mutex<Watchers>>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.watchers.lock().unwrap_or_else(|p| p.into_inner()).filters.remove(&self.id);
    }
}

struct Inner {
    next: u64,
    kept: VecDeque<Sent>,
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

impl Bus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(KEEP);
        Self { inner: Mutex::new(Inner { next: 1, kept: VecDeque::with_capacity(KEEP) }), tx, watchers: Arc::default() }
    }

    /// Publishes an event; returns its id.
    pub fn emit(&self, payload: Payload) -> u64 {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let id = inner.next;
        inner.next += 1;
        let sent = Sent { event: payload.event(id), topic: payload.topic() };
        if payload.kind().kept_for_replay() {
            if inner.kept.len() == KEEP {
                inner.kept.pop_front();
            }
            inner.kept.push_back(sent.clone());
        }
        // Sent under the lock, so a subscriber that replays then listens sees every id exactly once.
        let _ = self.tx.send(sent);
        id
    }

    /// The kept events after `since` that pass `filter`, then a receiver for what comes next.
    ///
    /// An id from before this run (larger than any given out, as after a restart) replays everything kept. When the
    /// events right after `since` are no longer kept, the subscription says so in `gap`, and the client should reload
    /// what it shows rather than trust the replay.
    pub fn subscribe(&self, since: u64, filter: TopicFilter) -> Subscription {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let since = if since >= inner.next { 0 } else { since };
        let oldest = inner.kept.front().map_or(inner.next, |s| s.event.id);
        // A client that has seen nothing yet (`since` 0) has no gap: it loads everything anyway.
        let gap = since > 0 && since + 1 < oldest;
        let backlog = inner
            .kept
            .iter()
            .filter(|s| s.event.id > since && filter.matches(&s.topic))
            .map(|s| s.event.clone())
            .collect();
        let watch = {
            let mut w = self.watchers.lock().unwrap_or_else(|p| p.into_inner());
            w.next += 1;
            let id = w.next;
            w.filters.insert(id, filter.clone());
            Watch { id, watchers: self.watchers.clone() }
        };
        Subscription { backlog, gap, filter, rx: self.tx.subscribe(), _watch: watch }
    }

    /// The id the next event will get.
    pub fn next_id(&self) -> u64 {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).next
    }

    /// How many subscriptions are open now.
    pub fn listeners(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Whether an open subscription would receive events on `topic`: an evaluation session is watched only by a
    /// stream that asked for it (or `eval:*`), not by any stream at all.
    pub fn watched(&self, topic: &Topic) -> bool {
        self.watchers.lock().unwrap_or_else(|p| p.into_inner()).filters.values().any(|f| f.matches(topic))
    }
}

/// What a subscriber receives next.
#[derive(Clone, Debug, PartialEq)]
pub enum Next {
    Event(Event),
    /// The subscriber fell this many events behind and they were dropped: it should reload and resubscribe.
    Lagged(u64),
}

/// One subscriber: the replay, then live events on its topics.
pub struct Subscription {
    /// Kept events after the id asked for, oldest first.
    pub backlog: Vec<Event>,
    /// Some events after the id asked for are no longer kept.
    pub gap: bool,
    filter: TopicFilter,
    rx: broadcast::Receiver<Sent>,
    _watch: Watch,
}

impl Subscription {
    /// The next live event on the subscription's topics; `None` once the bus is gone.
    pub async fn recv(&mut self) -> Option<Next> {
        loop {
            match self.rx.recv().await {
                Ok(sent) if self.filter.matches(&sent.topic) => return Some(Next::Event(sent.event)),
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(n)) => return Some(Next::Lagged(n)),
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// The next event if one is waiting, without waiting.
    pub fn try_recv(&mut self) -> Option<Next> {
        loop {
            match self.rx.try_recv() {
                Ok(sent) if self.filter.matches(&sent.topic) => return Some(Next::Event(sent.event)),
                Ok(_) => continue,
                Err(broadcast::error::TryRecvError::Lagged(n)) => return Some(Next::Lagged(n)),
                Err(_) => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use dameless_core::{
        GameId, Side, WorkId,
        events::{EvalInfo, EventKind, GameThinking, GameUndone, Policy, WorkLog},
    };

    use super::*;

    fn undone(game: i64) -> Payload {
        Payload::GameUndone(GameUndone { id: GameId(game), plies: 1 })
    }

    fn policy() -> Payload {
        Payload::PolicyChanged(Policy::default())
    }

    fn ids(events: &[Event]) -> Vec<u64> {
        events.iter().map(|e| e.id).collect()
    }

    #[test]
    fn replays_after_since_then_streams() {
        let bus = Bus::new();
        assert_eq!(bus.emit(policy()), 1);
        assert_eq!(bus.emit(policy()), 2);
        let mut sub = bus.subscribe(1, TopicFilter::default());
        assert_eq!(ids(&sub.backlog), [2]);
        assert!(!sub.gap);
        bus.emit(policy());
        assert!(matches!(sub.try_recv(), Some(Next::Event(e)) if e.id == 3));
        assert_eq!(bus.listeners(), 1);
        assert_eq!(bus.next_id(), 4);
    }

    #[test]
    fn progress_is_live_only() {
        let bus = Bus::new();
        let mut sub = bus.subscribe(0, TopicFilter::of(["work", "game:*", "eval:*"]));
        bus.emit(Payload::WorkLog(WorkLog { id: WorkId(1), line: "building".into() }));
        bus.emit(Payload::GameThinking(GameThinking {
            id: GameId(1),
            ply: 1,
            side: Side::White,
            depth: 1,
            score: None,
            nodes: 1,
            nps: 1,
            pv: vec![],
        }));
        bus.emit(Payload::EvalInfo(EvalInfo { session: "s".into(), depth: 1, lines: vec![], nodes: 1, nps: 1 }));
        bus.emit(undone(1));
        let live: Vec<EventKind> = std::iter::from_fn(|| sub.try_recv())
            .map(|n| match n {
                Next::Event(e) => e.kind,
                Next::Lagged(_) => panic!("lagged"),
            })
            .collect();
        assert_eq!(live, [EventKind::WorkLog, EventKind::GameThinking, EventKind::EvalInfo, EventKind::GameUndone]);
        let replay = bus.subscribe(0, TopicFilter::of(["work", "game:*", "eval:*"])).backlog;
        assert_eq!(replay.iter().map(|e| (e.id, e.kind)).collect::<Vec<_>>(), [(4, EventKind::GameUndone)]);
    }

    #[test]
    fn topics_filter_the_replay_and_the_stream() {
        let bus = Bus::new();
        bus.emit(undone(1));
        bus.emit(undone(2));
        bus.emit(policy());
        // No topics: everything but game detail and evaluation sessions.
        assert_eq!(ids(&bus.subscribe(0, TopicFilter::parse(None)).backlog), [3]);
        assert_eq!(ids(&bus.subscribe(0, TopicFilter::parse(Some("game:2, policy"))).backlog), [2, 3]);
        assert_eq!(ids(&bus.subscribe(0, TopicFilter::parse(Some("game:*"))).backlog), [1, 2]);
        let mut sub = bus.subscribe(3, TopicFilter::parse(Some("game:1")));
        bus.emit(undone(2));
        bus.emit(undone(1));
        assert!(matches!(sub.try_recv(), Some(Next::Event(e)) if e.id == 5));
        assert!(sub.try_recv().is_none());
    }

    #[test]
    fn keeps_a_bounded_replay_and_reports_gaps() {
        let bus = Bus::new();
        for _ in 0..KEEP + 10 {
            bus.emit(policy());
        }
        let all = bus.subscribe(0, TopicFilter::default());
        assert_eq!(all.backlog.len(), KEEP);
        assert_eq!(all.backlog[0].id, 11);
        assert!(!all.gap);
        assert!(bus.subscribe(5, TopicFilter::default()).gap);
        assert!(!bus.subscribe(10, TopicFilter::default()).gap);
        // An id from a previous run replays everything kept.
        let stale = bus.subscribe(1_000_000, TopicFilter::default());
        assert_eq!(stale.backlog.len(), KEEP);
    }

    #[test]
    fn a_topic_is_watched_only_while_a_subscription_asks_for_it() {
        let bus = Bus::new();
        let session = Topic::Eval("a".into());
        let other = bus.subscribe(0, TopicFilter::parse(Some("games,eval:b")));
        assert!(!bus.watched(&session), "a stream for another session does not keep this one");
        let mine = bus.subscribe(0, TopicFilter::parse(Some("games,eval:a")));
        assert!(bus.watched(&session));
        drop(mine);
        assert!(!bus.watched(&session));
        let all = bus.subscribe(0, TopicFilter::of(["eval:*"]));
        assert!(bus.watched(&session));
        drop((all, other));
        assert!(!bus.watched(&Topic::Games));
    }

    #[tokio::test]
    async fn lagging_subscribers_are_told() {
        let bus = Bus::new();
        let mut sub = bus.subscribe(0, TopicFilter::default());
        for _ in 0..KEEP + 5 {
            bus.emit(policy());
        }
        assert_eq!(sub.recv().await, Some(Next::Lagged(5)));
        assert!(matches!(sub.recv().await, Some(Next::Event(e)) if e.id == 6));
    }
}
