import type { PrepLine } from "../api/types";
import { formatPercent, plural } from "../format";

/** Mistakes made by this ply count as early: the first twelve moves (`core::prep::EARLY_PLY`). */
export const EARLY_PLY = 24;

/** Why a line is flagged, in the words the page uses; empty for a line that is fine. */
export function flags(line: Pick<PrepLine, "scores_badly" | "goes_wrong_early">): string[] {
  return [line.scores_badly ? "Scores badly" : "", line.goes_wrong_early ? "Goes wrong early" : ""].filter(Boolean);
}

/** Whether the line needs work: either flag. */
export const flagged = (line: Pick<PrepLine, "scores_badly" | "goes_wrong_early">): boolean => flags(line).length > 0;

/** Your points per decided game, as a percentage: `63%`, and a dash when nothing was decided. */
export const scoreText = (line: Pick<PrepLine, "score">): string =>
  line.score == null ? "–" : formatPercent(line.score * 100, 0);

/** `4 W · 1 D · 2 L`. */
export const recordText = (line: Pick<PrepLine, "wins" | "draws" | "losses">): string =>
  `${line.wins} W · ${line.draws} D · ${line.losses} L`;

/** How often it goes wrong early: `3 of 9 games`, or that nothing was reviewed to say. */
export function earlyText(line: Pick<PrepLine, "reviewed" | "early_games">): string {
  if (line.reviewed === 0) return "not reviewed";
  return `${line.early_games} of ${plural(line.reviewed, "game")}`;
}

/** The lines that need work first (flagged, most games first), then the rest as they came (most played first). */
export function byNeed<T extends Pick<PrepLine, "scores_badly" | "goes_wrong_early" | "games">>(
  lines: readonly T[],
): T[] {
  return [...lines].sort((a, b) => Number(flagged(b)) - Number(flagged(a)) || b.games - a.games);
}
