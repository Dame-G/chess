import type { Drill, DrillAnswerResult, DrillId, DrillsToday, GameId, Side, ThemeKind } from "../api/types";
import { THEMES } from "./themes";

/**
 * What the board needs to run a drill: no engine and no eval, one try at the position, then the answer. These are the
 * pure rules of that flow; `stores/drills.ts` holds the state and talks to the server.
 */

/** A drill before its answer: the server sends the question only, and the page shows nothing more until it is tried. */
export interface DrillQuestion {
  id: DrillId;
  fen: string;
  side: Side;
  /** The game it came from and the move it was in, for the link once it is answered. */
  game: Drill["game"];
  ply: number;
  opening: string | null;
  phase: Drill["theme"]["phase"];
  /** How many times it has been answered, to tell a new drill from one coming back. */
  reps: number;
}

/**
 * The question is the position, whose move it is, and where it came from: not the theme, which would say what kind of
 * move to look for.
 */
export function questionOf(d: Drill): DrillQuestion {
  return {
    id: d.id,
    fen: d.fen,
    side: d.side,
    game: d.game,
    ply: d.ply,
    opening: d.theme.opening ?? null,
    phase: d.theme.phase,
    reps: d.reps,
  };
}

/** The sentence over the board: whose move it is and what to find. */
export function prompt(q: Pick<DrillQuestion, "side">): string {
  return `${q.side === "white" ? "White" : "Black"} to move: find the move you missed`;
}

/**
 * The drill to ask next: the first of today's set that has not been tried in this sitting, after `after` if given, so
 * a drill that was skipped is not asked again straight away. `null` when the set is done.
 */
export function nextQuestion(
  queue: readonly Drill[],
  tried: ReadonlySet<DrillId>,
  after: DrillId | null = null,
): Drill | null {
  const at = after === null ? -1 : queue.findIndex((d) => d.id === after);
  const ahead = queue.slice(at + 1).find((d) => !tried.has(d.id));
  return ahead ?? queue.find((d) => !tried.has(d.id)) ?? null;
}

/** How a sitting stands: tried so far, passed, and what is left. */
export interface Sitting {
  tried: number;
  passed: number;
  left: number;
  total: number;
}

export function sitting(queue: readonly Drill[], outcomes: ReadonlyMap<DrillId, "pass" | "fail">): Sitting {
  let passed = 0;
  for (const o of outcomes.values()) if (o === "pass") passed++;
  const tried = queue.filter((d) => outcomes.has(d.id)).length;
  return { tried, passed, left: queue.length - tried, total: queue.length };
}

/** "3 of 8" for the drill on the board, and "8 of 8 done: 6 passed" once the last one is answered. */
export function sittingText(s: Sitting, answered = false): string {
  if (s.total === 0) return "";
  if (answered && s.left === 0) return `${s.total} of ${s.total} done: ${s.passed} passed`;
  return `${Math.min(s.total, answered ? s.tried : s.tried + 1)} of ${s.total}`;
}

/** When a drill comes back, in words: "tomorrow", "in 6 days". */
export function comesBack(interval: number): string {
  if (interval <= 1) return "tomorrow";
  return `in ${interval} days`;
}

/** The sentence once a drill is answered. */
export function verdict(r: Pick<DrillAnswerResult, "outcome" | "best_san" | "played_san" | "drill">): string {
  const best = r.best_san.join(" or ");
  return r.outcome === "pass"
    ? `Yes, ${r.played_san}. It comes back ${comesBack(r.drill.interval_days)}.`
    : `Not ${r.played_san}. ${best} was the move. It comes back ${comesBack(r.drill.interval_days)}.`;
}

/** The arrow to draw over the board once the drill is answered: the best move, from the line the server gave. */
export function answerArrow(r: Pick<DrillAnswerResult, "best_moves" | "line">): string | null {
  return r.best_moves[0] ?? r.line[0] ?? null;
}

/** Today's set in words for the You sheet: "5 today: 3 due, 2 new", "Nothing due today". */
export function todayText(t: Pick<DrillsToday, "due" | "new" | "drills">): string {
  if (t.drills.length === 0) return "Nothing due today";
  const parts = [t.due ? `${t.due} due` : "", t.new ? `${t.new} new` : ""].filter(Boolean);
  return parts.length ? `${t.drills.length} today: ${parts.join(", ")}` : `${t.drills.length} today`;
}

