/** What a game review says about each move, in the words and marks the board uses. The runner judges; this only reads. */
import type { components } from "../api/schema";
import { signed } from "../format";

export type Review = components["schemas"]["Review"];
export type PlyReview = components["schemas"]["PlyReview"];
export type Classification = components["schemas"]["Classification"];
export type ReviewScore = components["schemas"]["Score"];
export type ReviewSide = components["schemas"]["SideReview"];

export interface Mark {
  /** What follows the move in the list: `??`, `?!`. Empty for a move that earns no mark. */
  glyph: string;
  label: string;
  /** The colour class (`rv-blunder`), or nothing for a move the list keeps quiet. */
  tone: Classification | null;
}

const MARKS: Record<Classification, Mark> = {
  brilliant: { glyph: "!!", label: "Brilliant", tone: "brilliant" },
  great: { glyph: "!", label: "Great move", tone: "great" },
  best: { glyph: "", label: "Best move", tone: null },
  excellent: { glyph: "", label: "Excellent", tone: null },
  good: { glyph: "", label: "Good", tone: null },
  book: { glyph: "", label: "Book move", tone: null },
  inaccuracy: { glyph: "?!", label: "Inaccuracy", tone: "inaccuracy" },
  mistake: { glyph: "?", label: "Mistake", tone: "mistake" },
  blunder: { glyph: "??", label: "Blunder", tone: "blunder" },
  miss: { glyph: "×", label: "Missed chance", tone: "miss" },
};
export const markOf = (c: Classification): Mark => MARKS[c];

/** The marks that count on the summary, in the order Lichess lists them. */
export const SUMMARY_MARKS = ["brilliant", "great", "inaccuracy", "mistake", "blunder", "miss"] as const;

/** A move the player could have done better: the ones the board draws the best move for and "next mistake" stops at. */
export const isError = (c: Classification): boolean =>
  c === "inaccuracy" || c === "mistake" || c === "blunder" || c === "miss";

/** A score as White's edge in pawns. A mate counts as far more than any material, and the mover wins a mate of 0. */
export function pawnsOf(s: ReviewScore, mover: "white" | "black"): number {
  if ("cp" in s) return s.cp / 100;
  if (s.mate === 0) return mover === "white" ? 100 : -100;
  return s.mate > 0 ? 100 : -100;
}

/** A score as people write it: `+0.45`, `−1.20`, `+M3`. */
export function scoreText(s: ReviewScore, mover: "white" | "black"): string {
  if ("cp" in s) return signed(s.cp / 100);
  if (s.mate === 0) return mover === "white" ? "Mate" : "−Mate";
  return `${s.mate > 0 ? "+" : "−"}M${Math.abs(s.mate)}`;
}

/** The one line shown for a move: its name, and for a move that could be better what was. */
export function hint(p: PlyReview): string {
  const m = markOf(p.classification);
  const best = `Best was ${p.best_san} (${scoreText(p.eval_before, p.side)})`;
  switch (p.classification) {
    case "brilliant":
      return p.sacrifice >= 1
        ? `Brilliant: gives up about ${Math.round(p.sacrifice)} ${Math.round(p.sacrifice) === 1 ? "pawn" : "pawns"} of material and still holds the advantage`
        : "Brilliant: a sacrifice that works";
    case "great":
      return "Great move: the only move that keeps the position";
    case "inaccuracy":
    case "mistake":
    case "blunder":
    case "miss":
      return p.best_uci === p.uci ? m.label : `${m.label}. ${best}`;
    default:
      return m.label;
  }
}

/** Whether the engine had a better move than a judged one, which the board then shows and offers to play. */
export const hasBetter = (p: PlyReview): boolean => isError(p.classification) && p.best_uci !== p.uci;

/**
 * The ply (1-based) of the next move by `side` (or either) that earns `want`, after `from` (and before it when `back`),
 * wrapping at the ends so the key never dead-ends.
 */
export function findPly(
  plies: readonly PlyReview[],
  from: number,
  want: (p: PlyReview) => boolean,
  back = false,
): number | undefined {
  const n = plies.length;
  for (let k = 1; k <= n; k++) {
    const i = back ? (((from - 1 - k) % n) + n) % n : (from - 1 + k) % n;
    const p = plies[i];
    if (p && want(p)) return p.ply;
  }
  return undefined;
}

/** The graph's values and marks, at each position from the start: White's edge as a share of a win, from the review's scores. */
export function reviewSeries(plies: readonly PlyReview[], gauge: (pawns: number) => number) {
  const values: (number | null)[] = [];
  const texts: (string | null)[] = [];
  const first = plies[0];
  if (!first) return { values, texts };
  values.push(gauge(pawnsOf(first.eval_before, "white")));
  texts.push(scoreText(first.eval_before, "white"));
  for (const p of plies) {
    values.push(gauge(pawnsOf(p.eval_after, p.side)));
    texts.push(scoreText(p.eval_after, p.side));
  }
  return { values, texts };
}
