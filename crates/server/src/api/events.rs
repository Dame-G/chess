//! `GET /api/events`: the one event stream, as server-sent events.
//!
//! Each event is sent as `id: <id>`, `event: <kind>` and `data: <envelope>`, where the envelope is
//! `{ id, topic, kind, data }` ([`Event`] in the OpenAPI document: a union of every kind with its typed payload). A
//! client resumes with `?since=<id>` or the browser's `Last-Event-ID`: the kept events after it come first, then live
//! ones. When some of the events after that id are no longer kept, or the client fell so far behind that events were
//! dropped, a `reload` event (no id) tells it to fetch what it shows again rather than trust the stream. A comment is
//! sent every 15 seconds to keep idle connections open.

use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    extract::State,
    http::HeaderMap,
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use chess_core::{
    engines::EnginesOverview,
    events::{self as ev, EventKind},
    game::GameSummary,
    lichess::{ChallengeEvent, ConnectionStatus},
    work::Work,
};
use chess_hub::{Next, TopicFilter};
use futures::{Stream, StreamExt, stream};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::error::Query;
use crate::App;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EventsQuery {
    /// Replay the kept events after this id. `Last-Event-ID` does the same for a reconnecting browser.
    pub since: Option<u64>,
    /// Topics, comma-separated (`games,game:12,work`, `game:*`). None: every topic but `eval:*` and game detail.
    pub topics: Option<String>,
    /// The API token, for clients that cannot send headers (`EventSource`).
    pub token: Option<String>,
}

/// The live event stream.
#[utoipa::path(
    get,
    path = "/api/events",
    operation_id = "follow_events",
    tag = "events",
    params(EventsQuery),
    responses((status = 200, description = "A text/event-stream; each `data:` is an `Event`", content_type = "text/event-stream", body = Event))
)]
pub async fn events(
    State(app): State<Arc<App>>,
    Query(q): Query<EventsQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let last = headers.get("last-event-id").and_then(|v| v.to_str().ok()).and_then(|v| v.trim().parse().ok());
    let since = last.or(q.since).unwrap_or(0);
    let mut sub = app.hub.bus.subscribe(since, TopicFilter::parse(q.topics.as_deref()));
    let first: Vec<SseEvent> = std::mem::take(&mut sub.backlog).iter().map(to_sse).collect();
    let gap = sub.gap.then(|| reload("Some events after the one asked for are no longer kept"));
    let live = stream::unfold(sub, |mut sub| async move {
        let sse = match sub.recv().await? {
            Next::Event(e) => to_sse(&e),
            Next::Lagged(n) => reload(&format!("{n} events were dropped while this client fell behind")),
        };
        Some((sse, sub))
    });
    let stream = stream::iter(gap.into_iter().chain(first))
        .chain(live)
        .take_until(app.shutdown.clone().cancelled_owned())
        .map(Ok);
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn to_sse(e: &ev::Event) -> SseEvent {
    let data = serde_json::to_string(e).unwrap_or_else(|_| "{}".into());
    SseEvent::default().id(e.id.to_string()).event(e.kind.as_str()).data(data)
}

/// The `reload` event: fetch again, the stream has a hole.
fn reload(reason: &str) -> SseEvent {
    let e = Event { id: 0, topic: RELOAD_TOPIC.into(), event: EventData::Reload(Reload { reason: reason.into() }) };
    SseEvent::default().event(RELOAD).data(serde_json::to_string(&e).unwrap_or_else(|_| "{}".into()))
}

/// The kind of the event that says the stream has a hole.
pub const RELOAD: &str = "reload";
/// Its topic: the stream itself.
pub const RELOAD_TOPIC: &str = "stream";

/// One event of the stream, as each `data:` line carries it: `{ id, topic, kind, data }`, where `kind` names the
/// payload's type. Every score in a payload is White's.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Event {
    /// Monotonic; pass the last one seen as `since` (or `Last-Event-ID`) to resume. 0 on `reload`.
    pub id: u64,
    /// `game:<id>`, `games`, `work`, `player:<key>`, `connection:you`, `engines`, `policy`, `eval:<session>`; `stream`
    /// on `reload`.
    pub topic: String,
    #[serde(flatten)]
    pub event: EventData,
}

/// Each kind with its payload.
#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(tag = "kind", content = "data")]
pub enum EventData {
    #[serde(rename = "game.upserted")]
    GameUpserted(Box<GameSummary>),
    #[serde(rename = "game.moved")]
    GameMoved(Box<ev::GameMoved>),
    #[serde(rename = "game.undone")]
    GameUndone(ev::GameUndone),
    #[serde(rename = "game.changed")]
    GameChanged(Box<ev::GameChanged>),
    /// At most five a second, never replayed.
    #[serde(rename = "game.thinking")]
    GameThinking(ev::GameThinking),
    #[serde(rename = "work.upserted")]
    WorkUpserted(Box<Work>),
    /// Never replayed.
    #[serde(rename = "work.log")]
    WorkLog(ev::WorkLog),
    #[serde(rename = "review.upserted")]
    ReviewUpserted(ev::ReviewUpserted),
    #[serde(rename = "connection.upserted")]
    ConnectionUpserted(Box<ConnectionStatus>),
    #[serde(rename = "challenge")]
    Challenge(Box<ChallengeEvent>),
    #[serde(rename = "policy.changed")]
    PolicyChanged(ev::Policy),
    /// Never replayed.
    #[serde(rename = "eval.info")]
    EvalInfo(ev::EvalInfo),
    #[serde(rename = "drill.upserted")]
    DrillUpserted(Box<chess_core::drills::Drill>),
    #[serde(rename = "drill.answered")]
    DrillAnswered(ev::DrillAnswered),
    #[serde(rename = "drill.deleted")]
    DrillDeleted(ev::DrillDeleted),
    #[serde(rename = "levels.updated")]
    LevelsUpdated(ev::LevelsUpdated),
    #[serde(rename = "engines.changed")]
    EnginesChanged(Box<EnginesOverview>),
    /// Not a domain event: the stream has a hole, so fetch what is shown again.
    #[serde(rename = "reload")]
    Reload(Reload),
}

/// The `reload` event's payload.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Reload {
    pub reason: String,
}

/// Every kind the stream carries, `reload` included, for the test that keeps [`EventData`] in step with `core`.
pub fn kinds() -> Vec<&'static str> {
    EventKind::ALL.iter().map(|k| k.as_str()).chain([RELOAD]).collect()
}