// ---- What a sitting is of

/** What a sitting is made of: the day's own set, one theme, or the games of one opening line. */
export type Scope =
  | { type: "day" }
  | { type: "theme"; theme: ThemeKind }
  | { type: "line"; name: string; games: GameId[]; maxPly: number };

/** Its name over the board and on the summary: "Today", "Hanging pieces", "Italian Game". */
export function scopeTitle(s: Scope): string {
  return s.type === "day" ? "Today" : s.type === "theme" ? THEMES[s.theme].name : s.name;
}

/** The query the server takes for it. */
export function scopeQuery(s: Scope): { theme?: ThemeKind; games?: string; max_ply?: number } {
  if (s.type === "theme") return { theme: s.theme };
  if (s.type === "line") return { games: s.games.join(","), max_ply: s.maxPly };
  return {};
}

// ---- Hints

/** The square to mark on the board: where the piece of the second hint stands, when all the right moves start there. */
export function hintSquare(hints: readonly { level: number; from?: string | null }[]): string | null {
  return hints.find((h) => h.level >= 2)?.from ?? null;
}

// ---- Progress through a sitting

/** One drill of the sitting in the progress strip: what came of it, or that it is on the board or still to come. */
export type Pip = "pass" | "half" | "fail" | "now" | "todo";

/** Credit an answer is worth in a hit rate: 1 a clean pass, one half a pass with a hint, 0 a miss. */
function creditOf(r: Pick<DrillAnswerResult, "outcome" | "hinted">): number {
  return r.outcome === "fail" ? 0 : r.hinted ? 0.5 : 1;
}

/** A pip for each drill of the queue, in order. */
export function pips(
  queue: readonly Drill[],
  results: ReadonlyMap<DrillId, Pick<DrillAnswerResult, "outcome" | "hinted">>,
  active: DrillId | null,
): Pip[] {
  return queue.map((d) => {
    const r = results.get(d.id);
    if (r) return r.outcome === "fail" ? "fail" : r.hinted ? "half" : "pass";
    return d.id === active ? "now" : "todo";
  });
}

/** The run of clean passes at the end of the answers, in the order they were given: a hint or a miss ends it. */
export function streak(
  order: readonly DrillId[],
  results: ReadonlyMap<DrillId, Pick<DrillAnswerResult, "outcome" | "hinted">>,
): number {
  let n = 0;
  for (let i = order.length - 1; i >= 0; i--) {
    const r = results.get(order[i]!);
    if (!r || creditOf(r) < 1) break;
    n++;
  }
  return n;
}

// ---- The summary

/** How a kind of mistake went in a sitting. */
interface ThemeRow {
  kind: ThemeKind;
  total: number;
  /** Credit earned: clean passes count one, hinted passes a half. */
  credit: number;
  /** The soonest any of them comes back, in days. */
  back: number;
}

export interface Summary {
  total: number;
  clean: number;
  hinted: number;
  missed: number;
  /** Credit per drill, 0 to 1. */
  rate: number;
  /** By theme, the ones with the most misses first. */
  themes: ThemeRow[];
}

/** A sitting added up, from the answers in the order given. */
export function summarise(results: readonly DrillAnswerResult[]): Summary {
  const rows = new Map<ThemeKind, ThemeRow>();
  let clean = 0;
  let hinted = 0;
  let missed = 0;
  let credit = 0;
  for (const r of results) {
    const c = creditOf(r);
    credit += c;
    if (c === 1) clean++;
    else if (c === 0.5) hinted++;
    else missed++;
    const kind = r.drill.theme.kind;
    const row = rows.get(kind) ?? { kind, total: 0, credit: 0, back: Infinity };
    row.total++;
    row.credit += c;
    row.back = Math.min(row.back, Math.max(1, r.drill.interval_days));
    rows.set(kind, row);
  }
  const themes = [...rows.values()].sort(
    (a, b) => b.total - b.credit - (a.total - a.credit) || b.total - a.total || a.kind.localeCompare(b.kind),
  );
  return {
    total: results.length,
    clean,
    hinted,
    missed,
    rate: results.length ? credit / results.length : 0,
    themes,
  };
}
