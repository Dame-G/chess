import { ref } from "vue";
import { retryDelay } from "./backoff";
import { serverFetch } from "./client";
import type { AppEvent, EventKind, EventPayloads } from "./types";

/**
 * Splits a text/event-stream into events. Returns the events and whatever is left of an incomplete one. An event is
 * either `event: <kind>` with its payload as data, or the server's whole envelope `{ id, topic, kind, data }` as data.
 */
export function parseEvents(buffer: string): [AppEvent[], string] {
  const blocks = buffer.replace(/\r\n/g, "\n").split("\n\n");
  const rest = blocks.pop() ?? "";
  const events: AppEvent[] = [];
  for (const block of blocks) {
    let id = 0;
    let kind = "";
    const data: string[] = [];
    for (const line of block.split("\n")) {
      if (line.startsWith(":")) continue;
      const colon = line.indexOf(":");
      const field = colon < 0 ? line : line.slice(0, colon);
      const text = colon < 0 ? "" : line.slice(colon + 1).replace(/^ /, "");
      if (field === "id") id = Number(text);
      else if (field === "event") kind = text;
      else if (field === "data") data.push(text);
    }
    if (!data.length) continue;
    let payload: unknown;
    try {
      payload = JSON.parse(data.join("\n"));
    } catch {
      continue;
    }
    const envelope = payload as Partial<AppEvent> | null;
    if (envelope && typeof envelope === "object" && "topic" in envelope && "kind" in envelope && "data" in envelope) {
      events.push({ ...envelope, id: envelope.id ?? id } as AppEvent);
    } else if (kind) {
      events.push({ id, topic: "", kind, data: payload } as AppEvent);
    }
  }
  return [events, rest];
}

/** Whether the server answers: false once the stream has failed twice in a row, true again as soon as it connects. */
export const reachable = ref(true);

type Handler<K extends EventKind> = (data: EventPayloads[K], event: AppEvent) => void;
const handlers = new Map<EventKind, Set<Handler<never>>>();
const reconnectHandlers = new Set<() => void>();

/** Listens for one kind of event. Stores call it once, when they are created. Returns a function that stops it. */
export function onEvent<K extends EventKind>(kind: K, handler: Handler<K>): () => void {
  const set = handlers.get(kind) ?? new Set();
  handlers.set(kind, set);
  set.add(handler as Handler<never>);
  return () => set.delete(handler as Handler<never>);
}

/**
 * Runs when what was missed may not have been replayed: after the stream comes back from a drop, and on the server's
 * `reload` event (the events after the last id are no longer kept, or this client fell behind). Stores reload.
 */
export function onReconnect(handler: () => void): () => void {
  reconnectHandlers.add(handler);
  return () => reconnectHandlers.delete(handler);
}

/** Hands an event to everything listening for its kind. */
export function dispatch(event: AppEvent) {
  for (const handler of handlers.get(event.kind) ?? [])
    (handler as Handler<typeof event.kind>)(event.data as never, event);
}

/**
 * The topics every page follows: game summaries and reviews, work, fair play, your Lichess connection, your drills, and the
 * engines list with the levels. The open game and an evaluation session are
 * added as they come and go.
 */
const BASE_TOPICS = ["games", "work", "policy", "connection:you", "player:you", "engines"];

/**
 * The one connection to `GET /api/events`. A fetch stream rather than EventSource, so the token travels in a header.
 * It resumes from the last id it saw after a drop, and after a change of topics (the open game, an evaluation session),
 * so nothing falls in the gap.
 */
class Stream {
  private extra = new Set<string>();
  private last: number | undefined;
  private attempt = new AbortController();
  private running = false;
  private stopped = false;

  /** Opens the stream; `since` is the last event already accounted for (the status's `next_event` − 1). */
  start(retryMs = 1000, since?: number) {
    if (this.running) return;
    this.running = true;
    if (since !== undefined && since >= 0) this.last = since;
    void this.loop(retryMs);
  }

  stop() {
    this.stopped = true;
    this.attempt.abort();
  }

  /** Follows one more topic, such as `game:12` or `eval:<session>`. */
  follow(topic: string) {
    if (this.extra.has(topic)) return;
    this.extra.add(topic);
    this.attempt.abort();
  }

  unfollow(topic: string) {
    if (!this.extra.delete(topic)) return;
    this.attempt.abort();
  }

  private url() {
    const topics = [...BASE_TOPICS, ...this.extra].join(",");
    const since = this.last === undefined ? "" : `&since=${this.last}`;
    return `/api/events?topics=${encodeURIComponent(topics)}${since}`;
  }

  private async loop(retryMs: number) {
    let dropped = false;
    // Failures in a row: the wait doubles with each, up to ten seconds, and starts over once the server answers.
    let failures = 0;
    while (!this.stopped) {
      this.attempt = new AbortController();
      const signal = this.attempt.signal;
      try {
        const res = await serverFetch(this.url(), { signal, headers: { Accept: "text/event-stream" } });
        if (!res.ok || !res.body) throw new Error(String(res.status));
        failures = 0;
        reachable.value = true;
        if (dropped) for (const h of reconnectHandlers) h();
        dropped = false;
        const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
        let buffer = "";
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          const [events, rest] = parseEvents(buffer + value);
          buffer = rest;
          for (const event of events) {
            if (event.id) this.last = event.id;
            if (event.kind === "reload") for (const h of reconnectHandlers) h();
            else dispatch(event);
          }
        }
        // The server closed the stream: a drop like any other.
        dropped = true;
      } catch {
        if (!signal.aborted) {
          dropped = true;
          failures++;
          if (failures >= 2) reachable.value = false;
        }
      }
      // A change of topics reconnects at once; a lost server after a pause that grows while it stays lost.
      if (!signal.aborted && !this.stopped)
        await new Promise((r) => setTimeout(r, retryDelay(Math.max(0, failures - 1), Math.random(), retryMs)));
    }
  }
}

export const stream = new Stream();
