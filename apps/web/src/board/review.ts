/** What a game review says about each move, in the words and marks the board uses. The server judges; this only reads. */
import type { Classification, ReviewedMove, Side } from "../api/types";
import { scoreText } from "../format";

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

/** The marks the summary counts, in the order Lichess lists them. */
export const SUMMARY_MARKS = ["brilliant", "great", "inaccuracy", "mistake", "blunder", "miss"] as const;

/** A move the player could have done better: the ones the board draws the better move for and X stops at. */
const isError = (c: Classification): boolean =>
  c === "inaccuracy" || c === "mistake" || c === "blunder" || c === "miss";

/** The one line shown for a move: its name, and for a move that could be better, what was. */
export function hint(p: ReviewedMove): string {
  const m = markOf(p.classification);
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
      return p.best_uci === p.uci ? m.label : `${m.label}. Best was ${p.best_san} (${scoreText(p.eval_before)})`;
    default:
      return m.label;
  }
}

/** Whether the engine had a better move than a judged one, which the board then shows. */
export const hasBetter = (p: ReviewedMove): boolean => isError(p.classification) && p.best_uci !== p.uci;

/**
 * The ply (1-based) of the next move by `side` (or either) that `want`s, after `from` (before it when `back`), wrapping
 * at the ends so the key never dead-ends.
 */
export function findPly(
  plies: readonly ReviewedMove[],
  from: number,
  want: (p: ReviewedMove) => boolean,
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

/** The side whose mistakes X walks: yours in your game, else both. */
export const walksSide = (you: Side | null) => (p: ReviewedMove) =>
  isError(p.classification) && (!you || p.side === you);
