use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::sse::{Event as SseEvent, KeepAlive, Sse},
};
use futures::{Stream, StreamExt, stream};
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use utoipa::IntoParams;

use super::AppState;
use crate::events::Event;

#[derive(Deserialize, IntoParams)]
pub struct EventsQuery {
    /// Replay events after this id. `Last-Event-ID` does the same for a reconnecting browser.
    since: Option<u64>,
}

/// Live events as server-sent events: every event has an id, and reconnecting with the last id seen replays what was
/// missed.
#[utoipa::path(
    get,
    path = "/api/events",
    operation_id = "follow_events",
    tag = "runner",
    params(EventsQuery),
    responses((status = 200, description = "A text/event-stream of `Event`s", content_type = "text/event-stream", body = Event))
)]
pub async fn events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<EventsQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let last = headers.get("last-event-id").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());
    let (backlog, rx) = state.events.subscribe(last.or(query.since).unwrap_or(0));
    // A slow reader that falls behind the channel skips ahead rather than ending the stream.
    let live = BroadcastStream::new(rx).filter_map(|e| async move { e.ok() });
    let stream = stream::iter(backlog).chain(live).map(|e| Ok(to_sse(&e)));
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn to_sse(e: &Event) -> SseEvent {
    SseEvent::default().id(e.id.to_string()).event(&e.kind).data(e.data.to_string())
}
