import type { TimeControl } from "../api/types";

/** A verb typed in the CLI's words, read into what it asks for. */
export type Verb =
  /** `opponent`: `lichess`, an engine's id, or `null` for the analysis engine; `level` only for the analysis engine. */
  | { verb: "play"; time: TimeControl | null; opponent: string | null; level?: number }
  | { verb: "review"; last: number }
  | { verb: "compare"; players: string[] }
  | { verb: "drills" };

/** "5+3" (minutes + seconds) as a clock. */
export function readClock(text: string): TimeControl | null {
  const m = /^(\d+(?:\.\d+)?)\+(\d+(?:\.\d+)?)$/.exec(text);
  if (!m) return null;
  return {
    type: "clock",
    initial_ms: Math.round(Number(m[1]) * 60_000),
    increment_ms: Math.round(Number(m[2]) * 1000),
  };
}

/**
 * Reads what is typed in the command bar as one of the CLI's verbs: `play 5+3 stockfish`, `play stockfish@5`,
 * `play maia-1500`, `play lichess`, `review last 20`, `compare cutemouse83`, `drills`. `undefined` when it is not one.
 */
export function readVerb(query: string): Verb | undefined {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const [head, ...rest] = words;
  if (head === "play" || head === "p") {
    const time = rest.map(readClock).find((t) => t) ?? null;
    const words = rest.filter((w) => !readClock(w));
    const named = words.find((w) => !/^\d+$/.test(w));
    const [id, at] = (named ?? "").split("@");
    const number = at ?? words.find((w) => /^\d+$/.test(w));
    const opponent = !id ? null : id === "sf" ? "stockfish" : id;
    return { verb: "play", time, opponent, ...(number ? { level: Number(number) } : {}) };
  }
  if (head === "review") {
    const n = Number(rest.find((w) => /^\d+$/.test(w)) ?? 20);
    return { verb: "review", last: Math.min(200, Math.max(1, n)) };
  }
  if (head === "compare" && rest.length) return { verb: "compare", players: rest };
  if (head === "drills" || head === "drill") return { verb: "drills" };
  return undefined;
}
