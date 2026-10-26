import { onScopeDispose } from "vue";
import { connection, runnerFetch } from "./runner";

export interface RunnerEvent {
  id: number;
  kind: string;
  data: unknown;
}

/** Splits a text/event-stream into events. Returns the events and whatever is left of an incomplete one. */
export function parseEvents(buffer: string): [RunnerEvent[], string] {
  const blocks = buffer.split("\n\n");
  const rest = blocks.pop() ?? "";
  const events: RunnerEvent[] = [];
  for (const block of blocks) {
    let id = 0;
    let kind = "message";
    const data: string[] = [];
    for (const line of block.split("\n")) {
      const [field, ...value] = line.split(":");
      const text = value.join(":").replace(/^ /, "");
      if (field === "id") id = Number(text);
      else if (field === "event") kind = text;
      else if (field === "data") data.push(text);
    }
    if (!data.length) continue;
    try {
      events.push({ id, kind, data: JSON.parse(data.join("\n")) });
    } catch {
      events.push({ id, kind, data: data.join("\n") });
    }
  }
  return [events, rest];
}

/** Asks the runner for no backlog: the stream starts with the next event. */
const FROM_NOW = Number.MAX_SAFE_INTEGER;

/**
 * Follows the runner's live events. A fetch stream rather than EventSource, so the token travels in a header.
 *
 * The first connection starts from now: the page fetches what it shows, so the runner's backlog would only replay
 * history (old log lines, a burst of refetches). After a dropped or refused connection it resumes from the last id
 * seen, and first hands over a `runner.started` event (id 0): a runner that (re)started counts its ids from 1 again
 * and what it said before the page reconnected is gone, so listeners refetch everything. Returns a function that
 * stops it.
 */
export function followEvents(onEvent: (event: RunnerEvent) => void, retryMs = 2000): () => void {
  const abort = new AbortController();
  let last: number | undefined;
  let tries = 0;

  (async () => {
    while (!abort.signal.aborted) {
      tries++;
      try {
        const res = await runnerFetch(`${connection.url}/api/events?since=${last ?? FROM_NOW}`, {
          signal: abort.signal,
        });
        if (!res.ok || !res.body) throw new Error(String(res.status));
        if (tries > 1) onEvent({ id: 0, kind: "runner.started", data: { resumed: true } });
        const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
        let buffer = "";
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          const [events, rest] = parseEvents(buffer + value);
          buffer = rest;
          for (const event of events) {
            // The newest id rather than the largest: after a restart the ids start again.
            if (event.id) last = event.id;
            onEvent(event);
          }
        }
      } catch {}
      if (!abort.signal.aborted) await new Promise((r) => setTimeout(r, retryMs));
    }
  })();

  return () => abort.abort();
}

type Handler = (data: unknown, event: RunnerEvent) => void;
const handlers = new Map<string, Set<Handler>>();

/** Hands an event to everything listening for its kind. The app follows the stream once and dispatches here. */
export function dispatch(event: RunnerEvent) {
  for (const handler of handlers.get(event.kind) ?? []) handler(event.data, event);
}

/** Listens for one kind of event while the calling component is mounted. */
export function useRunnerEvent(kind: string, handler: Handler) {
  const set = handlers.get(kind) ?? new Set();
  handlers.set(kind, set);
  set.add(handler);
  onScopeDispose(() => set.delete(handler));
}
