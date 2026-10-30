import type { TimeControl } from "../api/types";

/** A verb typed in the CLI's words, read into what it asks for. */
export type Verb =
  | { verb: "play"; time: TimeControl | null; opponent: "dameless" | "stockfish" | "lichess"; level?: number }
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
 * Reads what is typed in the command bar as one of the CLI's verbs: `play 5+3 dameless`, `play stockfish 5`,
 * `review last 20`, `compare cutemouse83`, `drills`. `undefined` when it is not one.
 */
export function readVerb(query: string): Verb | undefined {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const [head, ...rest] = words;
  if (head === "play" || head === "p") {
    const time = rest.map(readClock).find((t) => t) ?? null;
    if (rest.includes("stockfish") || rest.includes("sf")) {
      const level = Number(rest.find((w) => /^[1-8]$/.test(w)) ?? 5);
      return { verb: "play", time, opponent: "stockfish", level };
    }
    if (rest.includes("lichess")) return { verb: "play", time, opponent: "lichess" };
    return { verb: "play", time, opponent: "dameless" };
  }
  if (head === "review") {
    const n = Number(rest.find((w) => /^\d+$/.test(w)) ?? 20);
    return { verb: "review", last: Math.min(200, Math.max(1, n)) };
  }
  if (head === "compare" && rest.length) return { verb: "compare", players: rest };
  if (head === "drills" || head === "drill") return { verb: "drills" };
  return undefined;
}
