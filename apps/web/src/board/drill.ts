import type { Drill, DrillAnswerResult, DrillId, DrillsToday, Side } from "../api/types";

/**
 * What the board needs to run a drill: no engine and no eval, one try at the position, then the answer. These are the
 * pure rules of that flow; `stores/drills.ts` holds the state and talks to the server.
 */

/** A drill before its answer: the server sends the answer with it, and the page must not show it until it is tried. */
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
