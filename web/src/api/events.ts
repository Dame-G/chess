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

/**
 * Follows the runner's live events. A fetch stream rather than EventSource, so the token travels in a header; after a
 * dropped connection it reconnects from the last id seen, so nothing is missed. Returns a function that stops it.
 */
export function followEvents(onEvent: (event: RunnerEvent) => void): () => void {
  const abort = new AbortController();
  let last = 0;

  (async () => {
    while (!abort.signal.aborted) {
      try {
        const res = await runnerFetch(`${connection.url}/api/events?since=${last}`, { signal: abort.signal });
        if (!res.ok || !res.body) throw new Error(String(res.status));
        const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
        let buffer = "";
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          const [events, rest] = parseEvents(buffer + value);
          buffer = rest;
          for (const event of events) {
            last = Math.max(last, event.id);
            onEvent(event);
          }
        }
      } catch {}
      if (!abort.signal.aborted) await new Promise((r) => setTimeout(r, 2000));
    }
  })();

  return () => abort.abort();
}
