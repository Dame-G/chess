import type { ThemeKind } from "../api/types";

/**
 * What a mistake of yours was about, in words: the vocabulary of `crates/core/src/themes.rs`. A name for the list and
 * the sheet, a singular for one drill, and the evidence each theme stands on, so the page never claims more than the
 * rule saw. Pins, skewers and positional ideas are not themes: they cannot be told from the engine's line cheaply.
 */
export interface ThemeWords {
  /** In a list: "Hanging pieces". */
  name: string;
  /** For one drill: "Hanging piece". */
  one: string;
  /** What the rule saw, in one line. */
  evidence: string;
}

export const THEMES: Record<ThemeKind, ThemeWords> = {
  time_trouble: {
    name: "Time trouble",
    one: "Time trouble",
    evidence: "Under a tenth of your clock was left.",
  },
  king_safety: {
    name: "King safety",
    one: "King safety",
    evidence: "The move allowed a forced mate, or moved the king and gave up castling.",
  },
  hanging_piece: {
    name: "Hanging pieces",
    one: "Hanging piece",
    evidence: "After the move a piece could be taken for less than it is worth, and the best move did not allow it.",
  },
  missed_tactic: {
    name: "Missed tactics",
    one: "Missed tactic",
    evidence: "The engine's line wins material or mates within a few moves, and the move missed it.",
  },
  opening_error: {
    name: "Opening errors",
    one: "Opening error",
    evidence: "A mistake in the opening phase that none of the above explains.",
  },
  endgame_technique: {
    name: "Endgame technique",
    one: "Endgame technique",
    evidence: "A mistake once few pieces were left that none of the above explains.",
  },
  other: {
    name: "Not classified",
    one: "Other",
    evidence: "Quiet mistakes that no rule above explains.",
  },
};

/** `Hanging piece · a knight`: a drill's theme with its cause, when the evidence names one. */
export function themeText(kind: ThemeKind, detail?: string | null): string {
  const one = THEMES[kind].one;
  return detail ? `${one} · ${detail}` : one;
}

/** Whether a theme names something to work on: `other` does not, so it has no drill button. */
export const trainable = (kind: ThemeKind): boolean => kind !== "other";
